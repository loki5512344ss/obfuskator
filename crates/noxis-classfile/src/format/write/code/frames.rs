//! Кадры `StackMapTable` при записи: вариант в вариант, дельты как есть.
//!
//! Наш IR хранит кадры в номерах инструкций (как backend после чтения) —
//! backend сам пересчитает их в байты при `to_bytes`.

use ristretto_classfile::attributes::{StackFrame as RtFrame, VerificationType as RtType};

use super::super::pool::Pool;
use crate::error::ClassfileError;
use crate::format::{StackMapFrame, VerificationType};

/// Кадры IR → кадры backend'а (индексные дельты — как есть).
pub(super) fn convert_frames(
    frames: &[StackMapFrame],
    pool: &mut Pool,
) -> Result<Vec<RtFrame>, ClassfileError> {
    frames
        .iter()
        .map(|frame| build_frame(frame, pool))
        .collect()
}

fn build_frame(frame: &StackMapFrame, pool: &mut Pool) -> Result<RtFrame, ClassfileError> {
    let delta = frame_delta(frame);
    let offset = u16::try_from(delta)
        .map_err(|_| ClassfileError::Write(format!("дельта кадра {delta} вне u16")))?;
    Ok(match frame {
        StackMapFrame::Same { .. } => {
            if delta <= 63 {
                RtFrame::SameFrame {
                    frame_type: delta as u8,
                }
            } else {
                RtFrame::SameFrameExtended {
                    frame_type: 251,
                    offset_delta: offset,
                }
            }
        }
        StackMapFrame::SameLocals { local, .. } => {
            let stack = vec![vtype(local, pool)?];
            if delta <= 63 {
                RtFrame::SameLocals1StackItemFrame {
                    frame_type: 64 + delta as u8,
                    stack,
                }
            } else {
                RtFrame::SameLocals1StackItemFrameExtended {
                    frame_type: 247,
                    offset_delta: offset,
                    stack,
                }
            }
        }
        StackMapFrame::Chop { count, .. } => RtFrame::ChopFrame {
            frame_type: 251u8.wrapping_sub(*count),
            offset_delta: offset,
        },
        StackMapFrame::Append { locals, .. } => {
            let count = u8::try_from(locals.len()).map_err(|_| {
                ClassfileError::Write(format!("append-кадр на {} local'ов", locals.len()))
            })?;
            if !(1..=3).contains(&count) {
                return Err(ClassfileError::Write(format!(
                    "append-кадр на {count} local'ов (нужно 1–3)"
                )));
            }
            RtFrame::AppendFrame {
                frame_type: 251 + count,
                offset_delta: offset,
                locals: types(locals, pool)?,
            }
        }
        StackMapFrame::Full { locals, stack, .. } => RtFrame::FullFrame {
            frame_type: 255,
            offset_delta: offset,
            locals: types(locals, pool)?,
            stack: types(stack, pool)?,
        },
    })
}

fn frame_delta(frame: &StackMapFrame) -> u32 {
    match *frame {
        StackMapFrame::Same { offset_delta } => offset_delta,
        StackMapFrame::SameLocals { offset_delta, .. } => offset_delta,
        StackMapFrame::Chop { offset_delta, .. } => offset_delta,
        StackMapFrame::Append { offset_delta, .. } => offset_delta,
        StackMapFrame::Full { offset_delta, .. } => offset_delta,
    }
}

fn types(values: &[VerificationType], pool: &mut Pool) -> Result<Vec<RtType>, ClassfileError> {
    values.iter().map(|value| vtype(value, pool)).collect()
}

fn vtype(value: &VerificationType, pool: &mut Pool) -> Result<RtType, ClassfileError> {
    Ok(match value {
        VerificationType::Top => RtType::Top,
        VerificationType::Integer => RtType::Integer,
        VerificationType::Float => RtType::Float,
        VerificationType::Double => RtType::Double,
        VerificationType::Long => RtType::Long,
        VerificationType::Null => RtType::Null,
        VerificationType::UninitializedThis => RtType::UninitializedThis,
        VerificationType::Object(name) => RtType::Object {
            cpool_index: pool.class(name)?,
        },
        VerificationType::Uninitialized(offset) => RtType::Uninitialized { offset: *offset },
    })
}
