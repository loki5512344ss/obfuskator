//! Constant-splitting schemes and opaque predicates.
//!
//! Integer constants outside the `iconst` range are replaced by triples
//! (`x^k`, `a+b`, shift) or the `mba` form below; an opaque predicate is an
//! always-true condition with a one-`nop` dead branch (valid, never
//! executed, breaks linear disassembly).
//!
//! None of this survives a real constant-propagation pass (every operand is
//! a literal, so a general evaluator folds the whole expression regardless
//! of depth) - that is a known, documented limit, not a claim otherwise.
//! `mba` raises the bar against a *pattern* matcher instead: `xor`/`add`
//! hardcode one opcode triple as "this is a split constant", `mba` needs the
//! `(a&b)+(a|b) = a+b` identity specifically.

use noxis_classfile::{Arith, BranchCond, Cond, Const, Insn, Prim};
use noxis_core::Rng;

/// Splitting scheme (names from `ConstantsConfig::schemes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scheme {
    Xor,
    Add,
    Shift,
    Mba,
}

impl Scheme {
    pub(super) fn parse(name: &str) -> Option<Self> {
        match name {
            "xor" => Some(Scheme::Xor),
            "add" => Some(Scheme::Add),
            "shift" => Some(Scheme::Shift),
            "mba" => Some(Scheme::Mba),
            _ => None,
        }
    }
}

/// Split an `int` constant into a triple that computes it.
pub(super) fn encrypt_int(value: i32, scheme: Scheme, rng: &mut Rng) -> Vec<Insn> {
    let (first, second) = match scheme {
        Scheme::Xor => {
            let mut key = rng.next_u64() as i32;
            if key == 0 {
                key = 0x5BD1_E995u32 as i32;
            }
            (value ^ key, key)
        }
        Scheme::Add | Scheme::Mba => {
            let part = rng.next_u64() as i32;
            (part, value.wrapping_sub(part))
        }
        Scheme::Shift => (value >> 1, value),
    };
    match scheme {
        Scheme::Xor => binary(first, second, Arith::Xor, Prim::Int),
        Scheme::Add => binary(first, second, Arith::Add, Prim::Int),
        Scheme::Shift => shift_form(value),
        Scheme::Mba => mba_form(first, second),
    }
}

/// Split a `long` constant (the same three schemes on `l*` opcodes).
pub(super) fn encrypt_long(value: i64, scheme: Scheme, rng: &mut Rng) -> Vec<Insn> {
    let (first, second) = match scheme {
        Scheme::Xor => {
            let mut key = rng.next_u64() as i64;
            if key == 0 {
                key = 0x5BD1_E995_7A93_2AA9u64 as i64;
            }
            (value ^ key, key)
        }
        Scheme::Add | Scheme::Mba => {
            let part = rng.next_u64() as i64;
            (part, value.wrapping_sub(part))
        }
        Scheme::Shift => (value >> 1, value),
    };
    match scheme {
        Scheme::Xor => binary_long(first, second, Arith::Xor),
        Scheme::Add => binary_long(first, second, Arith::Add),
        Scheme::Shift => shift_form_long(value),
        Scheme::Mba => mba_form_long(first, second),
    }
}

/// `(a, b, op)` as three instructions.
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

/// MBA identity: `a+b == (a&b) + (a|b)` (exact, bit-exact under wrapping).
/// `a`/`b` are pushed twice (no stack juggling needed) so each sub-expression
/// stays a plain, correct `and`/`or` - the shape, not a single opcode, is
/// what a matcher has to recognize.
fn mba_form(a: i32, b: i32) -> Vec<Insn> {
    vec![
        Insn::Const(Const::Int(a)),
        Insn::Const(Const::Int(b)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::And,
        },
        Insn::Const(Const::Int(a)),
        Insn::Const(Const::Int(b)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Or,
        },
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Add,
        },
    ]
}

/// The same for `long`.
fn mba_form_long(a: i64, b: i64) -> Vec<Insn> {
    vec![
        Insn::Const(Const::Long(a)),
        Insn::Const(Const::Long(b)),
        Insn::Binary {
            prim: Prim::Long,
            op: Arith::And,
        },
        Insn::Const(Const::Long(a)),
        Insn::Const(Const::Long(b)),
        Insn::Binary {
            prim: Prim::Long,
            op: Arith::Or,
        },
        Insn::Binary {
            prim: Prim::Long,
            op: Arith::Add,
        },
    ]
}

/// `v = ((v >> 1) << 1) | (v & 1)` - works for any value, including negatives.
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

/// The same for `long` (the shift amount is always an `int`!).
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

/// Opaque predicate `(C*C + C) & 1 == 0` (true for any `C`).
///
/// Stack-neutral and leaves control flow unchanged: the computed value is
/// consumed by a branch that never fires; the dead `DEAD: nop` branch only
/// bloats linear disassembly.
///
/// Pattern: `[pred..., ifne DEAD, goto MERGE, DEAD: nop, MERGE:]`.
/// Labels come from the calling code (free method numbers).
pub(super) fn opaque_true(dead: u32, merge: u32, rng: &mut Rng) -> Vec<Insn> {
    let constant = rng.next_u64() as i32;
    vec![
        // P = ((C*C) + C) & 1 - even, so always 0.
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
        // P != 0 never holds - the branch into DEAD is not taken, value popped.
        Insn::Branch(BranchCond::Zero(Cond::Ne), dead),
        Insn::Goto(merge),
        Insn::Mark(dead),
        Insn::Nop,
        Insn::Mark(merge),
    ]
}
