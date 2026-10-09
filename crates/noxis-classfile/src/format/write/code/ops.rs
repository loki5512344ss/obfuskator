//! Operands: constants, arithmetic, arrays, calls.

use ristretto_classfile::attributes::{ArrayType, Instruction as RtInsn};

use super::super::pool::Pool;
use crate::error::ClassfileError;
use crate::insn::{Arith, ArrayKind, Const, Convert, InvokeKind, Prim};

/// A constant: short forms (`iconst`, `bipush`) + `ldc_w`/`ldc2_w` through the pool.
pub(super) fn int_const(value: &Const, pool: &mut Pool) -> Result<RtInsn, ClassfileError> {
    Ok(match value {
        Const::Int(v) => match *v {
            -1 => RtInsn::Iconst_m1,
            0 => RtInsn::Iconst_0,
            1 => RtInsn::Iconst_1,
            2 => RtInsn::Iconst_2,
            3 => RtInsn::Iconst_3,
            4 => RtInsn::Iconst_4,
            5 => RtInsn::Iconst_5,
            v if i8::try_from(v).is_ok() => RtInsn::Bipush(v as i8),
            v if i16::try_from(v).is_ok() => RtInsn::Sipush(v as i16),
            _ => RtInsn::Ldc_w(pool.constant(value)?),
        },
        Const::Long(0) => RtInsn::Lconst_0,
        Const::Long(1) => RtInsn::Lconst_1,
        Const::Long(_) => RtInsn::Ldc2_w(pool.constant(value)?),
        Const::Float(v) if *v == 0.0 => RtInsn::Fconst_0,
        Const::Float(v) if *v == 1.0 => RtInsn::Fconst_1,
        Const::Float(v) if *v == 2.0 => RtInsn::Fconst_2,
        Const::Float(_) => RtInsn::Ldc_w(pool.constant(value)?),
        Const::Double(v) if *v == 0.0 => RtInsn::Dconst_0,
        Const::Double(v) if *v == 1.0 => RtInsn::Dconst_1,
        Const::Double(_) => RtInsn::Ldc2_w(pool.constant(value)?),
        Const::Dynamic {
            name,
            descriptor,
            bootstrap,
        } => RtInsn::Ldc2_w(pool.dynamic(*bootstrap, name, descriptor)?),
        _ => RtInsn::Ldc_w(pool.constant(value)?),
    })
}

pub(super) fn array_load(kind: ArrayKind) -> RtInsn {
    match kind {
        ArrayKind::Boolean | ArrayKind::Byte => RtInsn::Baload,
        ArrayKind::Char => RtInsn::Caload,
        ArrayKind::Short => RtInsn::Saload,
        ArrayKind::Int => RtInsn::Iaload,
        ArrayKind::Long => RtInsn::Laload,
        ArrayKind::Float => RtInsn::Faload,
        ArrayKind::Double => RtInsn::Daload,
        ArrayKind::Reference => RtInsn::Aaload,
    }
}

pub(super) fn array_store(kind: ArrayKind) -> RtInsn {
    match kind {
        ArrayKind::Boolean | ArrayKind::Byte => RtInsn::Bastore,
        ArrayKind::Char => RtInsn::Castore,
        ArrayKind::Short => RtInsn::Sastore,
        ArrayKind::Int => RtInsn::Iastore,
        ArrayKind::Long => RtInsn::Lastore,
        ArrayKind::Float => RtInsn::Fastore,
        ArrayKind::Double => RtInsn::Dastore,
        ArrayKind::Reference => RtInsn::Aastore,
    }
}

pub(super) fn array_type(kind: ArrayKind) -> Result<ArrayType, ClassfileError> {
    Ok(match kind {
        ArrayKind::Boolean => ArrayType::Boolean,
        ArrayKind::Char => ArrayType::Char,
        ArrayKind::Float => ArrayType::Float,
        ArrayKind::Double => ArrayType::Double,
        ArrayKind::Byte => ArrayType::Byte,
        ArrayKind::Short => ArrayType::Short,
        ArrayKind::Int => ArrayType::Int,
        ArrayKind::Long => ArrayType::Long,
        ArrayKind::Reference => {
            return Err(ClassfileError::Write(
                "newarray не создаёт ссылочные массивы (нужен anewarray)".into(),
            ));
        }
    })
}

pub(super) fn binary(prim: Prim, op: Arith) -> Result<RtInsn, ClassfileError> {
    use Arith::{Add, And, Div, Mul, Neg, Or, Rem, Shl, Shr, Sub, Ushr, Xor};
    Ok(match (prim, op) {
        (Prim::Int, Add) => RtInsn::Iadd,
        (Prim::Int, Sub) => RtInsn::Isub,
        (Prim::Int, Mul) => RtInsn::Imul,
        (Prim::Int, Div) => RtInsn::Idiv,
        (Prim::Int, Rem) => RtInsn::Irem,
        (Prim::Int, Neg) => RtInsn::Ineg,
        (Prim::Int, And) => RtInsn::Iand,
        (Prim::Int, Or) => RtInsn::Ior,
        (Prim::Int, Xor) => RtInsn::Ixor,
        (Prim::Int, Shl) => RtInsn::Ishl,
        (Prim::Int, Shr) => RtInsn::Ishr,
        (Prim::Int, Ushr) => RtInsn::Iushr,
        (Prim::Long, Add) => RtInsn::Ladd,
        (Prim::Long, Sub) => RtInsn::Lsub,
        (Prim::Long, Mul) => RtInsn::Lmul,
        (Prim::Long, Div) => RtInsn::Ldiv,
        (Prim::Long, Rem) => RtInsn::Lrem,
        (Prim::Long, Neg) => RtInsn::Lneg,
        (Prim::Long, And) => RtInsn::Land,
        (Prim::Long, Or) => RtInsn::Lor,
        (Prim::Long, Xor) => RtInsn::Lxor,
        (Prim::Long, Shl) => RtInsn::Lshl,
        (Prim::Long, Shr) => RtInsn::Lshr,
        (Prim::Long, Ushr) => RtInsn::Lushr,
        (Prim::Float, Add) => RtInsn::Fadd,
        (Prim::Float, Sub) => RtInsn::Fsub,
        (Prim::Float, Mul) => RtInsn::Fmul,
        (Prim::Float, Div) => RtInsn::Fdiv,
        (Prim::Float, Rem) => RtInsn::Frem,
        (Prim::Float, Neg) => RtInsn::Fneg,
        (Prim::Double, Add) => RtInsn::Dadd,
        (Prim::Double, Sub) => RtInsn::Dsub,
        (Prim::Double, Mul) => RtInsn::Dmul,
        (Prim::Double, Div) => RtInsn::Ddiv,
        (Prim::Double, Rem) => RtInsn::Drem,
        (Prim::Double, Neg) => RtInsn::Dneg,
        (prim, op) => {
            return Err(ClassfileError::Write(format!(
                "недопустимая комбинация {prim:?}/{op:?}"
            )));
        }
    })
}

pub(super) fn cmp(prim: Prim, nan_great: bool) -> Result<RtInsn, ClassfileError> {
    Ok(match (prim, nan_great) {
        (Prim::Long, false) => RtInsn::Lcmp,
        (Prim::Float, false) => RtInsn::Fcmpl,
        (Prim::Float, true) => RtInsn::Fcmpg,
        (Prim::Double, false) => RtInsn::Dcmpl,
        (Prim::Double, true) => RtInsn::Dcmpg,
        (prim, nan_great) => {
            return Err(ClassfileError::Write(format!(
                "недопустимое сравнение {prim:?}/{nan_great}"
            )));
        }
    })
}

pub(super) fn convert_op(op: Convert) -> RtInsn {
    match op {
        Convert::I2L => RtInsn::I2l,
        Convert::I2F => RtInsn::I2f,
        Convert::I2D => RtInsn::I2d,
        Convert::I2B => RtInsn::I2b,
        Convert::I2C => RtInsn::I2c,
        Convert::I2S => RtInsn::I2s,
        Convert::L2I => RtInsn::L2i,
        Convert::L2F => RtInsn::L2f,
        Convert::L2D => RtInsn::L2d,
        Convert::F2I => RtInsn::F2i,
        Convert::F2L => RtInsn::F2l,
        Convert::F2D => RtInsn::F2d,
        Convert::D2I => RtInsn::D2i,
        Convert::D2L => RtInsn::D2l,
        Convert::D2F => RtInsn::D2f,
    }
}

pub(super) fn invoke(
    kind: InvokeKind,
    owner: &str,
    name: &str,
    descriptor: &str,
    pool: &mut Pool,
) -> Result<RtInsn, ClassfileError> {
    Ok(match kind {
        InvokeKind::Virtual => {
            RtInsn::Invokevirtual(pool.method_ref(owner, name, descriptor, kind)?)
        }
        InvokeKind::Special => {
            RtInsn::Invokespecial(pool.method_ref(owner, name, descriptor, kind)?)
        }
        InvokeKind::Static => RtInsn::Invokestatic(pool.method_ref(owner, name, descriptor, kind)?),
        InvokeKind::Interface => {
            let index = pool.method_ref(owner, name, descriptor, kind)?;
            RtInsn::Invokeinterface(index, arg_count(descriptor)?)
        }
    })
}

/// The `count` byte for `invokeinterface`: argument slots + `objectref`.
fn arg_count(descriptor: &str) -> Result<u8, ClassfileError> {
    let params = descriptor
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(params, _)| params)
        .ok_or_else(|| ClassfileError::Write(format!("кривой дескриптор {descriptor}")))?;
    let mut slots = 1u32;
    let mut chars = params.chars();
    while let Some(char) = chars.next() {
        match char {
            'B' | 'C' | 'F' | 'I' | 'S' | 'Z' => slots += 1,
            'J' | 'D' => slots += 2,
            'L' => {
                if !skip_to(chars.by_ref(), ';') {
                    return Err(bad_descriptor(descriptor));
                }
                slots += 1;
            }
            '[' => {
                while chars.as_str().starts_with('[') {
                    chars.next();
                }
                match chars.next() {
                    Some('L') => {
                        if !skip_to(chars.by_ref(), ';') {
                            return Err(bad_descriptor(descriptor));
                        }
                    }
                    Some(_) => {}
                    None => return Err(bad_descriptor(descriptor)),
                }
                slots += 1;
            }
            _ => return Err(bad_descriptor(descriptor)),
        }
    }
    u8::try_from(slots)
        .map_err(|_| ClassfileError::Write(format!("слишком много аргументов в {descriptor}")))
}

/// Skip characters up to the separator (inclusive); `false` - the string ended.
fn skip_to(chars: &mut std::str::Chars<'_>, end: char) -> bool {
    for char in chars {
        if char == end {
            return true;
        }
    }
    false
}

fn bad_descriptor(descriptor: &str) -> ClassfileError {
    ClassfileError::Write(format!("кривой дескриптор {descriptor}"))
}
