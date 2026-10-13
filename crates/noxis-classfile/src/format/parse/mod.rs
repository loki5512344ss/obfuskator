//! Backend `ristretto_classfile` → our IR: pool, instructions, method bodies.
//!
//! Not wired in from anywhere yet (P1): `format` returns `Unimplemented` until
//! class-level `parse`/`write` are ready on top of these converters. The tree
//! is already split by responsibility so wiring is a single `mod parse` with no
//! refactoring.

pub(super) mod annos;
pub(super) mod attrs;
pub(super) mod class;
pub(super) mod code;
pub(super) mod insns;
pub(super) mod pool;
