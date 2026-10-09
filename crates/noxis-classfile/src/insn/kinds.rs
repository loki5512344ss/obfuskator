//! Instruction operand types: constants, primitives, conditions, calls.

/// A label identifier within a single method.
pub type Label = u32;

/// A constant for `ldc`/`ldc_w`/`ldc2_w`.
#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    /// The class's internal name (`java/lang/String`).
    Class(String),
    MethodType(String),
    MethodHandle {
        kind: u8,
        owner: String,
        name: String,
        descriptor: String,
        itf: bool,
    },
    /// `CONSTANT_Dynamic`: name, descriptor, bootstrap-method index.
    Dynamic {
        name: String,
        descriptor: String,
        bootstrap: u16,
    },
}

/// Operand/result size of an arithmetic operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    Int,
    Long,
    Float,
    Double,
}

/// An arithmetic/bitwise operation; the type comes from [`Prim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Ushr,
}

/// A branch condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Kind of conditional branch (matches the `if*` families).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchCond {
    /// `if<cond>` against zero.
    Zero(Cond),
    /// `if_icmp<cond>`.
    Int(Cond),
    /// `if_acmp<eq|ne>`.
    Ref { eq: bool },
    /// `ifnull` / `ifnonnull`.
    Null { is_null: bool },
}

/// Kind of call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvokeKind {
    Virtual,
    Special,
    Static,
    Interface,
}

/// Type conversion (`i2l`, `f2b`, ... - JVMS §6.5.i2<b|c|s|l|f|d>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convert {
    I2L,
    I2F,
    I2D,
    I2B,
    I2C,
    I2S,
    L2I,
    L2F,
    L2D,
    F2I,
    F2L,
    F2D,
    D2I,
    D2L,
    D2F,
}

/// Array element type for `newarray`/`*aload`/`*astore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayKind {
    Boolean,
    Char,
    Float,
    Double,
    Byte,
    Short,
    Int,
    Long,
    Reference,
}

/// Local-variable slot type for `*load*`/`*store*`.
///
/// Stored explicitly: the opcode choice (`iload` vs `aload`) is a hint to the
/// verifier, and losing it on a round-trip is not acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Int,
    Long,
    Float,
    Double,
    Ref,
}
