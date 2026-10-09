//! Constant-pass tests: forms compute the source value, trivia survives.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{ClassBuilder, Code, Const, Insn, MAX_METHOD_INSNS, Prim, Program, Version};
use noxis_core::{Config, Context, Pass};

use super::ConstantsPass;
use super::forms::{Scheme, encrypt_int, encrypt_long};

/// Forms round-trip arithmetically (an interpreter for triples/quintuples).
fn eval_int(insns: &[Insn]) -> i32 {
    match insns {
        [
            Insn::Const(Const::Int(a)),
            Insn::Const(Const::Int(b)),
            Insn::Binary { op, .. },
        ] => {
            use noxis_classfile::Arith::{Add, Xor};
            match op {
                Xor => a ^ b,
                Add => a.wrapping_add(*b),
                _ => panic!("not a triple: {insns:?}"),
            }
        }
        [
            Insn::Const(Const::Int(a)),
            Insn::Const(Const::Int(1)),
            Insn::Binary { .. },
            Insn::Const(Const::Int(b)),
            Insn::Binary { .. },
        ] => (a << 1) | (b & 1),
        _ => panic!("not a form: {insns:?}"),
    }
}

#[test]
fn int_forms_round_trip() {
    let mut rng = noxis_core::Rng::from_seed(11);
    for value in [-1000, -7, -6, 6, 7, 42, 65536, i32::MAX, i32::MIN] {
        for scheme in [Scheme::Xor, Scheme::Add, Scheme::Shift] {
            assert_eq!(eval_int(&encrypt_int(value, scheme, &mut rng)), value);
        }
    }
}

#[test]
fn long_forms_keep_shape() {
    let mut rng = noxis_core::Rng::from_seed(12);
    for scheme in [Scheme::Xor, Scheme::Add, Scheme::Shift] {
        let insns = encrypt_long(1_000_000_000_000, scheme, &mut rng);
        assert!(matches!(
            insns.last(),
            Some(Insn::Binary {
                prim: Prim::Long,
                ..
            })
        ));
    }
}

fn sample() -> Program {
    let code = Code {
        max_stack: 1,
        max_locals: 0,
        insns: vec![
            Insn::Const(Const::Int(100)),
            Insn::Const(Const::Int(3)),
            Insn::Const(Const::Long(10)),
            Insn::Return(None),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    Program {
        classes: vec![
            ClassBuilder::new("demo/App", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .method_with_code(0x0009, "run", "()V", code)
                .build(),
        ],
        resources: Vec::new(),
    }
}

/// The pass: big constants are split, trivia and structure survive.
#[test]
fn constants_split_and_predicated() {
    let mut program = sample();
    let mut ctx = Context::new(Config {
        seed: Some(9),
        ..Config::default()
    });
    let report = ConstantsPass
        .apply(&mut ctx, &mut program)
        .expect("constants");

    let code = program.classes[0].methods[0].code.as_ref().expect("code");
    let has = |v: i32| code.insns.contains(&Insn::Const(Const::Int(v)));
    assert!(!has(100), "source constant survived");
    assert!(has(3), "trivia touched");
    assert!(
        !code.insns.contains(&Insn::Const(Const::Long(10))),
        "long survived"
    );
    // Opaque predicate: a dead branch made of `nop`.
    assert!(code.insns.contains(&Insn::Nop), "no predicate");
    assert!(report.transformed >= 3, "few transforms: {report:?}");
    assert_eq!(report.skipped, 0);
}

/// A method that would blow past the 64KB ceiling stays byte-identical.
#[test]
fn oversized_methods_are_skipped() {
    let code = Code {
        max_stack: 2,
        max_locals: 0,
        insns: vec![Insn::Const(Const::Int(1_000_000)); MAX_METHOD_INSNS + 1],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    let mut program = Program {
        classes: vec![
            ClassBuilder::new("demo/App", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .method_with_code(0x0009, "run", "()V", code)
                .build(),
        ],
        resources: Vec::new(),
    };
    let mut ctx = Context::new(Config {
        seed: Some(9),
        ..Config::default()
    });
    let report = ConstantsPass
        .apply(&mut ctx, &mut program)
        .expect("constants");

    let code = program.classes[0].methods[0].code.as_ref().expect("code");
    assert!(
        code.insns
            .iter()
            .all(|i| matches!(i, Insn::Const(Const::Int(1_000_000)))),
        "oversized method was rewritten"
    );
    assert_eq!(report.skipped, 1);
}
