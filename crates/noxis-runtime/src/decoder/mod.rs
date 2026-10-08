//! Рантайм-декодер: ABI, который получает байткод после подстановки строк.
//!
//! ABI v1 (стабильный, его же нужно повторить на Java side):
//!
//! ```text
//! static String d(String cipher, int salt)
//! ```
//!
//! - `cipher` — base64 шифротекста (схема выбирается по salt: `scheme = all[salt % n]`);
//! - `salt` — индекс строки, по нему детерминированно выводятся key и nonce;
//! - мастер-секрет **не передаётся на вызове** и не лежит в одной константе:
//!   он собирается из двух частей (`k1 ^ k2`) прямо в декодере.
//!
//! Класс версии 49: циклы верифицируются выводом типов без `StackMapTable`
//! (проверено загрузкой v49 на Java 21), кадры не генерируются.

mod kdf;
mod xor;
mod xtea;

use noxis_classfile::{
    Arith, BranchCond, ClassBuilder, ClassFile, Code, Cond, Insn, InvokeKind, Prim, SlotKind,
    Version,
};

use crate::scheme::Scheme;

use kdf::{Gen, c_int, c_long, c_str, call, load, store};

/// Имя метода декодера.
pub const DECODER_METHOD: &str = "d";
/// Дескриптор метода декодера.
pub const DECODER_DESC: &str = "(Ljava/lang/String;I)Ljava/lang/String;";

/// Ошибка генерации рантайма.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("не реализовано: {0}")]
    Unimplemented(&'static str),
    #[error("нет активных схем шифрования")]
    NoSchemes,
}

/// Сгенерировать класс-декодер (мастер-секрет + расшифровка всех схем).
pub fn decoder_class(
    internal_name: &str,
    master: u64,
    schemes: &[&str],
) -> Result<ClassFile, RuntimeError> {
    if schemes.is_empty() {
        return Err(RuntimeError::NoSchemes);
    }
    let mut parsed = Vec::with_capacity(schemes.len());
    for name in schemes {
        parsed.push(Scheme::parse(name).map_err(|_| RuntimeError::NoSchemes)?);
    }
    let mut asm = Gen::new();
    let master_slot = emit_master(&mut asm, master);
    let key = kdf::emit_kdf(&mut asm, master_slot, "string/key");
    let nonce = kdf::emit_kdf(&mut asm, master_slot, "string/nonce");
    let data = emit_base64(&mut asm);
    let len = emit_len(&mut asm, data);
    emit_dispatch(&mut asm, &parsed, key, nonce, data, len);
    emit_epilogue(&mut asm, data);
    let code = Code {
        // Точный пик — 10 слотов (замер 08.10.2026); запас без вреда:
        // лимит обязан лишь покрывать фактическую глубину.
        max_stack: 12,
        max_locals: asm.slots,
        insns: asm.insns,
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    Ok(ClassBuilder::new(internal_name.to_string(), None)
        .version(Version {
            minor: 0,
            major: 49,
        })
        .method_with_code(0x0009, DECODER_METHOD, DECODER_DESC, code)
        .build())
}

/// Имя класса-декодера для прогона: детерминированно от сида, но выглядит
/// как обычный обфусцированный класс.
pub fn decoder_owner(seed_name: &str) -> String {
    seed_name.to_string()
}

/// Пролог: `master = k1 ^ k2`; возвращает слот пары. Ни одна часть не равна
/// мастер-секрету (вторая — `master ^ k1`, первая — потоковая).
fn emit_master(asm: &mut Gen, master: u64) -> u16 {
    use noxis_core::Rng;

    let mut rng = Rng::from_seed(master ^ 0x9E37_79B9_7F4A_7C15);
    let first = rng.next_u64() as i64;
    let second = (master ^ first as u64) as i64;
    let slot = asm.alloc(true);
    asm.emit(c_long(first));
    asm.emit(c_long(second));
    asm.emit(Insn::Binary {
        prim: Prim::Long,
        op: Arith::Xor,
    });
    asm.emit(store(slot, SlotKind::Long));
    slot
}

/// `data = Base64.getDecoder().decode(cipher)`; возвращает слот массива.
fn emit_base64(asm: &mut Gen) -> u16 {
    let slot = asm.alloc(false);
    asm.emit(call(
        "java/util/Base64",
        "getDecoder",
        "()Ljava/util/Base64$Decoder;",
        InvokeKind::Static,
    ));
    asm.emit(load(0, SlotKind::Ref));
    asm.emit(call(
        "java/util/Base64$Decoder",
        "decode",
        "(Ljava/lang/String;)[B",
        InvokeKind::Virtual,
    ));
    asm.emit(store(slot, SlotKind::Ref));
    slot
}

/// `len = data.length`; возвращает слот.
fn emit_len(asm: &mut Gen, data: u16) -> u16 {
    let slot = asm.alloc(false);
    asm.emit(load(data, SlotKind::Ref));
    asm.emit(Insn::ArrayLength);
    asm.emit(store(slot, SlotKind::Int));
    slot
}

/// Диспетчер схем: `salt % n` лестницей `if_icmpne` (n == 1 — без диспетчера).
fn emit_dispatch(asm: &mut Gen, schemes: &[Scheme], key: u16, nonce: u16, data: u16, len: u16) {
    if schemes.len() == 1 {
        dispatch_branch(asm, schemes[0], key, nonce, data, len);
        return;
    }
    let done = asm.label();
    for (index, scheme) in schemes.iter().enumerate() {
        if index + 1 == schemes.len() {
            break;
        }
        let next = asm.label();
        asm.emit(load(1, SlotKind::Int));
        asm.emit(c_int(schemes.len() as i32));
        asm.emit(Insn::Binary {
            prim: Prim::Int,
            op: Arith::Rem,
        });
        asm.emit(c_int(index as i32));
        asm.emit(Insn::Branch(BranchCond::Int(Cond::Ne), next));
        dispatch_branch(asm, *scheme, key, nonce, data, len);
        asm.emit(Insn::Goto(done));
        asm.mark(next);
    }
    dispatch_branch(asm, schemes[schemes.len() - 1], key, nonce, data, len);
    asm.mark(done);
}

fn dispatch_branch(asm: &mut Gen, scheme: Scheme, key: u16, nonce: u16, data: u16, len: u16) {
    match scheme {
        Scheme::XorSplitmix => xor::emit_xor(asm, key, nonce, data, len),
        Scheme::XteaCtr => xtea::emit_xtea(asm, key, nonce, data, len),
    }
}

/// Эпилог: `return new String(data, "UTF-8")`.
fn emit_epilogue(asm: &mut Gen, data: u16) {
    asm.emit(Insn::New("java/lang/String".to_string()));
    asm.emit(Insn::Dup);
    asm.emit(load(data, SlotKind::Ref));
    asm.emit(c_str("UTF-8"));
    asm.emit(call(
        "java/lang/String",
        "<init>",
        "([BLjava/lang/String;)V",
        InvokeKind::Special,
    ));
    asm.emit(Insn::AReturn);
}
