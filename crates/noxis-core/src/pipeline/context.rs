//! Контекст прогона: конфиг, мастер-сид, маппинги, заметки.

use crate::config::Config;
use crate::mapping::Mapping;
use crate::rng::{Rng, derive_seed};

/// Всё состояние, которое пассы читают и дописывают.
#[derive(Debug, Clone)]
pub struct Context {
    pub config: Config,
    /// Мастер-сид прогона (всегда известен и попадает в отчёт).
    pub master_seed: u64,
    pub mapping: Mapping,
    /// Заметки пассов: почему что-то пропущено, что не удалось.
    pub notes: Vec<String>,
}

impl Context {
    /// Новый контекст. `seed` берётся из конфига или генерируется.
    pub fn new(mut config: Config) -> Self {
        config.sanitize();
        let master_seed = config.seed.unwrap_or_else(fresh_seed);
        Self {
            config,
            master_seed,
            mapping: Mapping::default(),
            notes: Vec::new(),
        }
    }

    /// Детерминированный RNG для домена (`"rename"`, `"strings/0"`, ...).
    ///
    /// Доменное разделение: разные пассы не должны вытягивать одну
    /// последовательность — иначе их «случайности» статистически коррелируют.
    pub fn rng(&self, domain: &str) -> Rng {
        Rng::from_seed(derive_seed(self.master_seed, domain, 0))
    }

    /// RNG с порядковым номером внутри домена.
    pub fn rng_nth(&self, domain: &str, index: u64) -> Rng {
        Rng::from_seed(derive_seed(self.master_seed, domain, index))
    }

    /// Заметка в отчёт (видна в JSON-отчёте и в логе).
    pub fn note(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        tracing::debug!(target: "noxis", "{msg}");
        self.notes.push(msg);
    }
}

/// Нестабильный сид, когда пользователь его не задал.
fn fresh_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5EED_C0DE);
    let mut r = Rng::from_seed(nanos ^ 0xA5A5_A5A5_A5A5_A5A5);
    r.next_u64()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn rng_is_reproducible_from_config_seed() {
        let cfg = Config {
            seed: Some(777),
            ..Config::default()
        };
        let a = Context::new(cfg.clone());
        let b = Context::new(cfg);
        assert_eq!(a.master_seed, b.master_seed);
        let av: Vec<u64> = {
            let mut r = a.rng("rename");
            (0..8).map(|_| r.next_u64()).collect()
        };
        let bv: Vec<u64> = {
            let mut r = b.rng("rename");
            (0..8).map(|_| r.next_u64()).collect()
        };
        assert_eq!(av, bv);
    }

    #[test]
    fn note_is_recorded() {
        let mut ctx = Context::new(Config::default());
        ctx.note("тест");
        assert_eq!(ctx.notes.len(), 1);
    }
}
