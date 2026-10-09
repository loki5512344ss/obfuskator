//! Reading/writing the binary `.class` format (JVMS §4) through the
//! `ristretto_classfile` backend: candidate review and decision in
//! `docs/stack.md`.

mod frames;
mod parse;
pub(crate) mod write;

pub use frames::{Hierarchy, StackMapFrame, VerificationType, compute_frames};

use crate::error::ClassfileError;
use crate::model::ClassFile;

/// Parse one `.class` into a [`ClassFile`].
pub fn parse(bytes: &[u8]) -> Result<ClassFile, ClassfileError> {
    parse::class::parse_class(bytes)
}

/// Write a [`ClassFile`] to bytes, including `Code`/`StackMapTable` recompute.
pub fn write(class: &ClassFile) -> Result<Vec<u8>, ClassfileError> {
    write::write_class(class)
}

/// Classes that were not modified must go to the output archive "as-is".
///
/// This is cheaper and safer than a round-trip through our IR: no risk of
/// losing an unknown attribute. The implementation depends on the backend
/// decision (P1).
pub fn passthrough_needed(class: &ClassFile) -> bool {
    class.methods.is_empty() && class.fields.is_empty()
}
