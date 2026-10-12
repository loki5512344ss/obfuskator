//! # noxis-analysis
//!
//! Анализ программы перед обфускацией: иерархия наследования, entrypoint'ы,
//! рефлексивные обращения по имени, keep-правила.
//!
//! Из этих данных пасс переименования узнаёт, **что** трогать нельзя.
//! Урок из `info/evo-weaknesses.md`: у EvoVis рефлексия по имени была
//! открытым текстом, а entrypoint'ы - в `fabric.mod.json`; здесь они
//! обязаны попадать в keep-набор автоматически.

pub mod entrypoints;
pub mod hierarchy;
pub mod jdk;
pub mod keep;

pub use entrypoints::Entrypoints;
pub use hierarchy::{ClassHierarchy, RenameSafety, method_rename_safety};
pub use jdk::JdkHierarchy;
pub use keep::{Analysis, KeepRules, RuleError, external_dispatch_families};
