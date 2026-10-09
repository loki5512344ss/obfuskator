//! # noxis-verify
//!
//! Проверка результата обфускации, а не процесса:
//!
//! * [`leaks`] - утечки исходных имён + секреты открытым текстом;
//! * [`java`] - настоящий `java -Xverify:all` по байтам на диске;
//! * [`metrics`] - счётчики программы, раздувание, стабильность маппингов.

pub mod java;
pub mod leaks;
pub mod metrics;

pub use java::{JavaVerdict, java_available, verify_class_bytes, verify_jar};
pub use leaks::{RED_PATTERNS, SecretHit, check_patterns, collect_names, find_leaks, scan_secrets};
pub use metrics::{
    ProgramStats, check_inflation, inflation_ratio, mappings_changed, program_stats,
};
