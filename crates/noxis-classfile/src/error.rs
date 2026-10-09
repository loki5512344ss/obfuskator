//! Errors from class-file parsing/writing.

/// An error at the level of a single class file.
#[derive(Debug, thiserror::Error)]
pub enum ClassfileError {
    #[error("не удалось разобрать классовый файл: {0}")]
    Parse(String),

    #[error("не удалось записать классовый файл: {0}")]
    Write(String),

    #[error("некорректный байткод {class}: {msg}")]
    InvalidCode { class: String, msg: String },

    #[error("не реализовано: {0}")]
    Unimplemented(&'static str),
}
