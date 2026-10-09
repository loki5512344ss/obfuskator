//! `StackMapTable` frames on read: variant to variant, deltas as-is.
//!
//! The backend stores frames in instruction indices (see
//! `from_bytes_code_attributes`), and so does our IR: the writer recomputes them
//! into bytes on `to_bytes`.

use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::{StackFrame, VerificationType as RtType};

use super::super::pool::class_name;
use crate::error::ClassfileError;
use crate::format::{StackMapFrame, VerificationType};

/// Backend frames → ours (index deltas - as-is).
pub(super) fn to_frames(
    frames: &[StackFrame],
    pool: &ConstantPool<'_>,
) -> Result<Vec<StackMapFrame>, ClassfileError> {
    frames.iter().map(|frame| to_frame(frame, pool)).collect()
}

fn to_frame(frame: &StackFrame, pool: &ConstantPool<'_>) -> Result<StackMapFrame, ClassfileError> {
    Ok(match frame {
        StackFrame::SameFrame { frame_type } => StackMapFrame::Same {
            offset_delta: u32::from(*frame_type),
        },
        StackFrame::SameFrameExtended { offset_delta, .. } => StackMapFrame::Same {
            offset_delta: u32::from(*offset_delta),
        },
        StackFrame::SameLocals1StackItemFrame { frame_type, stack } => StackMapFrame::SameLocals {
            offset_delta: u32::from(*frame_type) - 64,
            local: single(stack, pool)?,
        },
        StackFrame::SameLocals1StackItemFrameExtended {
            offset_delta,
            stack,
            ..
        } => StackMapFrame::SameLocals {
            offset_delta: u32::from(*offset_delta),
            local: single(stack, pool)?,
        },
        StackFrame::ChopFrame {
            frame_type,
            offset_delta,
        } => StackMapFrame::Chop {
            offset_delta: u32::from(*offset_delta),
            count: 251u8.wrapping_sub(*frame_type),
        },
        StackFrame::AppendFrame {
            offset_delta,
            locals,
            ..
        } => StackMapFrame::Append {
            offset_delta: u32::from(*offset_delta),
            locals: types(locals, pool)?,
        },
        StackFrame::FullFrame {
            offset_delta,
            locals,
            stack,
            ..
        } => StackMapFrame::Full {
            offset_delta: u32::from(*offset_delta),
            locals: types(locals, pool)?,
            stack: types(stack, pool)?,
        },
    })
}

/// The frame's single stack element (`same_locals_1_stack_item`).
fn single(stack: &[RtType], pool: &ConstantPool<'_>) -> Result<VerificationType, ClassfileError> {
    let mut types = types(stack, pool)?.into_iter();
    match (types.next(), types.next()) {
        (Some(first), None) => Ok(first),
        _ => Err(ClassfileError::Parse(
            "в same_locals-кадре не один элемент стека".into(),
        )),
    }
}

fn types(
    values: &[RtType],
    pool: &ConstantPool<'_>,
) -> Result<Vec<VerificationType>, ClassfileError> {
    values.iter().map(|value| vtype(value, pool)).collect()
}

/// Backend verification type → our type (the class name comes from the constant pool).
fn vtype(value: &RtType, pool: &ConstantPool<'_>) -> Result<VerificationType, ClassfileError> {
    Ok(match value {
        RtType::Top => VerificationType::Top,
        RtType::Integer => VerificationType::Integer,
        RtType::Float => VerificationType::Float,
        RtType::Double => VerificationType::Double,
        RtType::Long => VerificationType::Long,
        RtType::Null => VerificationType::Null,
        RtType::UninitializedThis => VerificationType::UninitializedThis,
        RtType::Object { cpool_index } => VerificationType::Object(class_name(pool, *cpool_index)?),
        RtType::Uninitialized { offset } => VerificationType::Uninitialized(*offset),
    })
}

/// The catch type (`0` - catch-all for `finally`/`any`).
pub(super) fn catch_type(
    pool: &ConstantPool<'_>,
    index: u16,
) -> Result<Option<String>, ClassfileError> {
    if index == 0 {
        Ok(None)
    } else {
        Ok(Some(class_name(pool, index)?))
    }
}
