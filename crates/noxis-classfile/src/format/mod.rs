//! Чтение/запись бинарного формата `.class` (JVMS §4) через backend
//! `ristretto_classfile`: обзор кандидатов и решение — `docs/stack.md`.

mod frames;
mod parse;
pub(crate) mod write;

pub use frames::{Hierarchy, StackMapFrame, VerificationType, compute_frames};

use crate::error::ClassfileError;
use crate::model::ClassFile;

/// Разобрать один `.class` в [`ClassFile`].
pub fn parse(bytes: &[u8]) -> Result<ClassFile, ClassfileError> {
    parse::class::parse_class(bytes)
}

/// Записать [`ClassFile`] в байты, включая пересчёт `Code`/`StackMapTable`.
pub fn write(class: &ClassFile) -> Result<Vec<u8>, ClassfileError> {
    write::write_class(class)
}

/// Классы, которые не изменялись, должны уезжать в выходной архив «как есть».
///
/// Это дешевле и безопаснее, чем round-trip через наш IR: нет риска потерять
/// неизвестный атрибут. Реализация зависит от решения по backend (P1).
pub fn passthrough_needed(class: &ClassFile) -> bool {
    class.methods.is_empty() && class.fields.is_empty()
}
