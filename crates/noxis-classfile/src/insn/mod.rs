//! IR instructions: no offsets, branches address a [`Label`].
//!
//! This is fundamental: any pass that changes instruction order/length need not
//! recompute jump offsets — the writer does that once at the end of the pipeline.

mod kinds;

pub use kinds::{
    Arith, ArrayKind, BranchCond, Cond, Const, Convert, InvokeKind, Label, Prim, SlotKind,
};

/// Mnemonic of a single instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum Insn {
    /// Label marker. Branches point at a [`Label`]; the writer computes offsets.
    Mark(Label),
    Nop,
    AconstNull,
    Const(Const),
    /// Load a local variable (`iload`, `aload`, ...).
    Load {
        slot: u16,
        kind: SlotKind,
    },
    /// Store a local variable.
    Store {
        slot: u16,
        kind: SlotKind,
    },
    /// `iinc` — increase a local variable by a constant.
    Iinc {
        index: u16,
        delta: i32,
    },
    /// Load an array element.
    ArrayLoad(ArrayKind),
    /// Store an array element.
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
    /// `iadd`, `lmul`, `lneg`, ... — type + operation.
    Binary {
        prim: Prim,
        op: Arith,
    },
    /// Comparison of longs (`lcmp`) and floats (`fcmpl`/`fcmpg`).
    Cmp {
        prim: Prim,
        nan_great: bool,
    },
    /// Type conversion (`i2l`, `d2f`, ...).
    Convert(Convert),
    Branch(BranchCond, Label),
    Goto(Label),
    /// `jsr` — subroutine call (only version ≤ 50 classes).
    Jsr(Label),
    /// `ret` — return from a subroutine (`jsr`/`ret` come in pairs).
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
    /// `return`/`ireturn`/...; `None` — void return.
    Return(Option<Prim>),
    /// `areturn` — return a reference ([`Prim`] has no reference type).
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
    /// `multianewarray` — array of any dimension; `dims` — how many dimensions from the stack.
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
    /// Labels the instruction references (for building the CFG).
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

    /// An encoded jump target (for inserting opaque predicates and rebuilding the CFG).
    pub fn branch_target_mut(&mut self) -> Option<&mut Label> {
        match self {
            Insn::Branch(_, label) | Insn::Goto(label) => Some(label),
            _ => None,
        }
    }
}
