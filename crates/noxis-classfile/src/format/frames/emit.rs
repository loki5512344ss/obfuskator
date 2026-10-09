//! Emitting compact frames at the points where the verifier expects them.
//!
//! A frame's kind is chosen by comparing with the previous frame
//! (`Same`/`Append`/`Chop`/`SameLocals`, otherwise `Full`) — like `javac`.
//! Trailing `Top`s are trimmed both when comparing and when writing: an empty
//! slot is not a type.

use std::collections::BTreeSet;

use super::merge::Merger;
use super::xfer::State;
use super::{StackMapFrame, VerificationType};
use crate::error::ClassfileError;

/// Frames at every point in `targets` (sorted — deltas in order).
///
/// Deltas are in instruction numbers (like the backend after reading); the writer
/// recomputes them into bytes itself.
pub(super) fn emit_frames(
    states: &[Option<State>],
    targets: &BTreeSet<u32>,
    initial: &[VerificationType],
    merger: &Merger<'_>,
) -> Result<Vec<StackMapFrame>, ClassfileError> {
    let mut out = Vec::new();
    let mut previous_locals = initial.to_vec();
    let mut previous_index: Option<u32> = None;
    for target in targets {
        let at = *target as usize;
        let Some(state) = &states[at] else {
            continue;
        };
        let delta = match previous_index {
            None => *target,
            Some(previous) => target
                .checked_sub(previous)
                .and_then(|delta| delta.checked_sub(1))
                .ok_or_else(|| merger.invalid("frames out of order"))?,
        };
        out.push(select(delta, &state.locals, &previous_locals, &state.stack));
        previous_locals = state.locals.clone();
        previous_index = Some(*target);
    }
    Ok(out)
}

fn select(
    delta: u32,
    locals: &[VerificationType],
    previous: &[VerificationType],
    stack: &[VerificationType],
) -> StackMapFrame {
    // Comparison is at variable granularity (`long` — one entry, like `javac`);
    // otherwise halves would produce `Append{[Top, Long]}`.
    let current = vars(locals);
    let base = vars(previous);
    // The operand stack needs the same collapsing: a `long`/`double` is one
    // entry (`javac` writes `[Double, Integer]`, not `[Double, Top, Integer]`);
    // an explicit half without its value is rejected by HotSpot.
    let stack = stack_vars(stack);
    if current == base {
        if stack.is_empty() {
            return StackMapFrame::Same {
                offset_delta: delta,
            };
        }
        if let [local] = stack.as_slice() {
            return StackMapFrame::SameLocals {
                offset_delta: delta,
                local: local.clone(),
            };
        }
    }
    if stack.is_empty()
        && let Some(added) = current.strip_prefix(base.as_slice())
        && (1..=3).contains(&added.len())
        && !added.contains(&VerificationType::Top)
    {
        return StackMapFrame::Append {
            offset_delta: delta,
            locals: added.to_vec(),
        };
    }
    if stack.is_empty()
        && let Some(dropped) = base.strip_prefix(current.as_slice())
        && (1..=3).contains(&dropped.len())
        && let Ok(count) = u8::try_from(dropped.len())
    {
        return StackMapFrame::Chop {
            offset_delta: delta,
            count,
        };
    }
    StackMapFrame::Full {
        offset_delta: delta,
        locals: current,
        stack: stack.to_vec(),
    }
}

/// Stack entries → values: `[Double, Top]` — one entry (like `javac`).
///
/// Unlike [`vars`], no trailing-`Top` trimming: every stack entry is live by
/// construction, and dropping one would corrupt the depth.
fn stack_vars(stack: &[VerificationType]) -> Vec<VerificationType> {
    let mut out = Vec::with_capacity(stack.len());
    let mut index = 0;
    while index < stack.len() {
        match &stack[index] {
            VerificationType::Long | VerificationType::Double
                if stack.get(index + 1) == Some(&VerificationType::Top) =>
            {
                out.push(stack[index].clone());
                index += 2;
            }
            other => {
                out.push(other.clone());
                index += 1;
            }
        }
    }
    out
}

/// Slots → variables: `[Long, Top]` — one entry; trailing `Top`s — gone.
///
/// Single `Top`s inside (holes) are kept: in `Full` they are a legal type, and we
/// don't allow them in `Append` (see `select`).
fn vars(locals: &[VerificationType]) -> Vec<VerificationType> {
    let mut out = Vec::with_capacity(locals.len());
    let mut index = 0;
    while index < locals.len() {
        match &locals[index] {
            VerificationType::Long | VerificationType::Double
                if locals.get(index + 1) == Some(&VerificationType::Top) =>
            {
                out.push(locals[index].clone());
                index += 2;
            }
            other => {
                out.push(other.clone());
                index += 1;
            }
        }
    }
    while out.last() == Some(&VerificationType::Top) {
        out.pop();
    }
    out
}
