//! Ветвь `xor-splitmix`: keystream из xoshiro256++ поверх SplitMix-сида.
//!
//! Зеркало `scheme::xor_splitmix` + `Rng::{from_seed, next_u64}` побитово:
//! сид `key ^ rotl(nonce, 17)`, стрим LE-байтами `next_u64`.

use noxis_classfile::{Arith, ArrayKind, BranchCond, Cond, Convert, Insn, Prim, SlotKind};

use super::kdf::{Gen, c_int, c_long, load, store};

/// Расшифровать `data` на месте; `len` - длина массива.
pub(super) fn emit_xor(asm: &mut Gen, key: u16, nonce: u16, data: u16, len: u16) {
    let liga = Slots::alloc(asm);
    emit_seed(asm, key, nonce, &liga);
    emit_states(asm, &liga);
    emit_loop(asm, data, len, &liga);
}

/// Слоты ветви (выделяются один раз, используются последовательно).
struct Slots {
    seed: u16,
    state: [u16; 4],
    stream: u16,
    index: u16,
    inner: u16,
    byte: u16,
    tmp_a: u16,
    tmp_b: u16,
    tmp_c: u16,
}

impl Slots {
    fn alloc(asm: &mut Gen) -> Self {
        Self {
            seed: asm.alloc(true),
            state: [
                asm.alloc(true),
                asm.alloc(true),
                asm.alloc(true),
                asm.alloc(true),
            ],
            stream: asm.alloc(true),
            index: asm.alloc(false),
            inner: asm.alloc(false),
            byte: asm.alloc(false),
            tmp_a: asm.alloc(true),
            tmp_b: asm.alloc(true),
            tmp_c: asm.alloc(true),
        }
    }
}

/// `seed = key ^ rotl(nonce, 17)`.
fn emit_seed(asm: &mut Gen, key: u16, nonce: u16, slots: &Slots) {
    asm.emit(load(nonce, SlotKind::Long));
    asm.emit(store(slots.tmp_a, SlotKind::Long));
    asm.emit(load(slots.tmp_a, SlotKind::Long));
    asm.emit(c_int(17));
    asm.emit(shift(Arith::Shl));
    asm.emit(store(slots.tmp_b, SlotKind::Long));
    asm.emit(load(slots.tmp_a, SlotKind::Long));
    asm.emit(c_int(47));
    asm.emit(shift(Arith::Ushr));
    asm.emit(load(slots.tmp_b, SlotKind::Long));
    asm.emit(long_op(Arith::Or));
    asm.emit(load(key, SlotKind::Long));
    asm.emit(long_op(Arith::Xor));
    asm.emit(store(slots.seed, SlotKind::Long));
}

/// `s[i] = splitmix(seed)` четыре раза.
fn emit_states(asm: &mut Gen, slots: &Slots) {
    asm.emit(load(slots.seed, SlotKind::Long));
    asm.emit(store(slots.tmp_a, SlotKind::Long));
    for state in slots.state {
        splitmix(asm, slots.tmp_a, state);
    }
}

/// Один `splitmix64`: `sm += PHI`, финализатор - в `out`.
fn splitmix(asm: &mut Gen, sm: u16, out: u16) {
    const PHI: i64 = 0x9E37_79B9_7F4A_7C15u64 as i64;
    const C1: i64 = 0xBF58_476D_1CE4_E5B9u64 as i64;
    const C2: i64 = 0x94D0_49BB_1331_11EBu64 as i64;
    asm.emit(load(sm, SlotKind::Long));
    asm.emit(c_long(PHI));
    asm.emit(long_op(Arith::Add));
    asm.emit(Insn::Dup2);
    asm.emit(store(sm, SlotKind::Long));
    finalize(asm, 30, C1);
    finalize(asm, 27, C2);
    asm.emit(Insn::Dup2);
    asm.emit(c_int(31));
    asm.emit(shift(Arith::Ushr));
    asm.emit(long_op(Arith::Xor));
    asm.emit(store(out, SlotKind::Long));
}

/// `(z ^ (z >>> amount)) * c` для вершины стека (результат остаётся наверху).
fn finalize(asm: &mut Gen, amount: i32, constant: i64) {
    asm.emit(Insn::Dup2);
    asm.emit(c_int(amount));
    asm.emit(shift(Arith::Ushr));
    asm.emit(long_op(Arith::Xor));
    asm.emit(c_long(constant));
    asm.emit(long_op(Arith::Mul));
}

/// Цикл по чанкам: свежий `next_u64` на 8 байт, внутренний - по байтам.
///
/// Без `skip`-джойна: стрим всегда свеж на входе во внутренний цикл.
fn emit_loop(asm: &mut Gen, data: u16, len: u16, slots: &Slots) {
    asm.emit(c_int(0));
    asm.emit(store(slots.index, SlotKind::Int));
    let outer = asm.label();
    let outer_done = asm.label();
    asm.mark(outer);
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(load(len, SlotKind::Int));
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Ge), outer_done));
    emit_next(asm, slots);
    asm.emit(c_int(0));
    asm.emit(store(slots.inner, SlotKind::Int));
    let inner = asm.label();
    let inner_done = asm.label();
    asm.mark(inner);
    asm.emit(load(slots.inner, SlotKind::Int));
    asm.emit(c_int(8));
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Ge), inner_done));
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(load(slots.inner, SlotKind::Int));
    asm.emit(int_op(Arith::Add));
    asm.emit(load(len, SlotKind::Int));
    asm.emit(Insn::Branch(BranchCond::Int(Cond::Ge), inner_done));
    asm.emit(load(slots.stream, SlotKind::Long));
    asm.emit(load(slots.inner, SlotKind::Int));
    asm.emit(c_int(3));
    asm.emit(int_op(Arith::Shl));
    asm.emit(shift(Arith::Ushr));
    asm.emit(Insn::Convert(Convert::L2I));
    asm.emit(c_int(0xFF));
    asm.emit(int_op(Arith::And));
    asm.emit(store(slots.byte, SlotKind::Int));
    asm.emit(load(data, SlotKind::Ref));
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(load(slots.inner, SlotKind::Int));
    asm.emit(int_op(Arith::Add));
    asm.emit(Insn::Dup2);
    asm.emit(Insn::ArrayLoad(ArrayKind::Byte));
    asm.emit(load(slots.byte, SlotKind::Int));
    asm.emit(int_op(Arith::Xor));
    asm.emit(Insn::ArrayStore(ArrayKind::Byte));
    asm.emit(Insn::Iinc {
        index: slots.inner,
        delta: 1,
    });
    asm.emit(Insn::Goto(inner));
    asm.mark(inner_done);
    asm.emit(load(slots.index, SlotKind::Int));
    asm.emit(c_int(8));
    asm.emit(int_op(Arith::Add));
    asm.emit(store(slots.index, SlotKind::Int));
    asm.emit(Insn::Goto(outer));
    asm.mark(outer_done);
}

/// Один `next_u64` xoshiro256++ в `stream` (точное зеркало `Rng`).
fn emit_next(asm: &mut Gen, slots: &Slots) {
    let s = slots.state;
    // result = rotl(s0 + s3, 23) + s0
    asm.emit(load(s[0], SlotKind::Long));
    asm.emit(load(s[3], SlotKind::Long));
    asm.emit(long_op(Arith::Add));
    asm.emit(store(slots.tmp_a, SlotKind::Long));
    asm.emit(load(slots.tmp_a, SlotKind::Long));
    asm.emit(c_int(23));
    asm.emit(shift(Arith::Shl));
    asm.emit(store(slots.tmp_b, SlotKind::Long));
    asm.emit(load(slots.tmp_a, SlotKind::Long));
    asm.emit(c_int(41));
    asm.emit(shift(Arith::Ushr));
    asm.emit(load(slots.tmp_b, SlotKind::Long));
    asm.emit(long_op(Arith::Or));
    asm.emit(load(s[0], SlotKind::Long));
    asm.emit(long_op(Arith::Add));
    asm.emit(store(slots.stream, SlotKind::Long));
    // t = s1 << 17
    asm.emit(load(s[1], SlotKind::Long));
    asm.emit(c_int(17));
    asm.emit(shift(Arith::Shl));
    asm.emit(store(slots.tmp_c, SlotKind::Long));
    // s2 ^= s0; s3 ^= s1; s1 ^= s2; s0 ^= s3 - строго по порядку.
    xor_into(asm, s[2], s[0]);
    xor_into(asm, s[3], s[1]);
    xor_into(asm, s[1], s[2]);
    xor_into(asm, s[0], s[3]);
    // s2 ^= t
    xor_into(asm, s[2], slots.tmp_c);
    // s3 = rotl(s3, 45)
    asm.emit(load(s[3], SlotKind::Long));
    asm.emit(store(slots.tmp_a, SlotKind::Long));
    asm.emit(load(slots.tmp_a, SlotKind::Long));
    asm.emit(c_int(45));
    asm.emit(shift(Arith::Shl));
    asm.emit(store(slots.tmp_b, SlotKind::Long));
    asm.emit(load(slots.tmp_a, SlotKind::Long));
    asm.emit(c_int(19));
    asm.emit(shift(Arith::Ushr));
    asm.emit(load(slots.tmp_b, SlotKind::Long));
    asm.emit(long_op(Arith::Or));
    asm.emit(store(s[3], SlotKind::Long));
}

/// `dst ^= src` для long-слотов.
fn xor_into(asm: &mut Gen, dst: u16, src: u16) {
    asm.emit(load(dst, SlotKind::Long));
    asm.emit(load(src, SlotKind::Long));
    asm.emit(long_op(Arith::Xor));
    asm.emit(store(dst, SlotKind::Long));
}

/// `ladd/lsub/...` для `long` (операнды уже на стеке).
fn long_op(op: Arith) -> Insn {
    Insn::Binary {
        prim: Prim::Long,
        op,
    }
}

/// Сдвиг `long` на `int` (`lshl`/`lushr`, операнды уже на стеке).
fn shift(op: Arith) -> Insn {
    Insn::Binary {
        prim: Prim::Long,
        op,
    }
}

/// `iadd/isub/...` для `int` (операнды уже на стеке).
fn int_op(op: Arith) -> Insn {
    Insn::Binary {
        prim: Prim::Int,
        op,
    }
}
