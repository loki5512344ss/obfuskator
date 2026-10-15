//! Condensed emission (`CONSTANT_Dynamic`): the JVM resolves the string
//! itself through the decoder's bootstrap method instead of us calling `d`.
//!
//! Why: a concrete-execution deobfuscator (narumii/SSVM, todo.md 2026-10-14)
//! follows `invokestatic d` and recovers the plaintext, but made **0
//! changes** on a condy-only class - it does not resolve `CONSTANT_Dynamic`
//! at all. The plaintext then lives only inside JVM constant caching.

use noxis_classfile::{Attribute, BootstrapMethod, ClassFile, Const, Insn};
use noxis_runtime::{BOOTSTRAP_DESC, BOOTSTRAP_METHOD};

/// `REF_invokeStatic` (JVMS §5.4.3.5).
const INVOKE_STATIC: u8 = 6;

/// Class-file version introducing `CONSTANT_Dynamic` (Java 11).
pub(super) const CONDY_MIN_MAJOR: u16 = 55;

/// The class supports condy constants at all.
pub(super) fn supported(class: &ClassFile) -> bool {
    class.version.major >= CONDY_MIN_MAJOR
}

/// Where new bootstrap entries would start: right after the existing ones
/// (Kotlin lambdas etc. already own the front of the attribute).
pub(super) fn base_index(class: &ClassFile) -> u16 {
    class
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            Attribute::BootstrapMethods(list) => Some(list.len() as u16),
            _ => None,
        })
        .unwrap_or(0)
}

/// One bootstrap entry per chunk: the decoder's `b` with `cipher` + `salt`
/// as static arguments. The BSM prefix (`lookup`, `name`, `type`) is filled
/// in by the JVM (JVMS §5.4.3.5).
pub(super) fn entry(owner: &str, cipher: &str, salt: i32) -> BootstrapMethod {
    BootstrapMethod {
        kind: INVOKE_STATIC,
        owner: owner.to_string(),
        name: BOOTSTRAP_METHOD.to_string(),
        descriptor: BOOTSTRAP_DESC.to_string(),
        itf: false,
        args: vec![Const::String(cipher.to_string()), Const::Int(salt)],
    }
}

/// Condensed replacements for one string's chunks: a `ldc` per chunk
/// (bootstrap indices `base..base+n`) joined by `String.concat`.
pub(super) fn sequence(base: u16, chunks: usize) -> Vec<Insn> {
    let mut out = Vec::with_capacity(chunks * 2);
    for position in 0..chunks {
        out.push(Insn::Const(Const::Dynamic {
            name: "_".to_string(),
            descriptor: "Ljava/lang/String;".to_string(),
            bootstrap: base + position as u16,
        }));
        if position > 0 {
            out.push(concat());
        }
    }
    out
}

/// `String.concat` - the same chunk join the direct path uses.
fn concat() -> Insn {
    Insn::Invoke {
        kind: noxis_classfile::InvokeKind::Virtual,
        owner: "java/lang/String".to_string(),
        name: "concat".to_string(),
        descriptor: "(Ljava/lang/String;)Ljava/lang/String;".to_string(),
        itf: false,
    }
}
