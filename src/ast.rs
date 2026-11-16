//! Untyped syntax tree produced by the parser.

use crate::diag::Pos;
use crate::types::Type;

pub mod modifier {
    pub const PUBLIC: u16 = 0x0001;
    pub const PRIVATE: u16 = 0x0002;
    pub const PROTECTED: u16 = 0x0004;
    pub const STATIC: u16 = 0x0008;
    pub const FINAL: u16 = 0x0010;
    pub const SYNCHRONIZED: u16 = 0x0020;
    pub const VOLATILE: u16 = 0x0040;
    pub const TRANSIENT: u16 = 0x0080;
    pub const NATIVE: u16 = 0x0100;
    pub const ABSTRACT: u16 = 0x0400;
    pub const STRICT: u16 = 0x0800;
    /// Not a real JVM flag: marks `default` interface methods so we can reject them.
    pub const DEFAULT: u16 = 0x8000;
}

#[derive(Clone, Debug)]
pub enum AstType {
    Prim(Type),
    Named(Vec<String>, Pos),
    Array(Box<AstType>),
    /// `var` in a local declaration.
    Var,
}

impl AstType {
    pub fn pos(&self) -> Option<Pos> {
        match self {
            AstType::Named(_, p) => Some(*p),
            AstType::Array(e) => e.pos(),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct CompilationUnit {
    pub file: usize,
    pub package: Option<String>,
    pub classes: Vec<ClassDecl>,
}

#[derive(Debug)]
pub struct ClassDecl {
    pub name: String,
    pub pos: Pos,
    pub mods: u16,
    pub is_interface: bool,
    pub extends: Option<AstType>,
    pub implements: Vec<AstType>,
    pub members: Vec<Member>,
    pub nested: Vec<ClassDecl>,
}

#[derive(Debug)]
pub enum Member {
    Field(FieldDecl),
    Method(MethodDecl),
    StaticInit(Vec<Stmt>, Pos),
    InstanceInit(Vec<Stmt>, Pos),
}

#[derive(Debug)]
pub struct FieldDecl {
    pub mods: u16,
    pub ty: AstType,
    pub name: String,
    pub init: Option<Expr>,
    pub pos: Pos,
}

#[derive(Debug)]
pub struct MethodDecl {
    pub mods: u16,
    pub ret: AstType,
    pub name: String,
    pub params: Vec<Param>,
    pub varargs: bool,
    pub body: Option<Vec<Stmt>>,
    pub pos: Pos,
    pub is_ctor: bool,
    pub end_pos: Pos,
}

#[derive(Debug)]
pub struct Param {
    pub ty: AstType,
    pub name: String,
    pub pos: Pos,
}

#[derive(Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub pos: Pos,
}

#[derive(Debug)]
pub struct VarDeclarator {
    pub name: String,
    pub extra_dims: usize,
    pub init: Option<Expr>,
    pub pos: Pos,
}

#[derive(Debug)]
pub struct SwitchCase {
    /// Empty with `is_default` for `default:`.
    pub labels: Vec<Expr>,
    pub is_default: bool,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug)]
pub struct Catch {
    pub types: Vec<AstType>,
    pub name: String,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug)]
pub enum StmtKind {
    Empty,
    Block(Vec<Stmt>),
    LocalVar(AstType, Vec<VarDeclarator>),
    Expr(Expr),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For {
        init: Vec<Stmt>,
        cond: Option<Expr>,
        update: Vec<Expr>,
        body: Box<Stmt>,
    },
    ForEach {
        ty: AstType,
        name: String,
        iter: Expr,
        body: Box<Stmt>,
    },
    Return(Option<Expr>),
    Break(Option<String>),
    Continue(Option<String>),
    Switch {
        scrut: Expr,
        cases: Vec<SwitchCase>,
        arrow: bool,
    },
    Throw(Expr),
    Try {
        body: Vec<Stmt>,
        catches: Vec<Catch>,
        finally: Option<Vec<Stmt>>,
    },
    Labeled(String, Box<Stmt>),
    /// `this(...)` or `super(...)` as the first statement of a constructor.
    CtorCall {
        is_super: bool,
        args: Vec<Expr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    UShr,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            Shl => "<<",
            Shr => ">>",
            UShr => ">>>",
            BitAnd => "&",
            BitOr => "|",
            BitXor => "^",
            And => "&&",
            Or => "||",
            Eq => "==",
            Ne => "!=",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
        }
    }
}

#[derive(Debug)]
pub enum Lit {
    Int(i64),
    Long(u64),
    Double(f64),
    Char(u16),
    Str(String),
    Bool(bool),
    Null,
}

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Debug)]
pub enum ExprKind {
    Lit(Lit),
    Name(String),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call {
        target: Option<Box<Expr>>,
        name: String,
        args: Vec<Expr>,
    },
    New(AstType, Vec<Expr>),
    NewArray {
        elem: AstType,
        dims: Vec<Expr>,
        extra_dims: usize,
        init: Option<Vec<Expr>>,
    },
    /// A bare `{a, b}` initializer; only legal where the array type is known.
    ArrayInit(Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    IncDec {
        pre: bool,
        inc: bool,
        target: Box<Expr>,
    },
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Assign {
        op: Option<BinOp>,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Cast(AstType, Box<Expr>),
    InstanceOf(Box<Expr>, AstType),
    This,
    Super,
}
