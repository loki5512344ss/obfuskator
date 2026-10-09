//! # noxis-classfile
//!
//! Own IR for JVM class files: parsing, representation, writing.
//!
//! Why a custom IR instead of working directly with a parser crate: passes
//! need a stable model (labels instead of offsets, a deliberate attribute
//! set), not a third-party API that changes between versions.
//!
//! Status: model and builder are ready; binary parse/write is stage P1
//! (see `docs/stack.md`).

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
