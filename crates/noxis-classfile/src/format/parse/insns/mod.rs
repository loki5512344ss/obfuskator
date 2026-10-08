//! `Instruction` backend'а → [`Insn`](crate::insn::Insn) нашего IR.
//!
//! Смещения ветвлений приходят из backend'а (абсолютные для `if*`/`goto`,
//! относительные от начала инструкции для `switch`), поэтому конвертация
//! принимает резолвер смещения в [`Label`](crate::insn::Label) — метки
//! назначает вызывающий код.

mod basic;
mod flow;
mod refs;

use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Instruction;

use crate::error::ClassfileError;
use crate::insn::{Insn, Label};

/// Резолвит абсолютное смещение байткода в метку.
pub(super) type Resolver<'a> = dyn Fn(u16) -> Result<Label, ClassfileError> + 'a;

/// Конвертировать одну инструкцию. `index` — её номер в теле метода (нужно `switch`).
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
