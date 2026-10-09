//! Entry dispatchers: opaque `switch` prologues and opaque guards.
//!
//! Every emitter here is stack-neutral as a whole: whatever the prologue
//! pushes it also consumes before falling through to `body`. That is what
//! makes prepending safe at method entry (empty stack) without any dataflow
//! analysis - all joins at `body` carry an empty stack, so any computed
//! `StackMapTable` stays consistent.

use noxis_classfile::{Arith, BranchCond, Cond, Const, Insn, Prim, SlotKind};
use noxis_core::Rng;

/// Dense `tableswitch` dispatcher (`state-spread`).
///
/// The opaque key is computed as `a + b` (two constants, never the key
/// itself) and dispatched over `[key-2, key+2]`; only the middle slot reaches
/// `body`, the rest fall into a dead block that jumps back to `body`.
/// Does not emit `Mark(body)` - the caller places it.
pub(super) fn table_dispatch(body: u32, dead: u32, rng: &mut Rng) -> Vec<Insn> {
    let key = 100 + (rng.next_u64() % 10_000) as i32;
    let half = (rng.next_u64() as i32) & 0x3FFF_FFFF;
    let low = key - 2;
    vec![
        Insn::Const(Const::Int(half)),
        Insn::Const(Const::Int(key.wrapping_sub(half))),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Add,
        },
        Insn::TableSwitch {
            default: dead,
            low,
            high: key + 2,
            targets: vec![dead, dead, body, dead, dead],
        },
        Insn::Mark(dead),
        Insn::Nop,
        Insn::Goto(body),
    ]
}

/// Sparse `lookupswitch` dispatcher over a counter local (`computed-switch`).
///
/// The counter lives in `slot` (a fresh local owned by the caller): the shape
/// differs from `table_dispatch` on purpose - a sparse switch over a variable
/// instead of a dense range over a stack value. Junk keys never equal `key`.
/// Does not emit `Mark(body)` - the caller places it.
pub(super) fn lookup_dispatch(body: u32, dead: u32, slot: u16, rng: &mut Rng) -> Vec<Insn> {
    let key = 100 + (rng.next_u64() % 10_000) as i32;
    let mut keys = vec![key];
    while keys.len() < 4 {
        let candidate = (rng.next_u64() % 20_000) as i32 - 5_000;
        if !keys.contains(&candidate) {
            keys.push(candidate);
        }
    }
    let mut pairs: Vec<(i32, u32)> = keys
        .into_iter()
        .map(|k| (k, if k == key { body } else { dead }))
        .collect();
    // The JVM requires lookupswitch pairs sorted by key.
    pairs.sort_by_key(|(k, _)| *k);
    vec![
        Insn::Const(Const::Int(key)),
        Insn::Store {
            slot,
            kind: SlotKind::Int,
        },
        Insn::Load {
            slot,
            kind: SlotKind::Int,
        },
        Insn::LookupSwitch {
            default: dead,
            pairs,
        },
        Insn::Mark(dead),
        Insn::Nop,
        Insn::Goto(body),
    ]
}

/// Always-true opaque guard `(C*C + C) & 1 == 0` with a dead `nop` branch.
///
/// Stack-neutral: the computed value is consumed by the branch that never
/// fires. Ends with `Mark(merge)` - execution continues there.
/// Same construction as the constants pass uses; the predicate core is shared
/// with the flow splices (`rewire::opaque_compute`).
pub(super) fn opaque_guard(dead: u32, merge: u32, rng: &mut Rng) -> Vec<Insn> {
    let mut guard = super::rewire::opaque_compute(rng);
    guard.push(Insn::Branch(BranchCond::Zero(Cond::Ne), dead));
    guard.push(Insn::Goto(merge));
    guard.push(Insn::Mark(dead));
    guard.push(Insn::Nop);
    guard.push(Insn::Mark(merge));
    guard
}
