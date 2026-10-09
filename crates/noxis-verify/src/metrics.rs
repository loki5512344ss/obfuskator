//! Program metrics: counters, inflation, mapping stability.

use noxis_analysis::Analysis;
use noxis_classfile::{Const, Insn, Program};
use noxis_core::Mapping;
use serde::Serialize;

/// Program counters (for the report and budgets).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProgramStats {
    pub classes: usize,
    pub methods: usize,
    pub fields: usize,
    pub insns: usize,
    pub strings: usize,
    pub resources: usize,
    /// Classes kept under their source name (entrypoints, reflection, config).
    /// They stick out next to obfuscated neighbours, so the share is loud.
    pub locked_classes: usize,
}

impl ProgramStats {
    /// The locked share of all classes (`0.0` when there are none).
    pub fn locked_share(&self) -> f64 {
        if self.classes == 0 {
            return 0.0;
        }
        self.locked_classes as f64 / self.classes as f64
    }
}

/// Count the program stats, including the locked-class share.
pub fn program_stats(program: &Program, analysis: &Analysis) -> ProgramStats {
    let mut stats = ProgramStats {
        classes: program.classes.len(),
        resources: program.resources.len(),
        ..ProgramStats::default()
    };
    for class in &program.classes {
        if analysis.is_locked(&class.this_class) {
            stats.locked_classes += 1;
        }
        stats.fields += class.fields.len();
        stats.methods += class.methods.len();
        for method in &class.methods {
            let Some(code) = method.code.as_ref() else {
                continue;
            };
            stats.insns += code.insns.len();
            stats.strings += code
                .insns
                .iter()
                .filter(|insn| matches!(insn, Insn::Const(Const::String(_))))
                .count();
        }
    }
    stats
}

/// The inflation factor (`after / before`).
pub fn inflation_ratio(before_bytes: u64, after_bytes: u64) -> f64 {
    if before_bytes == 0 {
        return f64::INFINITY;
    }
    after_bytes as f64 / before_bytes as f64
}

/// Inflation within budget (`max_ratio`, e.g. `1.5` = +50%).
pub fn check_inflation(before_bytes: u64, after_bytes: u64, max_ratio: f64) -> Result<(), String> {
    let ratio = inflation_ratio(before_bytes, after_bytes);
    if ratio > max_ratio {
        return Err(format!("inflation {ratio:.2} over budget {max_ratio:.2}"));
    }
    Ok(())
}

/// Mappings must change between builds (otherwise the seed does nothing).
pub fn mappings_changed(before: &Mapping, after: &Mapping) -> bool {
    before != after
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_program_stats_are_zero() {
        let stats = program_stats(&Program::default(), &empty_analysis());
        assert_eq!(stats, ProgramStats::default());
        assert_eq!(stats.classes, 0);
    }

    #[test]
    fn inflation_math() {
        assert!((inflation_ratio(100, 150) - 1.5).abs() < f64::EPSILON);
        assert!(inflation_ratio(0, 10).is_infinite());
        assert!(check_inflation(100, 120, 1.5).is_ok());
        assert!(check_inflation(100, 200, 1.5).is_err());
    }

    #[test]
    fn identical_mappings_are_stable() {
        let mapping = Mapping::default();
        assert!(!mappings_changed(&mapping, &mapping));
    }

    /// Locked classes stick out next to obfuscated neighbours: count them.
    #[test]
    fn locked_classes_are_counted() {
        use noxis_analysis::Analysis;
        use noxis_classfile::ClassBuilder;
        use noxis_core::Config;

        let program = Program {
            classes: vec![
                ClassBuilder::new("app/Keep", None).build(),
                ClassBuilder::new("app/Hide", None).build(),
            ],
            resources: Vec::new(),
        };
        let config = Config {
            keep: vec!["app/Keep".to_string()],
            ..Config::default()
        };
        let analysis = Analysis::run(&program, &config).expect("analysis");
        let stats = program_stats(&program, &analysis);
        assert_eq!(stats.classes, 2);
        assert_eq!(stats.locked_classes, 1);
        assert!((stats.locked_share() - 0.5).abs() < f64::EPSILON);
    }

    fn empty_analysis() -> noxis_analysis::Analysis {
        noxis_analysis::Analysis::run(&Program::default(), &noxis_core::Config::default())
            .expect("empty analysis")
    }
}
