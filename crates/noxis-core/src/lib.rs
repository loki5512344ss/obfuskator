//! # noxis-core
//!
//! Ядро обфускатора: конфигурация, детерминированный seed, маппинги,
//! контекст прогона и контракт пассов ([`pipeline::Pass`]).
//!
//! Ядро ничего не знает о бинарном формате `.class` и о ZIP - только о модели
//! [`noxis_classfile::Program`]. Сборка конвейера из пассов живёт в
//! `noxis-passes`, чтобы не было цикла зависимостей.

pub mod config;
pub mod error;
pub mod mapping;
pub mod pipeline;
pub mod rng;

pub use config::sections::{
    ConstantsConfig, DebugConfig, FlowConfig, NameStyle, NoiseConfig, RenameConfig, StringsConfig,
};
pub use config::{Config, PassId, Profile};
pub use error::CoreError;
pub use mapping::{Mapping, field_key, method_key};
pub use pipeline::context::Context;
pub use pipeline::report::Report;
pub use pipeline::{Pass, PassReport, Pipeline};
pub use rng::{Rng, derive_seed};
