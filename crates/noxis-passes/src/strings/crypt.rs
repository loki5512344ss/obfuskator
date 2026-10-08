//! Шифрование строк: чанки, salt под схему, последовательность вызова.
//!
//! Один `Const::String` превращается в `ldc cipher; ldc salt; invokestatic
//! d` на чанк; чанки склеиваются `String.concat` (без `StringBuilder` —
//! меньше инструкций и нет вызовов конструктора).

use noxis_classfile::{Const, Insn, InvokeKind};
use noxis_runtime::{DECODER_DESC, DECODER_METHOD};

/// Разбить строку на чанки по границе символов (1–3 штуки).
pub(super) fn split_chunks(text: &str, split_above: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= split_above.max(1) {
        return vec![text.to_string()];
    }
    let count = chars.len().div_ceil(split_above);
    let count = count.clamp(1, 3);
    let per = chars.len().div_ceil(count);
    chars
        .chunks(per)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Salt, попадающий в нужную схему: `salt % total == index`.
///
/// Salt всегда неотрицательный (Java `%` сохраняет знак — отрицательный salt
/// выбрал бы не ту ветку декодера). `index` обязан быть меньше `total`.
pub(super) fn salt_for_scheme(raw: u64, index: usize, total: usize) -> i32 {
    let total = total.max(1) as u64;
    let index = (index as u64) % total;
    let rounds = (i32::MAX as u64 - index) / total + 1;
    (index + (raw >> 33) % rounds * total) as i32
}

/// Вызовы декодера на чанки + склейка (`[(cipher, salt)]`).
pub(super) fn decode_sequence(owner: &str, parts: &[(String, i32)]) -> Vec<Insn> {
    let mut out = Vec::with_capacity(parts.len() * 4);
    for (position, (cipher, salt)) in parts.iter().enumerate() {
        out.push(Insn::Const(Const::String(cipher.clone())));
        out.push(Insn::Const(Const::Int(*salt)));
        out.push(Insn::Invoke {
            kind: InvokeKind::Static,
            owner: owner.to_string(),
            name: DECODER_METHOD.to_string(),
            descriptor: DECODER_DESC.to_string(),
            itf: false,
        });
        if position > 0 {
            out.push(Insn::Invoke {
                kind: InvokeKind::Virtual,
                owner: "java/lang/String".to_string(),
                name: "concat".to_string(),
                descriptor: "(Ljava/lang/String;)Ljava/lang/String;".to_string(),
                itf: false,
            });
        }
    }
    out
}
