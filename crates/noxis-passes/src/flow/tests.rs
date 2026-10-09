//! Unit tests for the control-flow pass: shapes, skips, determinism.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{
    Arith, BranchCond, ClassBuilder, Code, Cond, Const, Insn, Prim, Program, SlotKind, Version,
};
use noxis_core::{Config, Context, FlowConfig, Pass, PassId};

use super::{FlowPass, LabelGen};

/// A small branchy method: `acc=0; for(i=0;i<10;i++) acc+=i; return acc`.
fn branchy_code() -> Code {
    let (head, end) = (100, 101);
    Code {
        max_stack: 2,
        max_locals: 2,
        insns: vec![
            Insn::Const(Const::Int(0)),
            Insn::Store {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Const(Const::Int(0)),
            Insn::Store {
                slot: 1,
                kind: SlotKind::Int,
            },
            Insn::Mark(head),
            Insn::Load {
                slot: 1,
                kind: SlotKind::Int,
            },
            Insn::Const(Const::Int(10)),
            Insn::Branch(BranchCond::Int(Cond::Ge), end),
            Insn::Load {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Load {
                slot: 1,
                kind: SlotKind::Int,
            },
            Insn::Binary {
                prim: Prim::Int,
                op: Arith::Add,
            },
            Insn::Store {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Iinc { index: 1, delta: 1 },
            Insn::Goto(head),
            Insn::Mark(end),
            Insn::Load {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Return(Some(Prim::Int)),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    }
}

fn program_with(methods: Vec<(String, String, Code)>) -> Program {
    let mut builder = ClassBuilder::new("demo/Flow", Some("java/lang/Object"));
    for (name, descriptor, code) in methods {
        builder = builder.method_with_code(0x0009, name, descriptor, code);
    }
    Program {
        classes: vec![builder.version(Version::JAVA_21).build()],
        resources: Vec::new(),
    }
}

fn run(program: &mut Program, schemes: &[&str]) -> noxis_core::PassReport {
    let config = Config {
        seed: Some(42),
        flow: FlowConfig {
            schemes: schemes.iter().map(|s| s.to_string()).collect(),
            max_insns: 4000,
            encrypt_block_bodies: false,
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    FlowPass.apply(&mut ctx, program).expect("flow pass")
}

fn code_of(program: &Program, name: &str) -> Code {
    program.classes[0]
        .method(name, "()I")
        .expect("method")
        .code
        .clone()
        .expect("code")
}

fn has_switch(insns: &[Insn]) -> bool {
    insns
        .iter()
        .any(|i| matches!(i, Insn::TableSwitch { .. } | Insn::LookupSwitch { .. }))
}

#[test]
fn state_spread_emits_table_dispatch() {
    let mut program = program_with(vec![(
        "calc".to_string(),
        "()I".to_string(),
        branchy_code(),
    )]);
    let before = code_of(&program, "calc").insns.len();
    let report = run(&mut program, &["state-spread"]);
    let code = code_of(&program, "calc");
    assert!(
        code.insns
            .iter()
            .any(|i| matches!(i, Insn::TableSwitch { .. })),
        "no tableswitch in {code:?}"
    );
    assert!(code.insns.len() > before, "stream did not grow");
    assert!(!code.frames.is_empty(), "frames were not recomputed");
    assert_eq!(report.pass, PassId::Flow);
    assert_eq!(report.transformed, 1);
}

#[test]
fn computed_switch_uses_lookup_and_fresh_local() {
    let mut program = program_with(vec![(
        "calc".to_string(),
        "()I".to_string(),
        branchy_code(),
    )]);
    run(&mut program, &["computed-switch"]);
    let code = code_of(&program, "calc");
    let pairs = code.insns.iter().find_map(|i| match i {
        Insn::LookupSwitch { pairs, .. } => Some(pairs.clone()),
        _ => None,
    });
    let pairs = pairs.expect("no lookupswitch");
    let mut sorted = pairs.clone();
    sorted.sort_by_key(|(k, _)| *k);
    assert_eq!(pairs, sorted, "lookupswitch pairs must be sorted");
    assert_eq!(code.max_locals, 3, "counter local was not allocated");
}

#[test]
fn butterfly_inverts_without_switch() {
    let mut program = program_with(vec![(
        "calc".to_string(),
        "()I".to_string(),
        branchy_code(),
    )]);
    run(&mut program, &["butterfly"]);
    let code = code_of(&program, "calc");
    assert!(!has_switch(&code.insns), "butterfly must not dispatch");
    let gotos = code
        .insns
        .iter()
        .filter(|i| matches!(i, Insn::Goto(_)))
        .count();
    assert!(
        gotos >= 3,
        "expected inverted branches + splits, got {gotos}"
    );
    let labels = code.labels();
    for insn in &code.insns {
        for target in insn.targets() {
            assert!(labels.contains(&target), "dangling label {target}");
        }
    }
    // Every branch target must also be reachable: an orphaned splice leaves
    // its marks without computed frames and HotSpot rejects the class.
    let live = super::rewire::reachable(&code.insns);
    let mut at_mark = std::collections::BTreeMap::new();
    for (index, insn) in code.insns.iter().enumerate() {
        if let Insn::Mark(label) = insn {
            at_mark.entry(*label).or_insert(index);
        }
    }
    for insn in &code.insns {
        for target in insn.targets() {
            let index = at_mark.get(&target).expect("resolved above");
            assert!(live[*index], "unreachable branch target {target}");
        }
    }
}

#[test]
fn exception_goto_adds_catch_all() {
    let mut program = program_with(vec![(
        "calc".to_string(),
        "()I".to_string(),
        branchy_code(),
    )]);
    run(&mut program, &["exception-goto"]);
    let code = code_of(&program, "calc");
    assert_eq!(code.exception_table.len(), 1);
    assert_eq!(code.exception_table[0].catch_type, None);
    assert!(
        code.insns
            .iter()
            .any(|i| matches!(i, Insn::TableSwitch { .. })),
        "dispatcher missing inside try"
    );
}

#[test]
fn skips_init_and_tiny_methods() {
    let tiny = Code {
        max_stack: 0,
        max_locals: 0,
        insns: vec![Insn::Return(None)],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    let mut program = program_with(vec![
        ("<init>".to_string(), "()V".to_string(), tiny.clone()),
        ("run".to_string(), "()V".to_string(), tiny),
        ("calc".to_string(), "()I".to_string(), branchy_code()),
    ]);
    let report = run(
        &mut program,
        &[
            "state-spread",
            "computed-switch",
            "butterfly",
            "exception-goto",
        ],
    );
    assert_eq!(report.transformed, 1);
    assert_eq!(report.skipped, 2);
}

#[test]
fn unknown_scheme_is_rejected() {
    let mut program = program_with(vec![(
        "calc".to_string(),
        "()I".to_string(),
        branchy_code(),
    )]);
    let config = Config {
        seed: Some(1),
        flow: FlowConfig {
            schemes: vec!["death-star".to_string()],
            max_insns: 4000,
            encrypt_block_bodies: false,
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    assert!(FlowPass.apply(&mut ctx, &mut program).is_err());
}

#[test]
fn labels_are_allocated_above_existing_ones() {
    let code = branchy_code();
    let top = code.labels().into_iter().max().unwrap_or(0);
    let mut labels = LabelGen::of(&code);
    assert!(labels.next() > top);
}
