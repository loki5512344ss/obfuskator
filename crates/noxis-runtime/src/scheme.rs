//! Схемы шифрования строк: пер-строчный ключ, nonce и несколько алгоритмов.
//!
//! Требования (см. `info/evo-weaknesses.md` и `docs/techniques.md`):
//! - **разные схемы между строками** - один скрипт не покрывает весь jar;
//! - **ключ не рядом с шифротекстом** - на вызов передаётся только salt,
//!   мастер-секрет живёт в рантайм-декодере (сборка мастер-секрета - P2);
//! - **nonce уникален на строку** - keystream не повторяется;
//! - **MAC** - повреждение/подмена шифротекста ловится, а не молча портит текст.

use noxis_core::{Rng, derive_seed};

use crate::codec;

/// Ошибка схемы.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchemeError {
    #[error("неизвестная схема `{0}`")]
    UnknownScheme(String),
    #[error("повреждённый шифротекст")]
    CorruptCiphertext,
}

/// Поддерживаемые схемы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// XOR со keystream из xoshiro-like генератора (быстро, дёшево).
    XorSplitmix,
    /// XTEA в режиме счётчика (блочная, другая структура шифротекста).
    XteaCtr,
}

impl Scheme {
    /// Имя схемы, как она пишется в конфиге.
    pub const fn name(self) -> &'static str {
        match self {
            Scheme::XorSplitmix => "xor-splitmix",
            Scheme::XteaCtr => "xtea-ctr",
        }
    }

    /// Разбор имени из конфига.
    pub fn parse(name: &str) -> Result<Self, SchemeError> {
        match name {
            "xor-splitmix" => Ok(Scheme::XorSplitmix),
            "xtea-ctr" => Ok(Scheme::XteaCtr),
            other => Err(SchemeError::UnknownScheme(other.to_string())),
        }
    }

    /// Все схемы в порядке объявления (для ротации).
    pub const fn all() -> [Scheme; 2] {
        [Scheme::XorSplitmix, Scheme::XteaCtr]
    }
}

/// Зашифрованный блоб: шифротекст (base64) + tag для целостности.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    /// base64 шифротекста.
    pub cipher: String,
    /// 32-битный MAC по ключу от plaintext - ловит подмену/порчу.
    pub tag: u32,
}

/// Ключ строки из мастер-секрета и salt (то же вычисление делает Java-декодер).
pub fn string_key(master: u64, salt: i32) -> u64 {
    derive_seed(master, "string/key", u64::from(salt as u32))
}

/// Nonce строки: детерминированный от salt, но другой домен - keystream не
/// пересекается даже при совпадении ключей.
pub fn string_nonce(master: u64, salt: i32) -> u64 {
    derive_seed(master, "string/nonce", u64::from(salt as u32))
}

/// Зашифровать строку.
pub fn encrypt(scheme: Scheme, master: u64, salt: i32, plain: &str) -> Blob {
    let key = string_key(master, salt);
    let nonce = string_nonce(master, salt);
    let mut data = plain.as_bytes().to_vec();
    match scheme {
        Scheme::XorSplitmix => xor_splitmix(key, nonce, &mut data),
        Scheme::XteaCtr => xtea_ctr(key, nonce, &mut data),
    }
    Blob {
        cipher: codec::encode(&data),
        tag: mac(key, &data),
    }
}

/// Расшифровать блоб (зеркало того, что делает рантайм-декодер).
pub fn decrypt(scheme: Scheme, master: u64, salt: i32, blob: &Blob) -> Result<String, SchemeError> {
    let key = string_key(master, salt);
    let nonce = string_nonce(master, salt);
    let mut data = codec::decode(&blob.cipher).map_err(|_| SchemeError::CorruptCiphertext)?;
    if mac(key, &data) != blob.tag {
        return Err(SchemeError::CorruptCiphertext);
    }
    match scheme {
        Scheme::XorSplitmix => xor_splitmix(key, nonce, &mut data),
        Scheme::XteaCtr => xtea_ctr(key, nonce, &mut data),
    }
    String::from_utf8(data).map_err(|_| SchemeError::CorruptCiphertext)
}

/// MAC: fnv1a поверх keystream-домена (дёшево, ловит точечные правки).
fn mac(key: u64, data: &[u8]) -> u32 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.iter().chain(key.to_le_bytes().iter()) {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    (hash ^ (hash >> 32)) as u32
}

fn xor_splitmix(key: u64, nonce: u64, data: &mut [u8]) {
    let mut rng = Rng::from_seed(key ^ nonce.rotate_left(17));
    for chunk in data.chunks_mut(8) {
        let stream = rng.next_u64().to_le_bytes();
        for (b, s) in chunk.iter_mut().zip(stream.iter()) {
            *b ^= s;
        }
    }
}

/// XTEA в режиме счётчика: keystream = XTEA(k, counter), XOR - на месте.
fn xtea_ctr(key: u64, nonce: u64, data: &mut [u8]) {
    let k = [
        key as u32,
        (key >> 32) as u32,
        nonce as u32,
        (nonce >> 32) as u32,
    ];
    for (i, chunk) in data.chunks_mut(8).enumerate() {
        let stream = xtea_encrypt_block(k, (i as u64).to_be_bytes());
        for (b, s) in chunk.iter_mut().zip(stream.iter()) {
            *b ^= s;
        }
    }
}

/// Один блок XTEA (32 раунда, delta = 0x9E3779B9).
fn xtea_encrypt_block(k: [u32; 4], block: [u8; 8]) -> [u8; 8] {
    let mut v0 = u32::from_be_bytes([block[0], block[1], block[2], block[3]]);
    let mut v1 = u32::from_be_bytes([block[4], block[5], block[6], block[7]]);
    let mut sum: u32 = 0;
    const DELTA: u32 = 0x9E37_79B9;
    for _ in 0..32 {
        v0 = v0.wrapping_add(
            (((v1 << 4) ^ (v1 >> 5)).wrapping_add(v1)) ^ (sum.wrapping_add(k[(sum & 3) as usize])),
        );
        sum = sum.wrapping_add(DELTA);
        v1 = v1.wrapping_add(
            (((v0 << 4) ^ (v0 >> 5)).wrapping_add(v0))
                ^ (sum.wrapping_add(k[((sum >> 11) & 3) as usize])),
        );
    }
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&v0.to_be_bytes());
    out[4..].copy_from_slice(&v1.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn roundtrip_all_schemes() {
        for scheme in Scheme::all() {
            for text in ["", "a", "Hello, мир!", "x".repeat(33).as_str()] {
                let blob = encrypt(scheme, 42, 7, text);
                let back = decrypt(scheme, 42, 7, &blob).expect("decrypt");
                assert_eq!(back, text, "scheme={:?}", scheme);
            }
        }
    }

    #[test]
    fn different_salt_different_cipher() {
        let a = encrypt(Scheme::XorSplitmix, 1, 1, "same");
        let b = encrypt(Scheme::XorSplitmix, 1, 2, "same");
        assert_ne!(a.cipher, b.cipher);
    }

    #[test]
    fn tampering_is_detected() {
        let mut blob = encrypt(Scheme::XteaCtr, 5, 3, "sensitive");
        let mut bytes = codec::decode(&blob.cipher).expect("b64");
        bytes[0] ^= 0xFF;
        blob.cipher = codec::encode(&bytes);
        assert_eq!(
            decrypt(Scheme::XteaCtr, 5, 3, &blob),
            Err(SchemeError::CorruptCiphertext)
        );
    }

    #[test]
    fn unknown_scheme_rejected() {
        assert!(Scheme::parse("rot13").is_err());
    }
}
