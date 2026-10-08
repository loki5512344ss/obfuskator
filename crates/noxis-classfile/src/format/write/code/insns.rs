//! Наш [`Insn`](crate::insn::Insn) → `Instruction` backend'а.
//!
//! Обратная сторона `super::super::parse::insns`: метки резолвятся в
//! абсолютные номера инструкций (`switch` — в дельты от себя), константы
//! и ссылки уходят в пул через [`Pool`](super::super::pool::Pool).

use indexmap::IndexMap;
use ristretto_classfile::attributes::{Instruction as RtInsn, LookupSwitch, TableSwitch};

use super::super::pool::Pool;
use super::ops::{array_load, array_store, array_type, binary, cmp, convert_op, int_const, invoke};
use super::{branch, delta, fit_index, ret_op};
use crate::error::ClassfileError;
use crate::insn::{Insn, Label, SlotKind};

/// Одна инструкция IR → инструкция backend'а (`None` — маркер метки).
///
/// `index` — номер инструкции в методе (нужен `switch`-дельтам).
pub(crate) fn convert(
    insn: &Insn,
    index: u32,
    pool: &mut Pool,
    resolve: &impl Fn(Label) -> Result<u32, ClassfileError>,
) -> Result<Option<RtInsn>, ClassfileError> {
    Ok(Some(match insn {
        Insn::Mark(_) => return Ok(None),
        Insn::Nop => RtInsn::Nop,
        Insn::AconstNull => RtInsn::Aconst_null,
        Insn::Const(value) => int_const(value, pool)?,
        Insn::Load { slot, kind } => load(*slot, *kind),
        Insn::Store { slot, kind } => store(*slot, *kind),
        Insn::Iinc { index, delta } => iinc(*index, *delta)?,
        Insn::ArrayLoad(kind) => array_load(*kind),
        Insn::ArrayStore(kind) => array_store(*kind),
        Insn::Pop => RtInsn::Pop,
        Insn::Pop2 => RtInsn::Pop2,
        Insn::Dup => RtInsn::Dup,
        Insn::DupX1 => RtInsn::Dup_x1,
        Insn::DupX2 => RtInsn::Dup_x2,
        Insn::Dup2 => RtInsn::Dup2,
        Insn::Dup2X1 => RtInsn::Dup2_x1,
        Insn::Dup2X2 => RtInsn::Dup2_x2,
        Insn::Swap => RtInsn::Swap,
        Insn::Binary { prim, op } => binary(*prim, *op)?,
        Insn::Cmp { prim, nan_great } => cmp(*prim, *nan_great)?,
        Insn::Convert(op) => convert_op(*op),
        Insn::Branch(cond, target) => branch(*cond, resolve(*target)?)?,
        Insn::Goto(target) => RtInsn::Goto(fit_index(resolve(*target)?)?),
        Insn::Jsr(target) => RtInsn::Jsr(fit_index(resolve(*target)?)?),
        Insn::Ret(slot) => ret(*slot),
        Insn::TableSwitch {
            default,
            low,
            high,
            targets,
        } => RtInsn::Tableswitch(Box::new(TableSwitch {
            default: delta(index, resolve(*default)?)?,
            low: *low,
            high: *high,
            offsets: targets
                .iter()
                .map(|target| delta(index, resolve(*target)?))
                .collect::<Result<_, _>>()?,
        })),
        Insn::LookupSwitch { default, pairs } => RtInsn::Lookupswitch(Box::new(LookupSwitch {
            default: delta(index, resolve(*default)?)?,
            pairs: pairs
                .iter()
                .map(|(key, target)| Ok((*key, delta(index, resolve(*target)?)?)))
                .collect::<Result<IndexMap<_, _>, ClassfileError>>()?,
        })),
        Insn::Return(prim) => ret_op(*prim),
        Insn::AReturn => RtInsn::Areturn,
        Insn::GetStatic {
            owner,
            name,
            descriptor,
        } => RtInsn::Getstatic(pool.field_ref(owner, name, descriptor)?),
        Insn::PutStatic {
            owner,
            name,
            descriptor,
        } => RtInsn::Putstatic(pool.field_ref(owner, name, descriptor)?),
        Insn::GetField {
            owner,
            name,
            descriptor,
        } => RtInsn::Getfield(pool.field_ref(owner, name, descriptor)?),
        Insn::PutField {
            owner,
            name,
            descriptor,
        } => RtInsn::Putfield(pool.field_ref(owner, name, descriptor)?),
        Insn::Invoke {
            kind,
            owner,
            name,
            descriptor,
            ..
        } => invoke(*kind, owner, name, descriptor, pool)?,
        Insn::InvokeDynamic {
            name,
            descriptor,
            bootstrap,
        } => RtInsn::Invokedynamic(pool.invoke_dynamic(*bootstrap, name, descriptor)?),
        Insn::New(name) => RtInsn::New(pool.class(name)?),
        Insn::NewArray(kind) => RtInsn::Newarray(array_type(*kind)?),
        Insn::ANewArray(name) => RtInsn::Anewarray(pool.class(name)?),
        Insn::MultiANewArray { descriptor, dims } => {
            RtInsn::Multianewarray(pool.class(descriptor)?, *dims)
        }
        Insn::ArrayLength => RtInsn::Arraylength,
        Insn::AThrow => RtInsn::Athrow,
        Insn::CheckCast(name) => RtInsn::Checkcast(pool.class(name)?),
        Insn::InstanceOf(name) => RtInsn::Instanceof(pool.class(name)?),
        Insn::MonitorEnter => RtInsn::Monitorenter,
        Insn::MonitorExit => RtInsn::Monitorexit,
    }))
}

fn load(slot: u16, kind: SlotKind) -> RtInsn {
    match (kind, slot) {
        (SlotKind::Int, 0) => RtInsn::Iload_0,
        (SlotKind::Int, 1) => RtInsn::Iload_1,
        (SlotKind::Int, 2) => RtInsn::Iload_2,
        (SlotKind::Int, 3) => RtInsn::Iload_3,
        (SlotKind::Long, 0) => RtInsn::Lload_0,
        (SlotKind::Long, 1) => RtInsn::Lload_1,
        (SlotKind::Long, 2) => RtInsn::Lload_2,
        (SlotKind::Long, 3) => RtInsn::Lload_3,
        (SlotKind::Float, 0) => RtInsn::Fload_0,
        (SlotKind::Float, 1) => RtInsn::Fload_1,
        (SlotKind::Float, 2) => RtInsn::Fload_2,
        (SlotKind::Float, 3) => RtInsn::Fload_3,
        (SlotKind::Double, 0) => RtInsn::Dload_0,
        (SlotKind::Double, 1) => RtInsn::Dload_1,
        (SlotKind::Double, 2) => RtInsn::Dload_2,
        (SlotKind::Double, 3) => RtInsn::Dload_3,
        (SlotKind::Ref, 0) => RtInsn::Aload_0,
        (SlotKind::Ref, 1) => RtInsn::Aload_1,
        (SlotKind::Ref, 2) => RtInsn::Aload_2,
        (SlotKind::Ref, 3) => RtInsn::Aload_3,
        (SlotKind::Int, s) => narrow(s, RtInsn::Iload, RtInsn::Iload_w),
        (SlotKind::Long, s) => narrow(s, RtInsn::Lload, RtInsn::Lload_w),
        (SlotKind::Float, s) => narrow(s, RtInsn::Fload, RtInsn::Fload_w),
        (SlotKind::Double, s) => narrow(s, RtInsn::Dload, RtInsn::Dload_w),
        (SlotKind::Ref, s) => narrow(s, RtInsn::Aload, RtInsn::Aload_w),
    }
}

fn store(slot: u16, kind: SlotKind) -> RtInsn {
    match (kind, slot) {
        (SlotKind::Int, 0) => RtInsn::Istore_0,
        (SlotKind::Int, 1) => RtInsn::Istore_1,
        (SlotKind::Int, 2) => RtInsn::Istore_2,
        (SlotKind::Int, 3) => RtInsn::Istore_3,
        (SlotKind::Long, 0) => RtInsn::Lstore_0,
        (SlotKind::Long, 1) => RtInsn::Lstore_1,
        (SlotKind::Long, 2) => RtInsn::Lstore_2,
        (SlotKind::Long, 3) => RtInsn::Lstore_3,
        (SlotKind::Float, 0) => RtInsn::Fstore_0,
        (SlotKind::Float, 1) => RtInsn::Fstore_1,
        (SlotKind::Float, 2) => RtInsn::Fstore_2,
        (SlotKind::Float, 3) => RtInsn::Fstore_3,
        (SlotKind::Double, 0) => RtInsn::Dstore_0,
        (SlotKind::Double, 1) => RtInsn::Dstore_1,
        (SlotKind::Double, 2) => RtInsn::Dstore_2,
        (SlotKind::Double, 3) => RtInsn::Dstore_3,
        (SlotKind::Ref, 0) => RtInsn::Astore_0,
        (SlotKind::Ref, 1) => RtInsn::Astore_1,
        (SlotKind::Ref, 2) => RtInsn::Astore_2,
        (SlotKind::Ref, 3) => RtInsn::Astore_3,
        (SlotKind::Int, s) => narrow(s, RtInsn::Istore, RtInsn::Istore_w),
        (SlotKind::Long, s) => narrow(s, RtInsn::Lstore, RtInsn::Lstore_w),
        (SlotKind::Float, s) => narrow(s, RtInsn::Fstore, RtInsn::Fstore_w),
        (SlotKind::Double, s) => narrow(s, RtInsn::Dstore, RtInsn::Dstore_w),
        (SlotKind::Ref, s) => narrow(s, RtInsn::Astore, RtInsn::Astore_w),
    }
}

/// Короткая форма при слоте ≤255, иначе `wide`-форма.
fn narrow(slot: u16, short: fn(u8) -> RtInsn, wide: fn(u16) -> RtInsn) -> RtInsn {
    u8::try_from(slot).map(short).unwrap_or_else(|_| wide(slot))
}

fn ret(slot: u16) -> RtInsn {
    narrow(slot, RtInsn::Ret, RtInsn::Ret_w)
}

fn iinc(index: u16, delta: i32) -> Result<RtInsn, ClassfileError> {
    if let (Ok(i), Ok(d)) = (u8::try_from(index), i8::try_from(delta)) {
        return Ok(RtInsn::Iinc(i, d));
    }
    let wide_delta = i16::try_from(delta)
        .map_err(|_| ClassfileError::Write(format!("приращение iinc {delta} не влезает в i16")))?;
    Ok(RtInsn::Iinc_w(index, wide_delta))
}
