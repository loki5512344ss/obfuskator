//! KDF декодера: FNV-1a64 поверх `master || domain || salt`.
//!
//! Зеркало `noxis_core::derive_seed` (тот же FNV-1a64, те же домены).
//! Вход собирается `ByteBuffer`, чтобы не материализовывать 26 байт
//! поштучно: `allocate.putLong(master).put(domain).putLong(salt).array()`.
//!
//! Здесь же - строитель тела метода и короткие эмиссии (общие для всех
//! ветвей декодера).

use noxis_classfile::{Arith, BranchCond, Cond, Const, Insn, InvokeKind, Label, Prim, SlotKind};

/// Строитель тела метода: метки, слоты, короткие эмиссии.
pub(super) struct Gen {
    pub insns: Vec<Insn>,
    labels: u32,
    pub slots: u16,
}

impl Gen {
    pub(super) fn new() -> Self {
        Self {
            insns: Vec::new(),
            labels: 0,
            // Слоты 0 (`cipher`) и 1 (`salt`) - параметры метода.
            slots: 2,
        }
    }

    pub(super) fn label(&mut self) -> Label {
        let label = self.labels;
        self.labels += 1;
        label
    }

    /// Занять слоты (`wide` - пара под `long`).
    pub(super) fn alloc(&mut self, wide: bool) -> u16 {
        let slot = self.slots;
        self.slots += if wide { 2 } else { 1 };
        slot
    }

    pub(super) fn emit(&mut self, insn: Insn) {
        self.insns.push(insn);
    }

    pub(super) fn mark(&mut self, label: Label) {
        self.emit(Insn::Mark(label));
    }
}

/// Короткие эмиссии операндов (`writer` выбирает `iconst`/`bipush` сам).
pub(super) fn c_int(value: i32) -> Insn {
    Insn::Const(Const::Int(value))
}

pub(super) fn c_long(value: i64) -> Insn {
    Insn::Const(Const::Long(value))
}

pub(super) fn c_str(value: &str) -> Insn {
    Insn::Const(Const::String(value.to_string()))
}

pub(super) fn load(slot: u16, kind: SlotKind) -> Insn {
    Insn::Load { slot, kind }
}

pub(super) fn store(slot: u16, kind: SlotKind) -> Insn {
    Insn::Store { slot, kind }
}

pub(super) fn call(owner: &str, name: &str, descriptor: &str, kind: InvokeKind) -> Insn {
    Insn::Invoke {
        kind,
        owner: owner.to_string(),
        name: name.to_string(),
        descriptor: descriptor.to_string(),
        itf: false,
    }
}

/// FNV-1a64 init (знаковое представление для `ldc2_w`).
const FNV_INIT: i64 = 0xCBF2_9CE4_8422_2325u64 as i64;
/// FNV-1a64 prime.
const FNV_PRIME: i64 = 0x0000_0100_0000_01B3;

/// Вычислить `derive_seed(master, domain, salt)`; вернуть слот пары.
pub(super) fn emit_kdf(asm: &mut Gen, master: u16, domain: &str) -> u16 {
    let input = asm.alloc(false);
    build_input(asm, master, domain, input);
    let hash = asm.alloc(true);
    let index = asm.alloc(false);
    asm.emit(c_long(FNV_INIT));
    asm.emit(store(hash, SlotKind::Long));
    asm.emit(c_int(0));
    asm.emit(store(index, SlotKind::Int));
    let again = asm.label();
    let done = asm.label();
    asm.mark(again);
    asm.emit(load(index, SlotKind::Int));
    asm.emit(load(input, SlotKind::Ref));
    asm.emit(Insn::ArrayLength);
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Ge), done));
    asm.emit(load(hash, SlotKind::Long));
    asm.emit(load(input, SlotKind::Ref));
    asm.emit(load(index, SlotKind::Int));
    asm.emit(Insn::ArrayLoad(noxis_classfile::ArrayKind::Byte));
    asm.emit(Insn::Convert(noxis_classfile::Convert::I2L));
    asm.emit(c_long(0xFF));
    asm.emit(Insn::Binary {
        prim: Prim::Long,
        op: Arith::And,
    });
    asm.emit(Insn::Binary {
        prim: Prim::Long,
        op: Arith::Xor,
    });
    asm.emit(store(hash, SlotKind::Long));
    asm.emit(load(hash, SlotKind::Long));
    asm.emit(c_long(FNV_PRIME));
    asm.emit(Insn::Binary {
        prim: Prim::Long,
        op: Arith::Mul,
    });
    asm.emit(store(hash, SlotKind::Long));
    asm.emit(Insn::Iinc { index, delta: 1 });
    asm.emit(Insn::Goto(again));
    asm.mark(done);
    let out = asm.alloc(true);
    asm.emit(load(hash, SlotKind::Long));
    asm.emit(store(out, SlotKind::Long));
    out
}

/// Собрать вход `master || domain || salt` в слот `input`.
///
/// Rust считает FNV по little-endian байтам, а `putLong` кладёт big-endian -
/// поэтому оба `long` разворачиваются (`Long.reverseBytes`).
fn build_input(asm: &mut Gen, master: u16, domain: &str, input: u16) {
    asm.emit(c_int(16 + domain.len() as i32));
    asm.emit(call(
        "java/nio/ByteBuffer",
        "allocate",
        "(I)Ljava/nio/ByteBuffer;",
        InvokeKind::Static,
    ));
    asm.emit(Insn::Dup);
    asm.emit(load(master, SlotKind::Long));
    asm.emit(call(
        "java/lang/Long",
        "reverseBytes",
        "(J)J",
        InvokeKind::Static,
    ));
    asm.emit(call(
        "java/nio/ByteBuffer",
        "putLong",
        "(J)Ljava/nio/ByteBuffer;",
        InvokeKind::Virtual,
    ));
    asm.emit(Insn::Dup);
    asm.emit(c_str(domain));
    asm.emit(call(
        "java/lang/String",
        "getBytes",
        "()[B",
        InvokeKind::Virtual,
    ));
    asm.emit(call(
        "java/nio/ByteBuffer",
        "put",
        "([B)Ljava/nio/ByteBuffer;",
        InvokeKind::Virtual,
    ));
    asm.emit(load(1, SlotKind::Int));
    asm.emit(Insn::Convert(noxis_classfile::Convert::I2L));
    asm.emit(call(
        "java/lang/Long",
        "reverseBytes",
        "(J)J",
        InvokeKind::Static,
    ));
    asm.emit(call(
        "java/nio/ByteBuffer",
        "putLong",
        "(J)Ljava/nio/ByteBuffer;",
        InvokeKind::Virtual,
    ));
    asm.emit(call(
        "java/nio/ByteBuffer",
        "array",
        "()[B",
        InvokeKind::Virtual,
    ));
    asm.emit(store(input, SlotKind::Ref));
}
