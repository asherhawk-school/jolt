//! Typed intermediate representation: fully resolved, ready for bytecode emission.

use crate::ast::BinOp;
use crate::types::Type;

#[derive(Clone, Debug)]
pub struct TE {
    pub kind: TK,
    pub ty: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invoke {
    Static,
    Virtual,
    Special,
    Interface,
}

#[derive(Clone, Debug)]
pub enum TK {
    /// Int/char/boolean constants all live here (booleans as 0/1).
    Int(i32),
    Long(i64),
    Double(f64),
    Str(String),
    Null,
    Local(u16),
    This,
    GetStatic {
        owner: String,
        name: String,
    },
    GetField {
        obj: Box<TE>,
        owner: String,
        name: String,
    },
    ArrayLoad {
        arr: Box<TE>,
        idx: Box<TE>,
    },
    ArrayLen(Box<TE>),
    Call {
        kind: Invoke,
        owner: String,
        name: String,
        desc: String,
        recv: Option<Box<TE>>,
        args: Vec<TE>,
    },
    New {
        class: String,
        desc: String,
        args: Vec<TE>,
    },
    /// `ty` is the full array type; one dimension expression per sized dimension.
    NewArray(Vec<TE>),
    ArrayLit(Vec<TE>),
    Neg(Box<TE>),
    /// Arithmetic / bitwise / shift. Operands are already converted to the
    /// operation type (`ty`), except shift counts which are int.
    Arith(BinOp, Box<TE>, Box<TE>),
    /// Comparison producing boolean. `operand_ty` is the promoted operand type.
    Cmp(BinOp, Box<TE>, Box<TE>),
    And(Box<TE>, Box<TE>),
    Or(Box<TE>, Box<TE>),
    Not(Box<TE>),
    Concat(Vec<TE>),
    Assign(Box<LV>, Box<TE>),
    /// target = (target_ty)((op_ty)target op value)
    CompoundAssign {
        target: Box<LV>,
        op: BinOp,
        value: Box<TE>,
        op_ty: Type,
    },
    /// target += parts... for String targets
    StrAppendAssign {
        target: Box<LV>,
        parts: Vec<TE>,
    },
    IncDec {
        target: Box<LV>,
        pre: bool,
        inc: bool,
    },
    Cond(Box<TE>, Box<TE>, Box<TE>),
    /// Primitive conversion from the inner expression's type to `ty`.
    Convert(Box<TE>),
    CheckCast(Box<TE>),
    InstanceOf(Box<TE>, Type),
}

#[derive(Clone, Debug)]
pub enum LV {
    Local(u16, Type),
    Static {
        owner: String,
        name: String,
        ty: Type,
    },
    Field {
        obj: TE,
        owner: String,
        name: String,
        ty: Type,
    },
    Array {
        arr: TE,
        idx: TE,
        ty: Type,
    },
}

impl LV {
    pub fn ty(&self) -> &Type {
        match self {
            LV::Local(_, t) => t,
            LV::Static { ty, .. } | LV::Field { ty, .. } | LV::Array { ty, .. } => ty,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SwitchArm {
    pub values: Vec<i32>,
    pub is_default: bool,
    pub body: Vec<TS>,
}

#[derive(Clone, Debug)]
pub struct CatchArm {
    pub classes: Vec<String>,
    pub slot: u16,
    pub body: Vec<TS>,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum TS {
    /// Line-number marker for the following statement.
    Line(u32),
    Expr(TE),
    Block(Vec<TS>),
    If(TE, Box<TS>, Option<Box<TS>>),
    While {
        cond: TE,
        body: Box<TS>,
        id: u32,
    },
    DoWhile {
        body: Box<TS>,
        cond: TE,
        id: u32,
    },
    For {
        cond: Option<TE>,
        update: Vec<TE>,
        body: Box<TS>,
        id: u32,
    },
    /// A labeled non-loop statement that `break label` can exit.
    Labeled {
        body: Box<TS>,
        id: u32,
    },
    Return(Option<TE>),
    Break(u32),
    Continue(u32),
    Switch {
        scrut: TE,
        arms: Vec<SwitchArm>,
        id: u32,
    },
    Throw(TE),
    Try {
        body: Vec<TS>,
        catches: Vec<CatchArm>,
    },
}

pub struct TMethod {
    pub name: String,
    pub desc: String,
    pub flags: u16,
    pub body: Option<Vec<TS>>,
    pub max_locals: u16,
    pub start_line: u32,
}

pub struct TField {
    pub name: String,
    pub desc: String,
    pub flags: u16,
}

pub struct TClass {
    pub name: String,
    pub super_name: String,
    pub interfaces: Vec<String>,
    pub flags: u16,
    pub fields: Vec<TField>,
    pub methods: Vec<TMethod>,
    pub source_file: String,
}
