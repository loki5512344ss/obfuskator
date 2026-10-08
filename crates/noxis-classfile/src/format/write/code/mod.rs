//! Тело метода нашего IR → атрибут `Code` backend'а.
//!
//! Метки резолвятся в номера инструкций один раз здесь; дальше backend сам
//! пересчитывает номера в байтовые смещения при `to_bytes`.

mod frames;
pub(crate) mod insns;
mod ops;

use std::collections::BTreeMap;
use std::io::Cursor;

use ristretto_classfile::attributes::{
    Attribute as RtAttr, ExceptionTableEntry as RtEntry, Instruction as RtInsn,
};

use super::attrs;
use super::pool::Pool;
use crate::error::ClassfileError;
use crate::insn::{BranchCond, Cond, Insn, Label, Prim};
use crate::model::{Attribute, Code, ExceptionHandler};

use frames::convert_frames;

/// Тело метода → готовый атрибут `Code` (с логическими номерами инструкций).
pub(super) fn convert_code(code: &Code, pool: &mut Pool) -> Result<RtAttr, ClassfileError> {
    let (labels, bare) = split_marks(&code.insns)?;
    let resolve = |label: Label| -> Result<u32, ClassfileError> {
        labels
            .get(&label)
            .copied()
            .ok_or_else(|| ClassfileError::Write(format!("нет метки {label}")))
    };
    let mut rt_insns = Vec::with_capacity(bare.len());
    for (index, insn) in bare.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| ClassfileError::Write("в методе больше 4 млрд инструкций".into()))?;
        if let Some(rt) = insns::convert(insn, index, pool, &resolve)? {
            rt_insns.push(rt);
        }
    }
    let exception_table = code
        .exception_table
        .iter()
        .map(|handler| entry(handler, &resolve, pool))
        .collect::<Result<_, _>>()?;
    let to_byte = measure(&rt_insns)?;
    let byte_of = |index: u32| -> Result<u16, ClassfileError> {
        to_byte
            .get(&index)
            .copied()
            .ok_or_else(|| ClassfileError::Write(format!("нет байта для инструкции {index}")))
    };
    let positions = attrs::Positions {
        resolve: &resolve,
        byte_of: &byte_of,
    };
    let frames = convert_frames(&code.frames, pool)?;
    let mut attributes = Vec::with_capacity(code.attributes.len() + 1);
    for attr in &code.attributes {
        attributes.push(match attr {
            // Backend хранит номера строк в индексах и маппит сам.
            Attribute::LineNumberTable(lines) => attrs::line_number_table(lines, pool, &positions)?,
            Attribute::LocalVariableTable(vars) => attrs::local_table(vars, pool, &positions)?,
            Attribute::LocalVariableTypeTable(vars) => {
                attrs::local_type_table(vars, pool, &positions)?
            }
            other => attrs::convert(other, pool)?,
        });
    }
    if !frames.is_empty() {
        attributes.push(RtAttr::StackMapTable {
            name_index: pool.utf8("StackMapTable")?,
            frames,
        });
    }
    Ok(RtAttr::Code {
        name_index: pool.utf8("Code")?,
        max_stack: code.max_stack,
        max_locals: code.max_locals,
        code: rt_insns,
        exception_table,
        attributes,
    })
}

/// Метки → номера инструкций; сами маркеры из потока убираются.
fn split_marks(insns: &[Insn]) -> Result<(BTreeMap<Label, u32>, Vec<&Insn>), ClassfileError> {
    let mut labels = BTreeMap::new();
    let mut bare = Vec::with_capacity(insns.len());
    for insn in insns {
        match insn {
            Insn::Mark(label) => {
                let index = u32::try_from(bare.len()).map_err(|_| {
                    ClassfileError::Write("в методе больше 4 млрд инструкций".into())
                })?;
                if let Some(prev) = labels.insert(*label, index)
                    && prev != index
                {
                    return Err(ClassfileError::Write(format!(
                        "метка {label} объявлена дважды"
                    )));
                }
            }
            _ => bare.push(insn),
        }
    }
    Ok((labels, bare))
}

/// Номер инструкции → байтовое смещение (замер с заглушками целей).
///
/// В карту входит и конец кода (`len` → длина): концы диапазонов указывают
/// за последнюю инструкцию.
pub(crate) fn measure(insns: &[RtInsn]) -> Result<BTreeMap<u32, u16>, ClassfileError> {
    let mut map = BTreeMap::new();
    let mut cursor = Cursor::new(Vec::new());
    for (index, insn) in insns.iter().enumerate() {
        let position = tell(&cursor)?;
        let index = u32::try_from(index)
            .map_err(|_| ClassfileError::Write("в методе больше 4 млрд инструкций".into()))?;
        map.insert(index, position);
        let mut sizing = insn.clone();
        smash_targets(&mut sizing, position);
        sizing
            .to_bytes(&mut cursor)
            .map_err(|e| ClassfileError::Write(e.to_string()))?;
    }
    let end = u32::try_from(insns.len())
        .map_err(|_| ClassfileError::Write("в методе больше 4 млрд инструкций".into()))?;
    map.insert(end, tell(&cursor)?);
    Ok(map)
}

/// Цели ветвлений не влияют на размер — подменяем текущей позицией для замера.
fn smash_targets(insn: &mut RtInsn, position: u16) {
    match insn {
        RtInsn::Ifeq(target)
        | RtInsn::Ifne(target)
        | RtInsn::Iflt(target)
        | RtInsn::Ifge(target)
        | RtInsn::Ifgt(target)
        | RtInsn::Ifle(target)
        | RtInsn::If_icmpeq(target)
        | RtInsn::If_icmpne(target)
        | RtInsn::If_icmplt(target)
        | RtInsn::If_icmpge(target)
        | RtInsn::If_icmpgt(target)
        | RtInsn::If_icmple(target)
        | RtInsn::If_acmpeq(target)
        | RtInsn::If_acmpne(target)
        | RtInsn::Goto(target)
        | RtInsn::Jsr(target)
        | RtInsn::Ifnull(target)
        | RtInsn::Ifnonnull(target) => *target = position,
        RtInsn::Goto_w(target) | RtInsn::Jsr_w(target) => *target = i32::from(position),
        _ => {}
    }
}

fn tell(cursor: &Cursor<Vec<u8>>) -> Result<u16, ClassfileError> {
    u16::try_from(cursor.position())
        .map_err(|_| ClassfileError::Write("тело метода длиннее 64 КБ".into()))
}

fn entry(
    handler: &ExceptionHandler,
    resolve: &impl Fn(Label) -> Result<u32, ClassfileError>,
    pool: &mut Pool,
) -> Result<RtEntry, ClassfileError> {
    Ok(RtEntry {
        range_pc: fit_index(resolve(handler.start)?)?..fit_index(resolve(handler.end)?)?,
        handler_pc: fit_index(resolve(handler.handler)?)?,
        catch_type: match &handler.catch_type {
            None => 0,
            Some(name) => pool.class(name)?,
        },
    })
}

/// Условный переход IR → `if*` backend'а.
fn branch(cond: BranchCond, target: u32) -> Result<RtInsn, ClassfileError> {
    let target = fit_index(target)?;
    Ok(match cond {
        BranchCond::Zero(Cond::Eq) => RtInsn::Ifeq(target),
        BranchCond::Zero(Cond::Ne) => RtInsn::Ifne(target),
        BranchCond::Zero(Cond::Lt) => RtInsn::Iflt(target),
        BranchCond::Zero(Cond::Ge) => RtInsn::Ifge(target),
        BranchCond::Zero(Cond::Gt) => RtInsn::Ifgt(target),
        BranchCond::Zero(Cond::Le) => RtInsn::Ifle(target),
        BranchCond::Int(Cond::Eq) => RtInsn::If_icmpeq(target),
        BranchCond::Int(Cond::Ne) => RtInsn::If_icmpne(target),
        BranchCond::Int(Cond::Lt) => RtInsn::If_icmplt(target),
        BranchCond::Int(Cond::Ge) => RtInsn::If_icmpge(target),
        BranchCond::Int(Cond::Gt) => RtInsn::If_icmpgt(target),
        BranchCond::Int(Cond::Le) => RtInsn::If_icmple(target),
        BranchCond::Ref { eq: true } => RtInsn::If_acmpeq(target),
        BranchCond::Ref { eq: false } => RtInsn::If_acmpne(target),
        BranchCond::Null { is_null: true } => RtInsn::Ifnull(target),
        BranchCond::Null { is_null: false } => RtInsn::Ifnonnull(target),
    })
}

fn ret_op(prim: Option<Prim>) -> RtInsn {
    match prim {
        None => RtInsn::Return,
        Some(Prim::Int) => RtInsn::Ireturn,
        Some(Prim::Long) => RtInsn::Lreturn,
        Some(Prim::Float) => RtInsn::Freturn,
        Some(Prim::Double) => RtInsn::Dreturn,
    }
}

/// Номер инструкции обязан влезать в `u16` (метод < 64 КБ кода).
fn fit_index(index: u32) -> Result<u16, ClassfileError> {
    u16::try_from(index)
        .map_err(|_| ClassfileError::Write(format!("номер инструкции {index} вне u16")))
}

/// Дельта `switch`: цель минус позиция самого `switch`.
fn delta(index: u32, target: u32) -> Result<i32, ClassfileError> {
    let (index, target) = (to_i32(index)?, to_i32(target)?);
    target
        .checked_sub(index)
        .ok_or_else(|| ClassfileError::Write(format!("переполнение дельты {index}->{target}")))
}

fn to_i32(value: u32) -> Result<i32, ClassfileError> {
    i32::try_from(value)
        .map_err(|_| ClassfileError::Write(format!("номер инструкции {value} вне i32")))
}
