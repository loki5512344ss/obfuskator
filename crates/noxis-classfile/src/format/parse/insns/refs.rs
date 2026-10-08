//! Поля, вызовы и создание объектов: индексы пула → строки IR.

use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::{ArrayType, Instruction};

use super::super::pool::{class_name, dynamic_parts, expect_kind, is_interface, ref_parts};
use crate::error::ClassfileError;
use crate::insn::{ArrayKind, Insn, InvokeKind};

/// `Some` — инструкция покрыта, `None` — отдать следующим конвертерам.
pub(super) fn convert_ref(
    instruction: &Instruction,
    pool: &ConstantPool<'_>,
) -> Result<Option<Insn>, ClassfileError> {
    Ok(Some(match instruction {
        Instruction::Getstatic(i) => {
            let (owner, name, descriptor) = ref_parts(pool, *i)?;
            Insn::GetStatic {
                owner,
                name,
                descriptor,
            }
        }
        Instruction::Putstatic(i) => {
            let (owner, name, descriptor) = ref_parts(pool, *i)?;
            Insn::PutStatic {
                owner,
                name,
                descriptor,
            }
        }
        Instruction::Getfield(i) => {
            let (owner, name, descriptor) = ref_parts(pool, *i)?;
            Insn::GetField {
                owner,
                name,
                descriptor,
            }
        }
        Instruction::Putfield(i) => {
            let (owner, name, descriptor) = ref_parts(pool, *i)?;
            Insn::PutField {
                owner,
                name,
                descriptor,
            }
        }
        Instruction::Invokevirtual(i) => invoke(pool, *i, InvokeKind::Virtual)?,
        Instruction::Invokespecial(i) => invoke(pool, *i, InvokeKind::Special)?,
        Instruction::Invokestatic(i) => invoke(pool, *i, InvokeKind::Static)?,
        Instruction::Invokeinterface(i, _) => invoke(pool, *i, InvokeKind::Interface)?,
        Instruction::Invokedynamic(i) => dynamic(pool, *i)?,
        Instruction::New(i) => Insn::New(class_name(pool, *i)?),
        Instruction::Newarray(t) => Insn::NewArray(array_kind(t.clone())),
        Instruction::Anewarray(i) => Insn::ANewArray(class_name(pool, *i)?),
        Instruction::Multianewarray(i, dims) => Insn::MultiANewArray {
            descriptor: class_name(pool, *i)?,
            dims: *dims,
        },
        Instruction::Arraylength => Insn::ArrayLength,
        Instruction::Athrow => Insn::AThrow,
        Instruction::Checkcast(i) => Insn::CheckCast(class_name(pool, *i)?),
        Instruction::Instanceof(i) => Insn::InstanceOf(class_name(pool, *i)?),
        Instruction::Monitorenter => Insn::MonitorEnter,
        Instruction::Monitorexit => Insn::MonitorExit,
        _ => return Ok(None),
    }))
}

fn invoke(pool: &ConstantPool<'_>, index: u16, kind: InvokeKind) -> Result<Insn, ClassfileError> {
    expect_kind(pool, index, kind)?;
    let (owner, name, descriptor) = ref_parts(pool, index)?;
    Ok(Insn::Invoke {
        kind,
        owner,
        name,
        descriptor,
        itf: is_interface(pool, index),
    })
}

fn dynamic(pool: &ConstantPool<'_>, index: u16) -> Result<Insn, ClassfileError> {
    let (name, descriptor, bootstrap) = dynamic_parts(pool, index)?;
    Ok(Insn::InvokeDynamic {
        name,
        descriptor,
        bootstrap,
    })
}

fn array_kind(kind: ArrayType) -> ArrayKind {
    match kind {
        ArrayType::Boolean => ArrayKind::Boolean,
        ArrayType::Char => ArrayKind::Char,
        ArrayType::Float => ArrayKind::Float,
        ArrayType::Double => ArrayKind::Double,
        ArrayType::Byte => ArrayKind::Byte,
        ArrayType::Short => ArrayKind::Short,
        ArrayType::Int => ArrayKind::Int,
        ArrayType::Long => ArrayKind::Long,
    }
}
