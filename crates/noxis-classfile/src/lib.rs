//! # noxis-classfile
//!
//! Собственный IR для классовых файлов JVM: разбор, представление, запись.
//!
//! Почему свой IR, а не работа напрямую с крейтом-парсером: пассам нужна
//! стабильная модель (метки вместо смещений, осознанный набор атрибутов),
//! а не API третьей стороны, который меняется между версиями.
//!
//! Статус: модель и билдер готовы, бинарный parse/write — этап P1
//! (см. `docs/stack.md`).

pub mod error;
pub mod format;
pub mod insn;
pub mod model;

pub use error::ClassfileError;
pub use format::{
    Hierarchy, StackMapFrame, VerificationType, compute_frames, parse, passthrough_needed, write,
};
pub use insn::{
    Arith, ArrayKind, BranchCond, Cond, Const, Convert, Insn, InvokeKind, Label, Prim, SlotKind,
};
pub use model::builder::{ClassBuilder, empty_code};
pub use model::{
    Attribute, BootstrapMethod, ClassFile, Code, ExceptionHandler, Field, InnerClass, LineNumber,
    LocalVariable, LocalVariableType, Method, MethodParameter, Program, RecordComponent, Resource,
    Version,
};
