//! A minimal constant-folding adversary: the kind of tool any attacker
//! reaches for first (and the one a forum reader pointed at - two adjacent
//! literals and a binary op fold in your head). Used two ways:
//!
//! * as an honest measurement for §7 - "how much of `Constants` survives a
//!   generic literal evaluator", instead of trusting the docs' own claims;
//! * as a correctness oracle - whatever a splitting scheme emits must fold
//!   back to the exact source value, for every scheme we ship, forever.
//!
//! Deliberately NOT a full symbolic/SMT solver: it only walks maximal
//! straight-line runs of `Const(Int/Long)` + `Binary{Int,Long}`, the same
//! vocabulary `noxis_passes::constants` writes. That is the realistic floor,
//! not a strawman - a generic constant-propagation pass in any real
//! deobfuscator does at least this much.
//!
//! Each stack slot carries its own provenance (the span of instructions that
//! produced it), not just a bare value: several independent split constants
//! placed back to back (no separator between them) must be reported as
//! separate findings, not glommed into one run just because nothing happens
//! to separate them on the instruction stream.

use noxis_classfile::{Arith, Const, Insn, Prim, Program};

/// One straight-line run the adversary folded to a single value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldedConstant {
    pub class: String,
    pub method: String,
    /// Instruction index the run starts at (first `Const`).
    pub start: usize,
    /// Instruction count the run spans.
    pub len: usize,
    /// The folded value (`Int` results are sign-extended).
    pub value: i64,
}

/// Fold every maximal literal-arithmetic run in the program.
pub fn fold_constants(program: &Program) -> Vec<FoldedConstant> {
    let mut found = Vec::new();
    for class in &program.classes {
        for method in &class.methods {
            let Some(code) = method.code.as_ref() else {
                continue;
            };
            fold_method(&class.this_class, &method.name, &code.insns, &mut found);
        }
    }
    found
}

#[derive(Clone, Copy)]
enum Val {
    I(i32),
    L(i64),
}

impl Val {
    fn widen(self) -> i64 {
        match self {
            Val::I(v) => i64::from(v),
            Val::L(v) => v,
        }
    }
}

/// A stack slot: the abstract value plus the instruction span that produced
/// it (`start` of its earliest literal, `end` exclusive of the op that
/// closed it).
struct Entry {
    val: Val,
    start: usize,
    end: usize,
}

fn fold_method(class: &str, method: &str, insns: &[Insn], out: &mut Vec<FoldedConstant>) {
    let mut i = 0;
    while i < insns.len() {
        if !is_literal(&insns[i]) {
            i += 1;
            continue;
        }
        let mut stack: Vec<Entry> = Vec::new();
        let mut j = i;
        while j < insns.len() {
            match &insns[j] {
                Insn::Const(Const::Int(v)) => {
                    stack.push(Entry {
                        val: Val::I(*v),
                        start: j,
                        end: j + 1,
                    });
                }
                Insn::Const(Const::Long(v)) => {
                    stack.push(Entry {
                        val: Val::L(*v),
                        start: j,
                        end: j + 1,
                    });
                }
                Insn::Binary {
                    prim: prim @ (Prim::Int | Prim::Long),
                    op,
                } => {
                    let Some(b) = stack.pop() else { break };
                    let Some(a) = stack.pop() else { break };
                    let Some(val) = apply(*prim, *op, a.val, b.val) else {
                        break;
                    };
                    stack.push(Entry {
                        val,
                        start: a.start.min(b.start),
                        end: j + 1,
                    });
                }
                _ => break,
            }
            j += 1;
        }
        // Every stack entry still standing when the run ends is a completed,
        // never-consumed sub-expression. A bare literal (span of 1) isn't an
        // obfuscated form - only a multi-instruction reduction counts.
        for entry in &stack {
            if entry.end - entry.start > 1 {
                out.push(FoldedConstant {
                    class: class.to_string(),
                    method: method.to_string(),
                    start: entry.start,
                    len: entry.end - entry.start,
                    value: entry.val.widen(),
                });
            }
        }
        i = if j > i { j } else { i + 1 };
    }
}

fn is_literal(insn: &Insn) -> bool {
    matches!(insn, Insn::Const(Const::Int(_) | Const::Long(_)))
}

fn apply(prim: Prim, op: Arith, a: Val, b: Val) -> Option<Val> {
    match (prim, a, b) {
        (Prim::Int, Val::I(a), Val::I(b)) => int_op(op, a, b).map(Val::I),
        (Prim::Long, Val::L(a), Val::L(b)) => long_op(op, a, b).map(Val::L),
        // A splitting scheme never mixes widths mid-run; a mismatch here
        // means this isn't one of ours - stop folding rather than guess.
        _ => None,
    }
}

fn int_op(op: Arith, a: i32, b: i32) -> Option<i32> {
    Some(match op {
        Arith::Add => a.wrapping_add(b),
        Arith::Sub => a.wrapping_sub(b),
        Arith::Mul => a.wrapping_mul(b),
        Arith::Div => a.checked_div(b)?,
        Arith::Rem => a.checked_rem(b)?,
        Arith::And => a & b,
        Arith::Or => a | b,
        Arith::Xor => a ^ b,
        Arith::Shl => a.wrapping_shl(b as u32),
        Arith::Shr => a.wrapping_shr(b as u32),
        Arith::Ushr => (a as u32).wrapping_shr(b as u32) as i32,
        Arith::Neg => return None,
    })
}

fn long_op(op: Arith, a: i64, b: i64) -> Option<i64> {
    Some(match op {
        Arith::Add => a.wrapping_add(b),
        Arith::Sub => a.wrapping_sub(b),
        Arith::Mul => a.wrapping_mul(b),
        Arith::Div => a.checked_div(b)?,
        Arith::Rem => a.checked_rem(b)?,
        Arith::And => a & b,
        Arith::Or => a | b,
        Arith::Xor => a ^ b,
        Arith::Shl => a.wrapping_shl(b as u32),
        Arith::Shr => a.wrapping_shr(b as u32),
        Arith::Ushr => (a as u64).wrapping_shr(b as u32) as i64,
        Arith::Neg => return None,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use noxis_classfile::{ClassBuilder, Code, Version};
    use noxis_core::{Config, Context, Pass};
    use noxis_passes::ConstantsPass;

    use super::*;

    fn sample(values: &[i32]) -> Program {
        let mut insns: Vec<Insn> = values.iter().map(|v| Insn::Const(Const::Int(*v))).collect();
        insns.push(Insn::Return(None));
        let code = Code {
            max_stack: 1,
            max_locals: 0,
            insns,
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

    /// A plain literal is not itself an obfuscated form (no arithmetic to
    /// fold) - the adversary must not flag it.
    #[test]
    fn bare_literals_are_not_folded() {
        let program = sample(&[1, 2, 3]);
        assert!(fold_constants(&program).is_empty());
    }

    /// The correctness oracle: every scheme `Constants` ships must fold back
    /// to the exact source value, in source order, even when several split
    /// constants sit back to back with nothing separating them - a
    /// regression here is a silently wrong obfuscation, not just a weak one.
    #[test]
    fn every_constants_scheme_folds_back_to_the_source_value() {
        for (seed, values) in [
            (1u64, vec![1000, -7, 65536, i32::MAX, i32::MIN]),
            (2, vec![42, -65536, 123456789]),
        ] {
            let mut program = sample(&values);
            let mut ctx = Context::new(Config {
                seed: Some(seed),
                ..Config::default()
            });
            ctx.config.constants.max_predicates = 0; // isolate splitting from predicates
            ConstantsPass.apply(&mut ctx, &mut program).expect("pass");

            let folded = fold_constants(&program);
            let recovered: Vec<i64> = folded.iter().map(|f| f.value).collect();
            let expected: Vec<i64> = values.iter().map(|v| i64::from(*v)).collect();
            assert_eq!(
                recovered, expected,
                "adversary disagrees with source for seed {seed}"
            );
        }
    }
}
