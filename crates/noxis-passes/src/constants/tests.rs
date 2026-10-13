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
        // MBA: `(a&b) + (a|b) == a+b`. Each sub-expression is still a
        // foldable `LDC,LDC,BINOP` triple - a generic constant evaluator
        // still resolves this instantly. What it defeats is a *pattern*
        // matcher hardcoded to one opcode (`ldc,ldc,ixor` = "this is a
        // split constant"): it would need to recognize the `and+or+add`
        // identity specifically, not just fold a literal pair.
        [
            Insn::Const(Const::Int(a1)),
            Insn::Const(Const::Int(b1)),
            Insn::Binary {
                op: noxis_classfile::Arith::And,
                ..
            },
            Insn::Const(Const::Int(a2)),
            Insn::Const(Const::Int(b2)),
            Insn::Binary {
                op: noxis_classfile::Arith::Or,
                ..
            },
            Insn::Binary {
                op: noxis_classfile::Arith::Add,
                ..
            },
        ] if a1 == a2 && b1 == b2 => (a1 & b1).wrapping_add(a1 | b1),
        _ => panic!("not a form: {insns:?}"),
    }
}

#[test]
fn int_forms_round_trip() {
    let mut rng = noxis_core::Rng::from_seed(11);
    for value in [-1000, -7, -6, 6, 7, 42, 65536, i32::MAX, i32::MIN] {
        for scheme in [Scheme::Xor, Scheme::Add, Scheme::Shift, Scheme::Mba] {
            assert_eq!(eval_int(&encrypt_int(value, scheme, &mut rng)), value);
        }
    }
}

#[test]
fn long_forms_keep_shape() {
    let mut rng = noxis_core::Rng::from_seed(12);
    for scheme in [Scheme::Xor, Scheme::Add, Scheme::Shift, Scheme::Mba] {
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

/// MBA shape: `and`/`or`/`add` on two reused operands, not `xor`/`add` on a
/// single pair - a matcher hardcoded to one opcode triple (`ldc,ldc,ixor`)
/// doesn't recognize this as "a split constant" without the identity.
#[test]
fn mba_form_uses_and_or_add_not_a_single_binop() {
    let mut rng = noxis_core::Rng::from_seed(13);
    for value in [-1000, -6, 6, 42, 65536, i32::MAX, i32::MIN] {
        let insns = encrypt_int(value, Scheme::Mba, &mut rng);
        let ops: Vec<_> = insns
            .iter()
            .filter_map(|i| match i {
                Insn::Binary { op, .. } => Some(*op),
                _ => None,
            })
            .collect();
        assert_eq!(
            ops,
            [
                noxis_classfile::Arith::And,
                noxis_classfile::Arith::Or,
                noxis_classfile::Arith::Add,
            ],
            "unexpected mba shape: {insns:?}"
        );
        assert_eq!(insns.len(), 7, "unexpected mba insn count: {insns:?}");
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
