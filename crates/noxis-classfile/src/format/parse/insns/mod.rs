//! Backend `Instruction` → [`Insn`](crate::insn::Insn) of our IR.
//!
//! Branch offsets come from the backend (absolute for `if*`/`goto`, relative to
//! the instruction start for `switch`), so conversion takes a resolver from an
//! offset to a [`Label`](crate::insn::Label) — the calling code assigns labels.

mod basic;
mod flow;
mod refs;

use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Instruction;

use crate::error::ClassfileError;
use crate::insn::{Insn, Label};

/// Resolves an absolute bytecode offset into a label.
pub(super) type Resolver<'a> = dyn Fn(u16) -> Result<Label, ClassfileError> + 'a;

/// Convert one instruction. `index` — its number in the method body (`switch` needs it).
pub(super) fn convert(
    instruction: &Instruction,
    index: u16,
    pool: &ConstantPool<'_>,
    resolve: &Resolver<'_>,
) -> Result<Insn, ClassfileError> {
    if let Some(insn) = basic::convert_basic(instruction, pool)? {
        return Ok(insn);
    }
    if let Some(insn) = flow::convert_flow(instruction, index, resolve)? {
        return Ok(insn);
    }
    if let Some(insn) = refs::convert_ref(instruction, pool)? {
        return Ok(insn);
    }
    match instruction {
        Instruction::Breakpoint | Instruction::Impdep1 | Instruction::Impdep2 => Err(
            ClassfileError::Parse("зарезервированный опкод в коде метода".into()),
        ),
        Instruction::Wide => Err(ClassfileError::Parse(
            "wide обязан быть развёрнут парсером backend'а".into(),
        )),
        other => Err(ClassfileError::Parse(format!(
            "неизвестная инструкция backend'а: {other:?}"
        ))),
    }
}
