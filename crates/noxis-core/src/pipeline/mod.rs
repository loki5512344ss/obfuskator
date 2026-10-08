//! Конвейер пассов: единый порядок исполнения и отчёт.

pub mod context;
pub mod report;

use serde::Serialize;
use noxis_classfile::Program;

use self::context::Context;
use self::report::Report;
use crate::config::PassId;
use crate::error::CoreError;

/// Результат одного пасса.
#[derive(Debug, Clone, Serialize)]
pub struct PassReport {
    pub pass: PassId,
    /// Сколько объектов трансформировано.
    pub transformed: usize,
    /// Сколько объектов сознательно пропущено.
    pub skipped: usize,
    /// Человекочитаемые заметки пасса.
    pub notes: Vec<String>,
}

impl PassReport {
    /// Пустой отчёт (пасс ещё не реализован).
    pub fn not_implemented(pass: PassId, todo: &'static str) -> Self {
        Self {
            pass,
            transformed: 0,
            skipped: 0,
            notes: vec![format!("не реализовано: {todo}")],
        }
    }
}

/// Единый контракт трансформации программы.
pub trait Pass: Send + Sync {
    /// Идентификатор пасса (совпадает с ключом в конфиге).
    fn id(&self) -> PassId;

    /// Применить пасс. Порядок пассов задаёт [`Pipeline`].
    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError>;
}

/// Упорядоченная цепочка пассов.
///
/// Порядок — не деталь: см. `docs/architecture.md` (rename → resources →
/// strings → constants → flow → noise → debug), иначе часть техник ломает
/// предыдущие или работает по уже мёртвым именам.
#[derive(Default)]
pub struct Pipeline {
    passes: Vec<Box<dyn Pass>>,
}

impl Pipeline {
    /// Пустой конвейер.
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавить пасс в конец.
    pub fn push(mut self, pass: Box<dyn Pass>) -> Self {
        self.passes.push(pass);
        self
    }

    /// Число пассов в цепочке.
    pub fn len(&self) -> usize {
        self.passes.len()
    }

    /// Пуст ли конвейер.
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }

    /// Прогнать всю программу через все пассы.
    pub fn run(&self, ctx: &mut Context, program: &mut Program) -> Result<Report, CoreError> {
        let mut report = Report {
            seed: ctx.master_seed,
            ..Report::default()
        };
        report.bump("classes", program.classes.len());
        report.bump("resources", program.resources.len());

        for pass in &self.passes {
            let id = pass.id();
            tracing::info!(target: "noxis", pass = %id, "пасс запущен");
            let result = pass.apply(ctx, program)?;
            tracing::info!(
                target: "noxis",
                pass = %id,
                transformed = result.transformed,
                skipped = result.skipped,
                "пасс завершён"
            );
            report.passes.push(result);
        }

        report.notes = std::mem::take(&mut ctx.notes);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    struct Stub(PassId);

    impl Pass for Stub {
        fn id(&self) -> PassId {
            self.0
        }

        fn apply(
            &self,
            _ctx: &mut Context,
            _program: &mut Program,
        ) -> Result<PassReport, CoreError> {
            Ok(PassReport {
                pass: self.0,
                transformed: 1,
                skipped: 0,
                notes: Vec::new(),
            })
        }
    }

    #[test]
    fn runs_in_order_and_collects_reports() {
        let pipeline = Pipeline::new()
            .push(Box::new(Stub(PassId::Rename)))
            .push(Box::new(Stub(PassId::Strings)));
        let mut ctx = Context::new(crate::config::Config::default());
        let mut program = Program::default();
        let report = pipeline.run(&mut ctx, &mut program).unwrap();
        assert_eq!(report.passes.len(), 2);
        assert_eq!(report.passes[0].pass, PassId::Rename);
        assert_eq!(report.passes[1].pass, PassId::Strings);
        assert_eq!(report.counters.get("classes"), Some(&0));
    }
}
