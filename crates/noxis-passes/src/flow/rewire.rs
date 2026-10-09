//! CFG rewiring: branch inversion, block splitting, exception routing.
//!
//! All rewrites preserve semantics without dataflow analysis:
//! - inverting a branch negates its condition and swaps target/fallthrough;
//! - a `goto` + dead `nop` splice is stack-neutral at any cut point, because
//!   both joins carry the same stack the original stream had;
//! - the dead catch-all rejoins `body` with an empty stack after popping the
//!   exception, so it merges with the dispatcher's empty-stack edges.

use noxis_classfile::{BranchCond, Cond, ExceptionHandler, Insn};
use noxis_core::Rng;

/// Logical negation of a branch condition.
pub(super) fn negate(cond: BranchCond) -> BranchCond {
    match cond {
        BranchCond::Zero(c) => BranchCond::Zero(negate_cond(c)),
        BranchCond::Int(c) => BranchCond::Int(negate_cond(c)),
        BranchCond::Ref { eq } => BranchCond::Ref { eq: !eq },
        BranchCond::Null { is_null } => BranchCond::Null { is_null: !is_null },
    }
}

fn negate_cond(cond: Cond) -> Cond {
    match cond {
        Cond::Eq => Cond::Ne,
        Cond::Ne => Cond::Eq,
        Cond::Lt => Cond::Ge,
        Cond::Le => Cond::Gt,
        Cond::Gt => Cond::Le,
        Cond::Ge => Cond::Lt,
    }
}

/// Turn `if-cond T` into `if-not SKIP; goto T; SKIP:` (`butterfly`).
///
/// `rate` is a 0–100 percentage applied per branch. Semantics are unchanged:
/// the inverted branch skips a `goto` to the original target, so the taken /
/// not-taken destinations are exactly the old ones. A `nop` pads the fresh
/// mark: whatever follows (an original mark, even a handler entry with its
/// own stack) must not share a bytecode offset with it, or their frames
/// would merge from inconsistent states.
pub(super) fn invert_branches(
    insns: &mut Vec<Insn>,
    fresh: &mut crate::flow::LabelGen,
    rng: &mut Rng,
    rate: u64,
) {
    let mut index = 0;
    while index < insns.len() {
        let (cond, target) = match &insns[index] {
            Insn::Branch(cond, target) => (*cond, *target),
            _ => {
                index += 1;
                continue;
            }
        };
        if rng.below(100) >= rate {
            index += 1;
            continue;
        }
        let skip = fresh.next();
        insns.splice(
            index..=index,
            [
                Insn::Branch(negate(cond), skip),
                Insn::Goto(target),
                Insn::Mark(skip),
                Insn::Nop,
            ],
        );
        index += 4;
    }
}

/// Cut the stream with opaque-entered splices.
///
/// Each splice computes an always-false opaque predicate `(C*C + C) & 1 != 0`
/// and branches on it: the fallthrough continues the stream, the dead block
/// throws instead of rejoining. Throwing (rather than jumping back) keeps the
/// splice valid at *any* stack depth - `athrow` has no successor, so there is
/// nothing to merge - and the dead block still owns a frame through its entry
/// branch, which HotSpot requires even for code that never executes.
///
/// Cuts land at or after `start` (the entry prologue must stay first) and
/// only at reachable positions: a splice in an unreachable gap would leave
/// its `into` mark without a computed state, and the verifier rejects jump
/// targets without frames.
///
/// A cut is also pushed past a run of consecutive marks. Inserting *before* a
/// targeted mark would reroute its incoming branch edges over the splice,
/// orphaning the splice itself: the mark must keep its edges, so the splice
/// goes after it and inherits its reachability through fallthrough.
pub(super) fn split_blocks(
    insns: &mut Vec<Insn>,
    fresh: &mut crate::flow::LabelGen,
    rng: &mut Rng,
    start: usize,
    count: usize,
) {
    let live = reachable(insns);
    let mut cuts = Vec::with_capacity(count);
    for _ in 0..count {
        let allowed: Vec<usize> = (start..insns.len())
            .filter(|c| live.get(*c).copied().unwrap_or(false))
            .collect();
        if allowed.is_empty() {
            break;
        }
        let mut cut = allowed[rng.below(allowed.len() as u64) as usize];
        while cut < insns.len() && matches!(insns[cut], Insn::Mark(_)) {
            cut += 1;
        }
        // A mark run at the very end leaves no stream to cut into.
        if cut < insns.len() {
            cuts.push(cut);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    for cut in cuts.into_iter().rev() {
        let into = fresh.next();
        let dead = fresh.next();
        let mut splice = opaque_compute(rng);
        splice.push(Insn::Branch(BranchCond::Zero(Cond::Ne), dead));
        splice.push(Insn::Goto(into));
        splice.push(Insn::Mark(dead));
        splice.push(Insn::Nop);
        splice.push(Insn::AconstNull);
        splice.push(Insn::AThrow);
        splice.push(Insn::Mark(into));
        insns.splice(cut..cut, splice);
    }
}

/// The opaque predicate core: computes `(C*C + C) & 1` (always 0) on top of
/// the stack without disturbing what is below it.
pub(super) fn opaque_compute(rng: &mut Rng) -> Vec<Insn> {
    use noxis_classfile::{Arith, Const, Prim};

    let constant = rng.next_u64() as i32;
    vec![
        Insn::Const(Const::Int(constant)),
        Insn::Dup,
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Mul,
        },
        Insn::Const(Const::Int(constant)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Add,
        },
        Insn::Const(Const::Int(1)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::And,
        },
    ]
}

/// Vec positions reachable from method entry via fallthrough and branch
/// targets. Marks are transparent: control flows through them to the next
/// instruction. Deliberately exception-oblivious (handler-only code reads as
/// unreachable): cuts there are skipped rather than risked.
pub(super) fn reachable(insns: &[Insn]) -> Vec<bool> {
    use std::collections::{BTreeMap, VecDeque};

    let mut at_label: BTreeMap<u32, usize> = BTreeMap::new();
    for (index, insn) in insns.iter().enumerate() {
        if let Insn::Mark(label) = insn {
            at_label.entry(*label).or_insert(index);
        }
    }
    let resolve = |label: u32| at_label.get(&label).copied();
    let terminal = |insn: &Insn| {
        matches!(
            insn,
            Insn::Goto(_)
                | Insn::Jsr(_)
                | Insn::Ret(_)
                | Insn::TableSwitch { .. }
                | Insn::LookupSwitch { .. }
                | Insn::Return(_)
                | Insn::AReturn
                | Insn::AThrow
        )
    };
    let mut live = vec![false; insns.len()];
    let mut queue = VecDeque::new();
    if !live.is_empty() {
        queue.push_back(0);
    }
    while let Some(index) = queue.pop_front() {
        if live[index] {
            continue;
        }
        live[index] = true;
        let insn = &insns[index];
        if !terminal(insn) && index + 1 < insns.len() {
            queue.push_back(index + 1);
        }
        for target in insn.targets() {
            if let Some(resolved) = resolve(target) {
                queue.push_back(resolved);
            }
        }
    }
    live
}

/// Dead catch-all over `try_start..try_end`, rejoining `body`.
///
/// The covered prologue cannot throw (constants, switches, gotos), so the
/// handler never fires; if it ever did, popping the exception and jumping to
/// `body` behaves exactly like the fallthrough path.
pub(super) fn dead_catch(
    try_start: u32,
    try_end: u32,
    handler: u32,
    body: u32,
) -> (Vec<Insn>, ExceptionHandler) {
    let code = vec![Insn::Mark(handler), Insn::Pop, Insn::Nop, Insn::Goto(body)];
    let entry = ExceptionHandler {
        start: try_start,
        end: try_end,
        handler,
        catch_type: None,
    };
    (code, entry)
}
