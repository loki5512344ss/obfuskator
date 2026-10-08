//! Ветвления, `switch` и возвраты: абсолютные/относительные цели → метки.

use ristretto_classfile::attributes::{Instruction, LookupSwitch, TableSwitch};

use super::Resolver;
use crate::error::ClassfileError;
use crate::insn::{BranchCond, Cond, Insn, Label, Prim};

/// `Some` — инструкция покрыта, `None` — отдать следующим конвертерам.
pub(super) fn convert_flow(
    instruction: &Instruction,
    index: u16,
    resolve: &Resolver<'_>,
) -> Result<Option<Insn>, ClassfileError> {
    let rel = |delta: i32| -> Result<Label, ClassfileError> { abs(index, delta, resolve) };
    Ok(Some(match instruction {
        Instruction::Ifeq(t) => branch(BranchCond::Zero(Cond::Eq), *t, resolve)?,
        Instruction::Ifne(t) => branch(BranchCond::Zero(Cond::Ne), *t, resolve)?,
        Instruction::Iflt(t) => branch(BranchCond::Zero(Cond::Lt), *t, resolve)?,
        Instruction::Ifge(t) => branch(BranchCond::Zero(Cond::Ge), *t, resolve)?,
        Instruction::Ifgt(t) => branch(BranchCond::Zero(Cond::Gt), *t, resolve)?,
        Instruction::Ifle(t) => branch(BranchCond::Zero(Cond::Le), *t, resolve)?,
        Instruction::If_icmpeq(t) => branch(BranchCond::Int(Cond::Eq), *t, resolve)?,
        Instruction::If_icmpne(t) => branch(BranchCond::Int(Cond::Ne), *t, resolve)?,
        Instruction::If_icmplt(t) => branch(BranchCond::Int(Cond::Lt), *t, resolve)?,
        Instruction::If_icmpge(t) => branch(BranchCond::Int(Cond::Ge), *t, resolve)?,
        Instruction::If_icmpgt(t) => branch(BranchCond::Int(Cond::Gt), *t, resolve)?,
        Instruction::If_icmple(t) => branch(BranchCond::Int(Cond::Le), *t, resolve)?,
        Instruction::If_acmpeq(t) => branch(BranchCond::Ref { eq: true }, *t, resolve)?,
        Instruction::If_acmpne(t) => branch(BranchCond::Ref { eq: false }, *t, resolve)?,
        Instruction::Ifnull(t) => branch(BranchCond::Null { is_null: true }, *t, resolve)?,
        Instruction::Ifnonnull(t) => branch(BranchCond::Null { is_null: false }, *t, resolve)?,
        Instruction::Goto(t) => Insn::Goto(resolve(*t)?),
        Instruction::Goto_w(t) => Insn::Goto(resolve(wide(*t)?)?),
        Instruction::Jsr(t) => Insn::Jsr(resolve(*t)?),
        Instruction::Jsr_w(t) => Insn::Jsr(resolve(wide(*t)?)?),
        Instruction::Ret(i) => Insn::Ret(u16::from(*i)),
        Instruction::Ret_w(i) => Insn::Ret(*i),
        Instruction::Tableswitch(s) => switch_table(s, &rel)?,
        Instruction::Lookupswitch(s) => switch_lookup(s, &rel)?,
        Instruction::Return => Insn::Return(None),
        Instruction::Ireturn => Insn::Return(Some(Prim::Int)),
        Instruction::Lreturn => Insn::Return(Some(Prim::Long)),
        Instruction::Freturn => Insn::Return(Some(Prim::Float)),
        Instruction::Dreturn => Insn::Return(Some(Prim::Double)),
        Instruction::Areturn => Insn::AReturn,
        _ => return Ok(None),
    }))
}

fn branch(cond: BranchCond, target: u16, resolve: &Resolver<'_>) -> Result<Insn, ClassfileError> {
    Ok(Insn::Branch(cond, resolve(target)?))
}

/// Цель `switch` — номер инструкции: база плюс относительная дельта backend'а.
fn abs(index: u16, delta: i32, resolve: &Resolver<'_>) -> Result<Label, ClassfileError> {
    let absolute = i32::from(index)
        .checked_add(delta)
        .ok_or_else(|| ClassfileError::Parse("переполнение смещения ветвления".into()))?;
    let index = u16::try_from(absolute)
        .map_err(|_| ClassfileError::Parse("смещение ветвления вне диапазона u16".into()))?;
    resolve(index)
}

fn wide(target: i32) -> Result<u16, ClassfileError> {
    u16::try_from(target)
        .map_err(|_| ClassfileError::Parse("широкий переход вне диапазона u16".into()))
}

fn switch_table(
    table: &TableSwitch,
    rel: &dyn Fn(i32) -> Result<Label, ClassfileError>,
) -> Result<Insn, ClassfileError> {
    Ok(Insn::TableSwitch {
        default: rel(table.default)?,
        low: table.low,
        high: table.high,
        targets: table
            .offsets
            .iter()
            .map(|offset| rel(*offset))
            .collect::<Result<_, _>>()?,
    })
}

fn switch_lookup(
    lookup: &LookupSwitch,
    rel: &dyn Fn(i32) -> Result<Label, ClassfileError>,
) -> Result<Insn, ClassfileError> {
    Ok(Insn::LookupSwitch {
        default: rel(lookup.default)?,
        pairs: lookup
            .pairs
            .iter()
            .map(|(key, offset)| Ok((*key, rel(*offset)?)))
            .collect::<Result<_, _>>()?,
    })
}
