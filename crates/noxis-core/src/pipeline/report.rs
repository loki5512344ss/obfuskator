//! Итог прогона: пассы + счётчики - сериализуется в JSON.

use serde::Serialize;

/// Итог прогона: пассы + счётчики - сериализуется в JSON.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub seed: u64,
    pub passes: Vec<crate::pipeline::PassReport>,
    pub notes: Vec<String>,
    /// Произвольные счётчики (`classes`, `strings_encrypted`, ...).
    pub counters: std::collections::BTreeMap<String, usize>,
}

impl Report {
    /// Увеличить счётчик.
    pub fn bump(&mut self, key: &str, by: usize) {
        *self.counters.entry(key.to_string()).or_insert(0) += by;
    }
}
