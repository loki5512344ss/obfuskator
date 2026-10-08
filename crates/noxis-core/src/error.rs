//! Ошибки ядра пайплайна.

use crate::config::PassId;

/// Ошибка уровня ядра.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),

    #[error("некорректная конфигурация: {0}")]
    Config(String),

    #[error("ошибка классового файла: {0}")]
    Classfile(#[from] noxis_classfile::ClassfileError),

    #[error("пасс `{pass}` не смог выполниться: {msg}")]
    Pass { pass: PassId, msg: String },
}

impl CoreError {
    /// Ошибка конкретного пасса.
    pub fn pass(pass: PassId, msg: impl Into<String>) -> Self {
        Self::Pass {
            pass,
            msg: msg.into(),
        }
    }
}
