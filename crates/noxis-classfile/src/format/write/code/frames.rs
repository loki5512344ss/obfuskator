//! Кадры `StackMapTable` при записи: вариант в вариант, дельты по инструкциям.
//!
//! Наш IR хранит `offset_delta` в номерах инструкций; backend при `to_bytes`
//! сам пересчитает их в байты. Компактный формат (`Same`/`SameLocals`) годен
//! только когда **байтовая** дельта ≤ 63 — иначе нужен расширенный вариант.
//! Выбор делаем по байтовой дельте: байты считаются от layout текущего метода.

use ristretto_classfile::attributes::{StackFrame as RtFrame, VerificationType as RtType};

use super::super::pool::Pool;
use crate::error::ClassfileError;
use crate::format::{StackMapFrame, VerificationType};

/// Байтовое смещение начала инструкции (для расчёта дельт кадров).
type ByteOf<'a> = &'a (dyn Fn(u32) -> Result<u16, ClassfileError> + 'a);

/// Кадры IR → кадры backend'а (индексные дельты — как есть).
pub(super) fn convert_frames(
    frames: &[StackMapFrame],
    pool: &mut Pool,
    byte_of: ByteOf<'_>,
) -> Result<Vec<RtFrame>, ClassfileError> {
    let mut out = Vec::with_capacity(frames.len());
    let mut first = true;
    let mut last_instruction: u32 = 0;
    let mut last_byte: u16 = 0;
    for frame in frames {
        let delta = frame_delta(frame);
        let instruction = if first {
            delta
        } else {
            last_instruction + delta + 1
        };
        let byte = byte_of(instruction)?;
        // Байтовая дельта относительно конца предыдущего кадра.
        let byte_delta = if first {
            byte
        } else {
            byte.checked_sub(last_byte)
                .and_then(|d| d.checked_sub(1))
                .ok_or_else(|| ClassfileError::Write("кадры не по порядку".into()))?
        };
        out.push(build_frame(frame, pool, byte_delta)?);
        last_instruction = instruction;
        last_byte = byte;
        first = false;
    }
    Ok(out)
}

/// `byte_delta` — истинная байтовая дельта; компактный вид годен лишь при ≤ 63.
fn build_frame(
    frame: &StackMapFrame,
    pool: &mut Pool,
    byte_delta: u16,
) -> Result<RtFrame, ClassfileError> {
    let delta = frame_delta(frame);
    let extended = u16::try_from(delta)
        .map_err(|_| ClassfileError::Write(format!("дельта кадра {delta} вне u16")))?;
    Ok(match frame {
        StackMapFrame::Same { .. } => {
            if byte_delta <= 63 {
                // Backend при verify()/to_bytes читает frame_type как дельту по
                // ИНСТРУКЦИЯМ и сам пересчитает её в байты (≤ 63 — влезает).
                RtFrame::SameFrame {
                    frame_type: delta as u8,
                }
            } else {
                RtFrame::SameFrameExtended {
                    frame_type: 251,
                    offset_delta: extended,
                }
            }
        }
        StackMapFrame::SameLocals { local, .. } => {
            let stack = vec![vtype(local, pool)?];
            if byte_delta <= 63 {
                RtFrame::SameLocals1StackItemFrame {
                    frame_type: 64 + delta as u8,
                    stack,
                }
            } else {
                RtFrame::SameLocals1StackItemFrameExtended {
                    frame_type: 247,
                    offset_delta: extended,
                    stack,
                }
            }
        }
        StackMapFrame::Chop { count, .. } => RtFrame::ChopFrame {
            frame_type: 251u8.wrapping_sub(*count),
            offset_delta: extended,
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
                offset_delta: extended,
                locals: types(locals, pool)?,
            }
        }
        StackMapFrame::Full { locals, stack, .. } => RtFrame::FullFrame {
            frame_type: 255,
            offset_delta: extended,
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
