//! Backend `ristretto_classfile` → наш IR: пул, инструкции, тела методов.
//!
//! Пока ниоткуда не подключён (P1): `format` вернёт `Unimplemented`, пока не
//! будут готовы `parse`/`write` классового уровня поверх этих конвертеров.
//! Дерево уже разбито по ответственностям, чтобы подключение было одним
//! `mod parse` без рефакторинга.

pub(super) mod attrs;
pub(super) mod class;
pub(super) mod code;
pub(super) mod insns;
pub(super) mod pool;
