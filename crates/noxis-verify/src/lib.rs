//! # noxis-verify
//!
//! Проверка результата обфускации, а не процесса:
//!
//! * [`leaks`] - утечки исходных имён + секреты открытым текстом;
//! * [`java`] - настоящий `java -Xverify:all` по байтам на диске;
//! * [`metrics`] - счётчики программы, раздувание, стабильность маппингов;
//! * [`adversary`] - противник-скрипт: generic свёртка констант как
//!   честная мера (§7) и как oracle корректности схем `Constants`.

pub mod adversary;
pub mod java;
pub mod leaks;
pub mod metrics;

pub use adversary::{FoldedConstant, fold_constants};
pub use java::{
    DecompileReport, JavaVerdict, decompile_report, jadx_available, java_available,
    verify_class_bytes, verify_jar,
};
pub use leaks::{RED_PATTERNS, SecretHit, check_patterns, collect_names, find_leaks, scan_secrets};
pub use metrics::{
    ProgramStats, RunOutput, check_inflation, check_slowdown, inflation_ratio, mappings_changed,
    measure_runtime, program_stats, run_java, slowdown_ratio,
};
