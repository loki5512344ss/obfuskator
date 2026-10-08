//! Типы операндов инструкций: константы, примитивы, условия, вызовы.

/// Идентификатор метки внутри одного метода.
pub type Label = u32;

/// Константа для `ldc`/`ldc_w`/`ldc2_w`.
#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    /// Внутреннее имя класса (`java/lang/String`).
    Class(String),
    MethodType(String),
    MethodHandle {
        kind: u8,
        owner: String,
        name: String,
        descriptor: String,
        itf: bool,
    },
    /// `CONSTANT_Dynamic`: имя, дескриптор, индекс bootstrap-метода.
    Dynamic {
        name: String,
        descriptor: String,
        bootstrap: u16,
    },
}

/// Размер операнда/результата арифметики.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    Int,
    Long,
    Float,
    Double,
}

/// Арифметическая/битовая операция; тип берётся из [`Prim`].
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

/// Условие перехода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Вид условного перехода (соответствует семействам `if*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchCond {
    /// `if<cond>` относительно нуля.
    Zero(Cond),
    /// `if_icmp<cond>`.
    Int(Cond),
    /// `if_acmp<eq|ne>`.
    Ref { eq: bool },
    /// `ifnull` / `ifnonnull`.
    Null { is_null: bool },
}

/// Вид вызова.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvokeKind {
    Virtual,
    Special,
    Static,
    Interface,
}

/// Конверсия типов (`i2l`, `f2b`, ... — JVMS §6.5.i2<b|c|s|l|f|d>).
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

/// Тип элемента массива для `newarray`/`*aload`/`*astore`.
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

/// Тип слота локальной переменной для `*load*`/`*store*`.
///
/// Хранится явно: выбор опкода (`iload` vs `aload`) — подсказка верификатору,
/// потерять её при round-trip нельзя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Int,
    Long,
    Float,
    Double,
    Ref,
}
