//! # noxis-jar
//!
//! Чтение и запись JAR: ZIP-архив, классы, ресурсы, подписи.
//!
//! Слой разделён на три несвязанные задачи:
//!
//! * [`archive`] - чистая работа с ZIP: порядок записей, сжатие, дубликаты;
//! * [`signatures`] - срез невалидных подписей `META-INF/*.SF|RSA|DSA|EC`;
//! * [`loader`] - перевод архива в [`Program`](noxis_classfile::Program)
//!   и обратно (классы через [`format`](noxis_classfile::format), остальное
//!   как ресурсы).
//!
//! Обфускация ресурсов - пасс `resources` в `noxis-passes`; сюда она не
//! входит, потому что ей нужен маппинг имён, а не I/O.

pub mod archive;
pub mod loader;
pub mod signatures;

pub use archive::{Entry, JarError, read_entries, read_file, write_entries, write_file};
pub use loader::{load, save};
pub use signatures::{is_signature_entry, strip_signatures};
