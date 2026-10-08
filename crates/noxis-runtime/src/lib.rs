//! # noxis-runtime
//!
//! Схемы шифрования строк и ABI рантайм-декодера, который получает байткод.

pub mod codec;
pub mod decoder;
pub mod scheme;

pub use decoder::{DECODER_DESC, DECODER_METHOD, decoder_class, decoder_owner};
pub use scheme::{Blob, Scheme, decrypt, encrypt};
