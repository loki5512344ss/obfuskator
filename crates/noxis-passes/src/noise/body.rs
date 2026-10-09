//! Junk method bodies: plausible branching integer code.
//!
//! A helper is `private static int helper()`: two int locals (`acc`, `i`),
//! a counted loop with a few random binary ops plus one even/odd if/else
//! merge for a real branching shape. Everything is stack-neutral by
//! construction and uses only non-throwing integer ops: woven junk really
//! runs, so it must never throw.
//!
//! Labels start at fixed small numbers: bodies are built from scratch, so
//! nothing collides (labels are per-method).

use noxis_classfile::{Arith, BranchCond, Code, Cond, Const, Insn, Prim, SlotKind};
use noxis_core::Rng;

/// Non-throwing integer ops for junk arithmetic (`Div`/`Rem` could trap).
const OPS: [Arith; 8] = [
    Arith::Add,
    Arith::Sub,
    Arith::Mul,
    Arith::And,
    Arith::Or,
    Arith::Xor,
    Arith::Shl,
    Arith::Shr,
];

/// Fresh-label block for one helper (head, end, odd, merge).
const HEAD: u32 = 1;
const END: u32 = 2;
const ODD: u32 = 3;
const MERGE: u32 = 4;

/// Build one helper body.
pub(super) fn helper_body(rng: &mut Rng) -> Code {
    let bound = 4 + rng.below(29) as i32;
    let mut insns = vec![
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
        Insn::Mark(HEAD),
        Insn::Load {
            slot: 1,
            kind: SlotKind::Int,
        },
        Insn::Const(Const::Int(bound)),
        Insn::Branch(BranchCond::Int(Cond::Ge), END),
    ];
    // Straight-line accumulator churn: `acc = acc op i`, two to four ops.
    for _ in 0..2 + rng.below(3) {
        let op = OPS[rng.below(OPS.len() as u64) as usize];
        insns.extend([
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
                op,
            },
            Insn::Store {
                slot: 0,
                kind: SlotKind::Int,
            },
        ]);
    }
    // Even/odd merge: `if ((i & 1) == 0) acc += c1 else acc ^= c2`.
    let (even, odd) = (rng.next_u64() as i32, rng.next_u64() as i32);
    insns.extend([
        Insn::Load {
            slot: 1,
            kind: SlotKind::Int,
        },
        Insn::Const(Const::Int(1)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::And,
        },
        Insn::Branch(BranchCond::Zero(Cond::Ne), ODD),
        Insn::Load {
            slot: 0,
            kind: SlotKind::Int,
        },
        Insn::Const(Const::Int(even)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Add,
        },
        Insn::Store {
            slot: 0,
            kind: SlotKind::Int,
        },
        Insn::Goto(MERGE),
        Insn::Mark(ODD),
        Insn::Load {
            slot: 0,
            kind: SlotKind::Int,
        },
        Insn::Const(Const::Int(odd)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Xor,
        },
        Insn::Store {
            slot: 0,
            kind: SlotKind::Int,
        },
        Insn::Mark(MERGE),
    ]);
    insns.extend([
        Insn::Iinc { index: 1, delta: 1 },
        Insn::Goto(HEAD),
        Insn::Mark(END),
        Insn::Load {
            slot: 0,
            kind: SlotKind::Int,
        },
        Insn::Return(Some(Prim::Int)),
    ]);
    Code {
        max_stack: 4,
        max_locals: 2,
        insns,
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    }
}

/// Build the entry body: call every helper, sum the results into the static
/// field (a bare `void` call with a dropped result is a recognizable junk
/// pattern on its own). With no helpers the entry is a bare return.
pub(super) fn entry_body(callees: &[(String, String)], owner: &str, field: &str) -> Code {
    let mut insns = Vec::new();
    let mut first = true;
    for (callee_owner, callee_name) in callees {
        insns.push(Insn::Invoke {
            kind: noxis_classfile::InvokeKind::Static,
            owner: callee_owner.clone(),
            name: callee_name.clone(),
            descriptor: "()I".to_string(),
            itf: false,
        });
        if first {
            first = false;
        } else {
            insns.push(Insn::Binary {
                prim: Prim::Int,
                op: Arith::Add,
            });
        }
    }
    if callees.is_empty() {
        insns.push(Insn::Return(None));
    } else {
        insns.push(Insn::PutStatic {
            owner: owner.to_string(),
            name: field.to_string(),
            descriptor: "I".to_string(),
        });
        insns.push(Insn::Return(None));
    }
    Code {
        max_stack: 2,
        max_locals: 0,
        insns,
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    /// Helpers branch and loop instead of returning immediately.
    #[test]
    fn helper_has_branches_and_loop() {
        let mut rng = Rng::from_seed(3);
        let code = helper_body(&mut rng);
        let has = |pred: &dyn Fn(&Insn) -> bool| code.insns.iter().any(pred);
        assert!(has(&|i| matches!(i, Insn::Branch(..))), "no branch");
        assert!(has(&|i| matches!(i, Insn::Goto(_))), "no loop back-edge");
        assert!(has(&|i| matches!(i, Insn::Binary { .. })), "no arithmetic");
        assert!(has(&|i| matches!(i, Insn::Iinc { .. })), "no counter");
        assert!(
            has(&|i| matches!(i, Insn::Return(Some(Prim::Int)))),
            "no int return"
        );
        assert_eq!(code.max_locals, 2);
        // Every jump target is declared.
        let marks: std::collections::BTreeSet<u32> = code
            .insns
            .iter()
            .filter_map(|i| match i {
                Insn::Mark(label) => Some(*label),
                _ => None,
            })
            .collect();
        for insn in &code.insns {
            for target in insn.targets() {
                assert!(marks.contains(&target), "dangling label {target}");
            }
        }
    }

    /// Same seed, same body.
    #[test]
    fn helper_is_deterministic() {
        let first = helper_body(&mut Rng::from_seed(9));
        let second = helper_body(&mut Rng::from_seed(9));
        assert_eq!(first.insns, second.insns);
        let third = helper_body(&mut Rng::from_seed(10));
        assert_ne!(first.insns, third.insns);
    }

    /// The entry calls every helper and stores the sum into the field.
    #[test]
    fn entry_calls_helpers_and_stores() {
        let callees = vec![
            ("junk/A".to_string(), "h0".to_string()),
            ("junk/A".to_string(), "h1".to_string()),
        ];
        let code = entry_body(&callees, "junk/A", "f");
        let invokes = code
            .insns
            .iter()
            .filter(|i| matches!(i, Insn::Invoke { .. }))
            .count();
        assert_eq!(invokes, 2);
        assert!(
            code.insns
                .iter()
                .any(|i| matches!(i, Insn::PutStatic { name, .. } if name == "f")),
            "no field store"
        );
        assert!(matches!(code.insns.last(), Some(Insn::Return(None))));
    }

    /// No helpers, no calls: a bare return, still valid.
    #[test]
    fn entry_without_helpers_returns() {
        let code = entry_body(&[], "junk/A", "f");
        assert_eq!(code.insns, vec![Insn::Return(None)]);
    }
}
