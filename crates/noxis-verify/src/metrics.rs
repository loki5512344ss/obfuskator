//! Метрики программы: счётчики, раздувание, стабильность маппингов.

use noxis_classfile::{Const, Insn, Program};
use noxis_core::Mapping;
use serde::Serialize;

/// Счётчики программы (для отчёта и бюджетов).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProgramStats {
    pub classes: usize,
    pub methods: usize,
    pub fields: usize,
    pub insns: usize,
    pub strings: usize,
    pub resources: usize,
}

/// Посчитать статистику по программе.
pub fn program_stats(program: &Program) -> ProgramStats {
    let mut stats = ProgramStats {
        classes: program.classes.len(),
        resources: program.resources.len(),
        ..ProgramStats::default()
    };
    for class in &program.classes {
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

/// Коэффициент раздувания (`after / before`).
pub fn inflation_ratio(before_bytes: u64, after_bytes: u64) -> f64 {
    if before_bytes == 0 {
        return f64::INFINITY;
    }
    after_bytes as f64 / before_bytes as f64
}

/// Раздувание в бюджете (`max_ratio`, например `1.5` = +50%).
pub fn check_inflation(before_bytes: u64, after_bytes: u64, max_ratio: f64) -> Result<(), String> {
    let ratio = inflation_ratio(before_bytes, after_bytes);
    if ratio > max_ratio {
        return Err(format!(
            "раздувание {ratio:.2} сверх бюджета {max_ratio:.2}"
        ));
    }
    Ok(())
}

/// Маппинги обязаны меняться между сборками (иначе сид не работает).
pub fn mappings_changed(before: &Mapping, after: &Mapping) -> bool {
    before != after
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_program_stats_are_zero() {
        let stats = program_stats(&Program::default());
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
}
