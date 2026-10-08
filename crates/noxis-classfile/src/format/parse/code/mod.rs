//! `Code` backend'а → тело метода нашего IR: метки, исключения, кадры.
//!
//! Backend уже перевёл байтовые смещения в номера инструкций — конвертеры
//! работают прямо с ними: метки ставятся на целях переходов, границах
//! try-блоков и позициях отладочных таблиц; кадры переносятся как есть
//! (индексные дельты). Позиции `LineNumberTable`/`LocalVariableTable` —
//! тоже метки, поэтому вставки пассов их не ломают (байты досчитает writer).

mod frames;
mod labels;

use std::collections::BTreeSet;
use std::io::Cursor;

use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Attribute as RtAttr;
use ristretto_classfile::attributes::{ExceptionTableEntry, Instruction};

use super::attrs;
use super::insns::convert;
use crate::error::ClassfileError;
use crate::insn::Insn;
use crate::model::{
    Attribute, Code, ExceptionHandler, LineNumber, LocalVariable, LocalVariableType,
};

use frames::{catch_type, to_frames};
use labels::{assign, collect_debug_labels, label_indexes, lookup, resolve_byte, resolve_end};

/// Тело метода целиком: backend `Code` → наш [`Code`].
pub(super) fn convert_code(
    max_stack: u16,
    max_locals: u16,
    code: &[Instruction],
    exception_table: &[ExceptionTableEntry],
    attributes: &[RtAttr],
    pool: &ConstantPool<'_>,
) -> Result<Code, ClassfileError> {
    let (starts, code_length) = layout(code)?;
    let mut extra = BTreeSet::new();
    collect_debug_labels(attributes, &starts, code_length, &mut extra)?;
    let labels = assign(&label_indexes(code, exception_table, &extra)?);
    let resolve = |index: u16| lookup(&labels, index);

    let mut insns = Vec::with_capacity(code.len() + labels.len());
    for (position, instruction) in code.iter().enumerate() {
        let index = u16::try_from(position)
            .map_err(|_| ClassfileError::Parse("в теле метода больше 65535 инструкций".into()))?;
        if let Some(label) = labels.get(&u32::from(index)) {
            insns.push(Insn::Mark(*label));
        }
        insns.push(convert(instruction, index, pool, &resolve)?);
    }
    let end = u32::try_from(code.len())
        .map_err(|_| ClassfileError::Parse("в теле метода больше 65535 инструкций".into()))?;
    if let Some(label) = labels.get(&end) {
        insns.push(Insn::Mark(*label));
    }

    let exception_table = exception_table
        .iter()
        .map(|entry| {
            Ok(ExceptionHandler {
                start: lookup(&labels, entry.range_pc.start)?,
                end: lookup(&labels, entry.range_pc.end)?,
                handler: lookup(&labels, entry.handler_pc)?,
                catch_type: catch_type(pool, entry.catch_type)?,
            })
        })
        .collect::<Result<_, ClassfileError>>()?;

    let mut frames = Vec::new();
    let mut rest = Vec::new();
    for attr in attributes {
        match attr {
            RtAttr::StackMapTable { frames: found, .. } => {
                if !frames.is_empty() {
                    return Err(ClassfileError::Parse(
                        "два StackMapTable в одном Code".into(),
                    ));
                }
                frames = to_frames(found, pool)?;
            }
            RtAttr::LineNumberTable { line_numbers, .. } => rest.push(Attribute::LineNumberTable(
                line_numbers
                    .iter()
                    .map(|line| {
                        Ok(LineNumber {
                            start: lookup(&labels, line.start_pc)?,
                            line: line.line_number,
                        })
                    })
                    .collect::<Result<_, ClassfileError>>()?,
            )),
            RtAttr::LocalVariableTable { variables, .. } => {
                rest.push(Attribute::LocalVariableTable(
                    variables
                        .iter()
                        .map(|var| {
                            Ok(LocalVariable {
                                start: resolve_byte(&labels, &starts, var.start_pc)?,
                                end: resolve_end(
                                    &labels,
                                    &starts,
                                    var.start_pc,
                                    var.length,
                                    code_length,
                                )?,
                                name: super::pool::utf8(pool, var.name_index)?,
                                descriptor: super::pool::utf8(pool, var.descriptor_index)?,
                                index: var.index,
                            })
                        })
                        .collect::<Result<_, ClassfileError>>()?,
                ))
            }
            RtAttr::LocalVariableTypeTable { variable_types, .. } => {
                rest.push(Attribute::LocalVariableTypeTable(
                    variable_types
                        .iter()
                        .map(|var| {
                            Ok(LocalVariableType {
                                start: resolve_byte(&labels, &starts, var.start_pc)?,
                                end: resolve_end(
                                    &labels,
                                    &starts,
                                    var.start_pc,
                                    var.length,
                                    code_length,
                                )?,
                                name: super::pool::utf8(pool, var.name_index)?,
                                signature: super::pool::utf8(pool, var.signature_index)?,
                                index: var.index,
                            })
                        })
                        .collect::<Result<_, ClassfileError>>()?,
                ));
            }
            other => rest.push(attrs::convert(other, pool)?),
        }
    }

    Ok(Code {
        max_stack,
        max_locals,
        insns,
        exception_table,
        frames,
        attributes: rest,
    })
}

/// Байтовые смещения инструкций и длина тела метода.
fn layout(code: &[Instruction]) -> Result<(Vec<u16>, u16), ClassfileError> {
    let mut cursor = Cursor::new(Vec::new());
    let mut offsets = Vec::with_capacity(code.len());
    for instruction in code {
        offsets.push(tell(&cursor)?);
        instruction
            .to_bytes(&mut cursor)
            .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    }
    Ok((offsets, tell(&cursor)?))
}

fn tell(cursor: &Cursor<Vec<u8>>) -> Result<u16, ClassfileError> {
    u16::try_from(cursor.position())
        .map_err(|_| ClassfileError::Parse("тело метода длиннее 64 КБ".into()))
}
