//! Схемы разбиения констант и opaque-предикаты.
//!
//! Целые константы вне `iconst`-диапазона заменяются тройками (`x^k`,
//! `a+b`, сдвиг); opaque-предикат — всегда истинное условие с мёртвой
//! ветвью из одного `nop` (валидно, не исполняется, ломает линейный
//! дисассемблирование).

use noxis_classfile::{Arith, BranchCond, Cond, Const, Insn, Prim};
use noxis_core::Rng;

/// Схема разбиения (имена из `ConstantsConfig::schemes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scheme {
    Xor,
    Add,
    Shift,
}

impl Scheme {
    pub(super) fn parse(name: &str) -> Option<Self> {
        match name {
            "xor" => Some(Scheme::Xor),
            "add" => Some(Scheme::Add),
            "shift" => Some(Scheme::Shift),
            _ => None,
        }
    }
}

/// Разбить `int`-константу на вычисляющую её тройку.
pub(super) fn encrypt_int(value: i32, scheme: Scheme, rng: &mut Rng) -> Vec<Insn> {
    let (first, second) = match scheme {
        Scheme::Xor => {
            let mut key = rng.next_u64() as i32;
            if key == 0 {
                key = 0x5BD1_E995u32 as i32;
            }
            (value ^ key, key)
        }
        Scheme::Add => {
            let part = rng.next_u64() as i32;
            (part, value.wrapping_sub(part))
        }
        Scheme::Shift => (value >> 1, value),
    };
    match scheme {
        Scheme::Xor => binary(first, second, Arith::Xor, Prim::Int),
        Scheme::Add => binary(first, second, Arith::Add, Prim::Int),
        Scheme::Shift => shift_form(value),
    }
}

/// Разбить `long`-константу (те же три схемы на `l*`-опкодах).
pub(super) fn encrypt_long(value: i64, scheme: Scheme, rng: &mut Rng) -> Vec<Insn> {
    let (first, second) = match scheme {
        Scheme::Xor => {
            let mut key = rng.next_u64() as i64;
            if key == 0 {
                key = 0x5BD1_E995_7A93_2AA9u64 as i64;
            }
            (value ^ key, key)
        }
        Scheme::Add => {
            let part = rng.next_u64() as i64;
            (part, value.wrapping_sub(part))
        }
        Scheme::Shift => (value >> 1, value),
    };
    match scheme {
        Scheme::Xor => binary_long(first, second, Arith::Xor),
        Scheme::Add => binary_long(first, second, Arith::Add),
        Scheme::Shift => shift_form_long(value),
    }
}

/// `(a, b, op)` как три инструкции.
fn binary(first: i32, second: i32, op: Arith, prim: Prim) -> Vec<Insn> {
    vec![
        Insn::Const(Const::Int(first)),
        Insn::Const(Const::Int(second)),
        Insn::Binary { prim, op },
    ]
}

fn binary_long(first: i64, second: i64, op: Arith) -> Vec<Insn> {
    vec![
        Insn::Const(Const::Long(first)),
        Insn::Const(Const::Long(second)),
        Insn::Binary {
            prim: Prim::Long,
            op,
        },
    ]
}

/// `v = ((v >> 1) << 1) | (v & 1)` — работает для любых, включая отрицательные.
fn shift_form(value: i32) -> Vec<Insn> {
    vec![
        Insn::Const(Const::Int(value >> 1)),
        Insn::Const(Const::Int(1)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Shl,
        },
        Insn::Const(Const::Int(value & 1)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Or,
        },
    ]
}

/// То же для `long` (величина сдвига — всегда `int`!).
fn shift_form_long(value: i64) -> Vec<Insn> {
    vec![
        Insn::Const(Const::Long(value >> 1)),
        Insn::Const(Const::Int(1)),
        Insn::Binary {
            prim: Prim::Long,
            op: Arith::Shl,
        },
        Insn::Const(Const::Long(value & 1)),
        Insn::Binary {
            prim: Prim::Long,
            op: Arith::Or,
        },
    ]
}

/// Opaque-предикат `(C*C + C) & 1 == 0` (истина для любого `C`):
/// `[pred, ifne DEAD, goto MERGE, DEAD: nop, MERGE:]`, дальше — оригинал.
///
/// Метки выдаёт вызывающий код (свободные номера метода).
pub(super) fn opaque_true(dead: u32, merge: u32, rng: &mut Rng) -> Vec<Insn> {
    let constant = rng.next_u64() as i32;
    vec![
        Insn::Const(Const::Int(constant)),
        Insn::Dup,
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Mul,
        },
        Insn::Const(Const::Int(constant)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Add,
        },
        Insn::Const(Const::Int(1)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::And,
        },
        Insn::Branch(BranchCond::Zero(Cond::Ne), dead),
        Insn::Goto(merge),
        Insn::Mark(dead),
        Insn::Nop,
        Insn::Mark(merge),
    ]
}
