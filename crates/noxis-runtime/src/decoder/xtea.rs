//! Ветвь `xtea-ctr`: XTEA в режиме счётчика, побайтовый XOR на месте.
//!
//! Зеркало `scheme::xtea_ctr` побитово: ключ — четыре `u32` из key/nonce,
//! счётчик — номер чанка, все правые сдвиги — логические (`u32` в Rust).

use noxis_classfile::{Arith, ArrayKind, BranchCond, Cond, Convert, Insn, Prim, SlotKind};

use super::kdf::{Gen, c_int, load, store};

/// XTEA delta — знаковое представление для `ldc_w`.
const DELTA: i32 = 0x9E37_79B9u32 as i32;

/// Расшифровать `data` на месте; `len` — длина массива.
pub(super) fn emit_xtea(asm: &mut Gen, key: u16, nonce: u16, data: u16, len: u16) {
    let slots = Slots::alloc(asm);
    emit_key_array(asm, key, nonce, &slots);
    asm.emit(c_int(0));
    asm.emit(store(slots.chunk, SlotKind::Int));
    let outer = asm.label();
    let done = asm.label();
    asm.mark(outer);
    asm.emit(load(slots.chunk, SlotKind::Int));
    asm.emit(load(len, SlotKind::Int));
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Ge), done));
    emit_counter(asm, &slots);
    emit_rounds(asm, &slots);
    emit_xor_bytes(asm, data, len, &slots);
    asm.emit(load(slots.chunk, SlotKind::Int));
    asm.emit(c_int(8));
    asm.emit(int_op(Arith::Add));
    asm.emit(store(slots.chunk, SlotKind::Int));
    asm.emit(Insn::Goto(outer));
    asm.mark(done);
}

/// Слоты ветви.
struct Slots {
    keys: u16,
    chunk: u16,
    value0: u16,
    value1: u16,
    sum: u16,
    round: u16,
    counter: u16,
    tmp_a: u16,
    tmp_b: u16,
    src: u16,
    shift: u16,
    sub: u16,
    index: u16,
}

impl Slots {
    fn alloc(asm: &mut Gen) -> Self {
        Self {
            keys: asm.alloc(false),
            chunk: asm.alloc(false),
            value0: asm.alloc(false),
            value1: asm.alloc(false),
            sum: asm.alloc(false),
            round: asm.alloc(false),
            counter: asm.alloc(false),
            tmp_a: asm.alloc(false),
            tmp_b: asm.alloc(false),
            src: asm.alloc(false),
            shift: asm.alloc(false),
            sub: asm.alloc(false),
            index: asm.alloc(false),
        }
    }
}

/// `k = [key.lo, key.hi, nonce.lo, nonce.hi]` как `int[4]`.
fn emit_key_array(asm: &mut Gen, key: u16, nonce: u16, slots: &Slots) {
    asm.emit(c_int(4));
    asm.emit(Insn::NewArray(ArrayKind::Int));
    asm.emit(store(slots.keys, SlotKind::Ref));
    for (position, (source, high)) in [(key, false), (key, true), (nonce, false), (nonce, true)]
        .into_iter()
        .enumerate()
    {
        asm.emit(load(slots.keys, SlotKind::Ref));
        asm.emit(c_int(position as i32));
        asm.emit(load(source, SlotKind::Long));
        if high {
            asm.emit(c_int(32));
            asm.emit(long_shift(Arith::Ushr));
            asm.emit(Insn::Convert(Convert::L2I));
        } else {
            asm.emit(Insn::Convert(Convert::L2I));
        }
        asm.emit(Insn::ArrayStore(ArrayKind::Int));
    }
}

/// Счётчик чанка: `v0 = 0`, `v1 = chunk`, `sum = round = 0`.
fn emit_counter(asm: &mut Gen, slots: &Slots) {
    asm.emit(load(slots.chunk, SlotKind::Int));
    asm.emit(c_int(3));
    asm.emit(int_op(Arith::Ushr));
    asm.emit(store(slots.counter, SlotKind::Int));
    asm.emit(c_int(0));
    asm.emit(store(slots.value0, SlotKind::Int));
    asm.emit(load(slots.counter, SlotKind::Int));
    asm.emit(store(slots.value1, SlotKind::Int));
    asm.emit(c_int(0));
    asm.emit(store(slots.sum, SlotKind::Int));
    asm.emit(c_int(0));
    asm.emit(store(slots.round, SlotKind::Int));
}

/// 32 раунда XTEA над `(v0, v1, sum)`.
fn emit_rounds(asm: &mut Gen, slots: &Slots) {
    let again = asm.label();
    let done = asm.label();
    asm.mark(again);
    asm.emit(load(slots.round, SlotKind::Int));
    asm.emit(c_int(32));
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Ge), done));
    mixing(asm, slots.value0, slots.value1, 0, slots);
    asm.emit(load(slots.sum, SlotKind::Int));
    asm.emit(c_int(DELTA));
    asm.emit(int_op(Arith::Add));
    asm.emit(store(slots.sum, SlotKind::Int));
    mixing(asm, slots.value1, slots.value0, 11, slots);
    asm.emit(Insn::Iinc {
        index: slots.round,
        delta: 1,
    });
    asm.emit(Insn::Goto(again));
    asm.mark(done);
}

/// Половина раунда: `dst += (((src<<4)^(src>>>5)) + src) ^ (sum + k[(sum>>>s)&3])`.
fn mixing(asm: &mut Gen, dst: u16, src: u16, key_shift: i32, slots: &Slots) {
    asm.emit(load(src, SlotKind::Int));
    asm.emit(c_int(4));
    asm.emit(int_op(Arith::Shl));
    asm.emit(store(slots.tmp_a, SlotKind::Int));
    asm.emit(load(src, SlotKind::Int));
    asm.emit(c_int(5));
    asm.emit(int_op(Arith::Ushr));
    asm.emit(store(slots.tmp_b, SlotKind::Int));
    asm.emit(load(slots.tmp_a, SlotKind::Int));
    asm.emit(load(slots.tmp_b, SlotKind::Int));
    asm.emit(int_op(Arith::Xor));
    asm.emit(load(src, SlotKind::Int));
    asm.emit(int_op(Arith::Add));
    asm.emit(store(slots.tmp_a, SlotKind::Int));
    asm.emit(load(slots.sum, SlotKind::Int));
    asm.emit(load(slots.keys, SlotKind::Ref));
    asm.emit(load(slots.sum, SlotKind::Int));
    // Первая половина: `sum & 3` без сдвига; вторая: `(sum >>> s) & 3`.
    if key_shift != 0 {
        asm.emit(c_int(key_shift));
        asm.emit(int_op(Arith::Ushr));
    }
    asm.emit(c_int(3));
    asm.emit(int_op(Arith::And));
    asm.emit(Insn::ArrayLoad(ArrayKind::Int));
    asm.emit(int_op(Arith::Add));
    asm.emit(store(slots.tmp_b, SlotKind::Int));
    asm.emit(load(slots.tmp_a, SlotKind::Int));
    asm.emit(load(slots.tmp_b, SlotKind::Int));
    asm.emit(int_op(Arith::Xor));
    asm.emit(load(dst, SlotKind::Int));
    asm.emit(int_op(Arith::Add));
    asm.emit(store(dst, SlotKind::Int));
}

/// XOR 8 байт чанка со стримом `(v0, v1)`; хвост короче 8 — частично.
fn emit_xor_bytes(asm: &mut Gen, data: u16, len: u16, slots: &Slots) {
    asm.emit(c_int(0));
    asm.emit(store(slots.index, SlotKind::Int));
    let again = asm.label();
    let done = asm.label();
    let use_v0 = asm.label();
    let have_src = asm.label();
    asm.mark(again);
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(c_int(8));
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Ge), done));
    asm.emit(load(len, SlotKind::Int));
    asm.emit(load(slots.chunk, SlotKind::Int));
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(int_op(Arith::Add));
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Le), done));
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(c_int(4));
    asm.emit(int_op(Arith::And));
    asm.emit(Insn::Branch(BranchCond::Zero(Cond::Eq), use_v0));
    asm.emit(load(slots.value1, SlotKind::Int));
    asm.emit(Insn::Goto(have_src));
    asm.mark(use_v0);
    asm.emit(load(slots.value0, SlotKind::Int));
    asm.mark(have_src);
    asm.emit(store(slots.src, SlotKind::Int));
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(c_int(3));
    asm.emit(int_op(Arith::And));
    asm.emit(store(slots.tmp_a, SlotKind::Int));
    asm.emit(c_int(3));
    asm.emit(load(slots.tmp_a, SlotKind::Int));
    asm.emit(int_op(Arith::Sub));
    asm.emit(c_int(3));
    asm.emit(int_op(Arith::Shl));
    asm.emit(store(slots.shift, SlotKind::Int));
    asm.emit(load(slots.src, SlotKind::Int));
    asm.emit(load(slots.shift, SlotKind::Int));
    asm.emit(int_op(Arith::Ushr));
    asm.emit(c_int(0xFF));
    asm.emit(int_op(Arith::And));
    asm.emit(store(slots.sub, SlotKind::Int));
    asm.emit(load(data, SlotKind::Ref));
    asm.emit(load(slots.chunk, SlotKind::Int));
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(int_op(Arith::Add));
    asm.emit(Insn::Dup2);
    asm.emit(Insn::ArrayLoad(ArrayKind::Byte));
    asm.emit(load(slots.sub, SlotKind::Int));
    asm.emit(int_op(Arith::Xor));
    asm.emit(Insn::ArrayStore(ArrayKind::Byte));
    asm.emit(Insn::Iinc {
        index: slots.index,
        delta: 1,
    });
    asm.emit(Insn::Goto(again));
    asm.mark(done);
}

/// Сдвиг `long` на `int` (`lshl`/`lushr`).
fn long_shift(op: Arith) -> Insn {
    Insn::Binary {
        prim: Prim::Long,
        op,
    }
}

/// `iadd/isub/...` для `int`.
fn int_op(op: Arith) -> Insn {
    Insn::Binary {
        prim: Prim::Int,
        op,
    }
}
