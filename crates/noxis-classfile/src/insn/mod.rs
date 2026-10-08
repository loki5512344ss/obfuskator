//! Инструкции IR: смещения отсутствуют, ветвления адресуют [`Label`].
//!
//! Это принципиально: любой пасс, меняющий порядок/длину инструкций, не обязан
//! пересчитывать jump-офсеты — это делает writer один раз в конце пайплайна.

mod kinds;

pub use kinds::{
    Arith, ArrayKind, BranchCond, Cond, Const, Convert, InvokeKind, Label, Prim, SlotKind,
};

/// Мнемоника одной инструкции.
#[derive(Debug, Clone, PartialEq)]
pub enum Insn {
    /// Маркер метки. Ветвления указывают на [`Label`], writer вычисляет смещения.
    Mark(Label),
    Nop,
    AconstNull,
    Const(Const),
    /// Загрузка локальной переменной (`iload`, `aload`, ...).
    Load {
        slot: u16,
        kind: SlotKind,
    },
    /// Сохранение локальной переменной.
    Store {
        slot: u16,
        kind: SlotKind,
    },
    /// `iinc` — увеличение локальной переменной на константу.
    Iinc {
        index: u16,
        delta: i32,
    },
    /// Загрузка элемента массива.
    ArrayLoad(ArrayKind),
    /// Сохранение элемента массива.
    ArrayStore(ArrayKind),
    Pop,
    Pop2,
    Dup,
    DupX1,
    DupX2,
    Dup2,
    Dup2X1,
    Dup2X2,
    Swap,
    /// `iadd`, `lmul`, `lneg`, ... — тип + операция.
    Binary {
        prim: Prim,
        op: Arith,
    },
    /// Сравнение длин (`lcmp`) и плавающих (`fcmpl`/`fcmpg`).
    Cmp {
        prim: Prim,
        nan_great: bool,
    },
    /// Конверсия типов (`i2l`, `d2f`, ...).
    Convert(Convert),
    Branch(BranchCond, Label),
    Goto(Label),
    /// `jsr` — вызов подпрограммы (только классы версии ≤ 50).
    Jsr(Label),
    /// `ret` — возврат из подпрограммы (`jsr`/`ret` парны).
    Ret(u16),
    TableSwitch {
        default: Label,
        low: i32,
        high: i32,
        targets: Vec<Label>,
    },
    LookupSwitch {
        default: Label,
        pairs: Vec<(i32, Label)>,
    },
    /// `return`/`ireturn`/...; `None` — пустой возврат.
    Return(Option<Prim>),
    /// `areturn` — возврат ссылки (у [`Prim`] нет ссылочного типа).
    AReturn,
    GetStatic {
        owner: String,
        name: String,
        descriptor: String,
    },
    PutStatic {
        owner: String,
        name: String,
        descriptor: String,
    },
    GetField {
        owner: String,
        name: String,
        descriptor: String,
    },
    PutField {
        owner: String,
        name: String,
        descriptor: String,
    },
    Invoke {
        kind: InvokeKind,
        owner: String,
        name: String,
        descriptor: String,
        itf: bool,
    },
    InvokeDynamic {
        name: String,
        descriptor: String,
        bootstrap: u16,
    },
    New(String),
    NewArray(ArrayKind),
    ANewArray(String),
    /// `multianewarray` — массив любого измерения; `dims` — сколько измерений со стека.
    MultiANewArray {
        descriptor: String,
        dims: u8,
    },
    ArrayLength,
    AThrow,
    CheckCast(String),
    InstanceOf(String),
    MonitorEnter,
    MonitorExit,
}

impl Insn {
    /// Метки, на которые ссылается инструкция (для построения CFG).
    pub fn targets(&self) -> Vec<Label> {
        match self {
            Insn::Branch(_, label) | Insn::Goto(label) | Insn::Jsr(label) => vec![*label],
            Insn::TableSwitch {
                default,
                targets,
                low: _,
                high: _,
            } => {
                let mut all = Vec::with_capacity(targets.len() + 1);
                all.push(*default);
                all.extend(targets.iter().copied());
                all
            }
            Insn::LookupSwitch { default, pairs } => {
                let mut all = Vec::with_capacity(pairs.len() + 1);
                all.push(*default);
                all.extend(pairs.iter().map(|(k, l)| {
                    let _ = k;
                    *l
                }));
                all
            }
            _ => Vec::new(),
        }
    }

    /// Незакодированный переход (для вставки opaque-предикатов и перестройки CFG).
    pub fn branch_target_mut(&mut self) -> Option<&mut Label> {
        match self {
            Insn::Branch(_, label) | Insn::Goto(label) => Some(label),
            _ => None,
        }
    }
}
