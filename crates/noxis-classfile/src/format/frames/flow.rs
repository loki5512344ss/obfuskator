//! A method body's CFG: labels, edges, frame points, handler ranges.

use std::collections::{BTreeMap, BTreeSet};

use super::merge::Merger;
use crate::error::ClassfileError;
use crate::insn::{Insn, Label};
use crate::model::ExceptionHandler;

/// Labels → instruction numbers, without the markers.
pub(super) fn strip_marks<'a>(
    insns: &'a [Insn],
    merger: &Merger<'_>,
) -> Result<(BTreeMap<Label, u32>, Vec<&'a Insn>), ClassfileError> {
    let mut labels = BTreeMap::new();
    let mut bare = Vec::with_capacity(insns.len());
    for insn in insns {
        match insn {
            Insn::Mark(label) => {
                let index = u32::try_from(bare.len())
                    .map_err(|_| merger.invalid("слишком много инструкций"))?;
                if let Some(prev) = labels.insert(*label, index)
                    && prev != index
                {
                    return Err(merger.invalid(&format!("метка {label} объявлена дважды")));
                }
            }
            _ => bare.push(insn),
        }
    }
    Ok((labels, bare))
}

/// CFG edges without exceptions: fallthrough + branch targets.
pub(super) fn successors(
    insn: &Insn,
    index: u32,
    len: u32,
    labels: &BTreeMap<Label, u32>,
    merger: &Merger<'_>,
) -> Result<Vec<u32>, ClassfileError> {
    let mut out = Vec::new();
    // No fallthrough: jumps that don't return and method exits.
    // `Jsr`/`Ret` never reach here (rejected by `transfer`), but the edges exist.
    let terminal = matches!(
        insn,
        Insn::Goto(..)
            | Insn::Jsr(..)
            | Insn::Ret(..)
            | Insn::TableSwitch { .. }
            | Insn::LookupSwitch { .. }
            | Insn::Return(..)
            | Insn::AReturn
            | Insn::AThrow
    );
    if !terminal && index + 1 < len {
        out.push(index + 1);
    }
    for target in insn.targets() {
        out.push(
            labels
                .get(&target)
                .copied()
                .ok_or_else(|| merger.invalid(&format!("нет метки {target}")))?,
        );
    }
    Ok(out)
}

/// Points where the verifier requires frames: branch targets + handler entries.
pub(super) fn frame_targets(
    bare: &[&Insn],
    labels: &BTreeMap<Label, u32>,
    handlers: &[HandlerRange],
    merger: &Merger<'_>,
) -> Result<BTreeSet<u32>, ClassfileError> {
    let mut targets = BTreeSet::new();
    for insn in bare {
        for target in insn.targets() {
            targets.insert(
                labels
                    .get(&target)
                    .copied()
                    .ok_or_else(|| merger.invalid(&format!("нет метки {target}")))?,
            );
        }
    }
    for (_, _, handler, _) in handlers {
        targets.insert(*handler);
    }
    Ok(targets)
}

/// A handler point in instruction numbers: `(start, end, handler, catch type)`.
///
/// Ranges are half-open `[start, end)`, as in the JVMS.
pub(super) type HandlerRange = (u32, u32, u32, Option<String>);

/// Handlers in instruction numbers, for exception edges.
pub(super) fn handler_ranges(
    table: &[ExceptionHandler],
    labels: &BTreeMap<Label, u32>,
    merger: &Merger<'_>,
) -> Result<Vec<HandlerRange>, ClassfileError> {
    table
        .iter()
        .map(|handler| {
            let index_of = |label: Label| -> Result<u32, ClassfileError> {
                labels
                    .get(&label)
                    .copied()
                    .ok_or_else(|| merger.invalid(&format!("нет метки {label}")))
            };
            Ok((
                index_of(handler.start)?,
                index_of(handler.end)?,
                index_of(handler.handler)?,
                handler.catch_type.clone(),
            ))
        })
        .collect()
}
