//! Method-body labels: where they must stand and how they resolve.
//!
//! The backend numbers instructions in order — labels are placed directly on the
//! numbers, without byte offsets.

use std::collections::{BTreeMap, BTreeSet};

use ristretto_classfile::attributes::Attribute as RtAttr;
use ristretto_classfile::attributes::{ExceptionTableEntry, Instruction};

use crate::error::ClassfileError;
use crate::insn::Label;

/// Instruction numbers that must carry a label.
///
/// `extra` — debug-table positions (already as numbers): pass insertions must
/// not leave them without a label.
pub(super) fn label_indexes(
    code: &[Instruction],
    exception_table: &[ExceptionTableEntry],
    extra: &BTreeSet<u32>,
) -> Result<BTreeSet<u32>, ClassfileError> {
    let mut indexes = BTreeSet::from([0u32]);
    for (position, instruction) in code.iter().enumerate() {
        let index = to_index(position)?;
        for target in branch_indexes(instruction, index) {
            indexes.insert(to_index_checked(target)?);
        }
    }
    for entry in exception_table {
        indexes.insert(u32::from(entry.range_pc.start));
        indexes.insert(u32::from(entry.range_pc.end));
        indexes.insert(u32::from(entry.handler_pc));
    }
    indexes.extend(extra.iter().copied());
    Ok(indexes)
}

/// Backend branch targets: for `if*`/`goto` an absolute number, for `switch` a delta.
fn branch_indexes(instruction: &Instruction, index: u16) -> Vec<i32> {
    let base = i32::from(index);
    match instruction {
        Instruction::Tableswitch(table) => once(base + table.default)
            .chain(table.offsets.iter().map(|target| base + target))
            .collect(),
        Instruction::Lookupswitch(lookup) => once(base + lookup.default)
            .chain(lookup.pairs.values().map(|target| base + target))
            .collect(),
        Instruction::Ifeq(target)
        | Instruction::Ifne(target)
        | Instruction::Iflt(target)
        | Instruction::Ifge(target)
        | Instruction::Ifgt(target)
        | Instruction::Ifle(target)
        | Instruction::If_icmpeq(target)
        | Instruction::If_icmpne(target)
        | Instruction::If_icmplt(target)
        | Instruction::If_icmpge(target)
        | Instruction::If_icmpgt(target)
        | Instruction::If_icmple(target)
        | Instruction::If_acmpeq(target)
        | Instruction::If_acmpne(target)
        | Instruction::Ifnull(target)
        | Instruction::Ifnonnull(target)
        | Instruction::Goto(target)
        | Instruction::Jsr(target) => vec![i32::from(*target)],
        Instruction::Goto_w(target) | Instruction::Jsr_w(target) => vec![*target],
        _ => Vec::new(),
    }
}

fn once(value: i32) -> std::iter::Once<i32> {
    std::iter::once(value)
}

/// Turn a sorted set of numbers into sequential labels.
pub(super) fn assign(indexes: &BTreeSet<u32>) -> BTreeMap<u32, Label> {
    indexes
        .iter()
        .enumerate()
        .map(|(position, index)| (*index, position as u32))
        .collect()
}

/// The label for an instruction number.
pub(super) fn lookup(labels: &BTreeMap<u32, Label>, index: u16) -> Result<Label, ClassfileError> {
    labels
        .get(&u32::from(index))
        .copied()
        .ok_or_else(|| ClassfileError::Parse(format!("нет метки для инструкции {index}")))
}

/// Debug-table positions that need labels.
pub(super) fn collect_debug_labels(
    attributes: &[RtAttr],
    starts: &[u16],
    code_length: u16,
    extra: &mut BTreeSet<u32>,
) -> Result<(), ClassfileError> {
    for attr in attributes {
        match attr {
            RtAttr::LineNumberTable { line_numbers, .. } => {
                for line in line_numbers {
                    extra.insert(u32::from(line.start_pc));
                }
            }
            RtAttr::LocalVariableTable { variables, .. } => {
                for var in variables {
                    extra.insert(byte_to_index(starts, var.start_pc)?);
                    extra.insert(end_index(starts, var.start_pc, var.length, code_length)?);
                }
            }
            RtAttr::LocalVariableTypeTable { variable_types, .. } => {
                for var in variable_types {
                    extra.insert(byte_to_index(starts, var.start_pc)?);
                    extra.insert(end_index(starts, var.start_pc, var.length, code_length)?);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Byte → instruction number (instruction boundaries only).
pub(super) fn byte_to_index(starts: &[u16], byte: u16) -> Result<u32, ClassfileError> {
    starts
        .iter()
        .position(|&start| start == byte)
        .and_then(|position| u32::try_from(position).ok())
        .ok_or_else(|| ClassfileError::Parse(format!("смещение {byte} мимо инструкции")))
}

/// End of the range `[start, start+length)`: the index past the end.
fn end_index(
    starts: &[u16],
    start: u16,
    length: u16,
    code_length: u16,
) -> Result<u32, ClassfileError> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| ClassfileError::Parse("диапазон вне метода".into()))?;
    if end == code_length {
        return u32::try_from(starts.len())
            .map_err(|_| ClassfileError::Parse("в теле метода больше 65535 инструкций".into()));
    }
    byte_to_index(starts, end)
}

/// The label for a debug table's byte offset.
pub(super) fn resolve_byte(
    labels: &BTreeMap<u32, Label>,
    starts: &[u16],
    byte: u16,
) -> Result<Label, ClassfileError> {
    let index = byte_to_index(starts, byte)?;
    labels
        .get(&index)
        .copied()
        .ok_or_else(|| ClassfileError::Parse(format!("нет метки для смещения {byte}")))
}

/// The end label of a debug table's range.
pub(super) fn resolve_end(
    labels: &BTreeMap<u32, Label>,
    starts: &[u16],
    start: u16,
    length: u16,
    code_length: u16,
) -> Result<Label, ClassfileError> {
    let index = end_index(starts, start, length, code_length)?;
    labels
        .get(&index)
        .copied()
        .ok_or_else(|| ClassfileError::Parse(format!("нет метки конца {index}")))
}

fn to_index(position: usize) -> Result<u16, ClassfileError> {
    u16::try_from(position)
        .map_err(|_| ClassfileError::Parse("в теле метода больше 65535 инструкций".into()))
}

fn to_index_checked(index: i32) -> Result<u32, ClassfileError> {
    u32::try_from(index)
        .map_err(|_| ClassfileError::Parse(format!("номер инструкции {index} вне тела метода")))
}
