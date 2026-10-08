//! Кодирование шифротекста в вид, допустимый для `CONSTANT_String`.
//!
//! LDC работает только со строками (modified UTF-8), поэтому байты шифротекста
//! кодируем ASCII-алфавитом: без Base64 из JDK и без лишних зависимостей.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Ошибка кодирования.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("некорректный base64")]
    InvalidBase64,
}

/// Base64 (стандартный алфавит, без переносов строк).
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[triple as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn val(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Base64 → байты.
pub fn decode(text: &str) -> Result<Vec<u8>, CodecError> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(CodecError::InvalidBase64);
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        let mut pads = 0;
        for (i, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                if i < 2 {
                    return Err(CodecError::InvalidBase64);
                }
                pads += 1;
                acc <<= 6;
                continue;
            }
            let v = val(c).ok_or(CodecError::InvalidBase64)?;
            acc = (acc << 6) | v;
        }
        out.push((acc >> 16) as u8);
        if pads < 2 {
            out.push((acc >> 8) as u8);
        }
        if pads < 1 {
            out.push(acc as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn roundtrip() {
        for data in [
            &b""[..],
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            &[0u8, 255, 128, 7][..],
        ] {
            let enc = encode(data);
            assert_eq!(decode(&enc).expect("decode"), data, "data={data:?}");
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode("abc").is_err());
        assert!(decode("!!!!").is_err());
    }
}
