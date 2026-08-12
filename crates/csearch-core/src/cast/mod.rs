//! The C AST (`CAst`) — an owned, tree-sitter-independent representation of C.
//!
//! Stage 1 (`lower`) builds these types from a tree-sitter CST. Stage 2
//! (`normalize`) rewrites them in place. Stage 3 (`abt`) lowers them into the
//! e-graph language.
//!
//! Every node is a struct carrying a `kind` and a `Span`, so pattern matching
//! never has to bind or ignore the span. `Identifier` is the exception: it is
//! a name, not a node, and it is used as a hash key — giving it a span would
//! make two occurrences of the same variable compare unequal.

pub mod lower;

pub use lower::build_translation_unit;

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier(pub String);

#[derive(Debug, Clone)]
pub struct TranslationUnit {
    pub functions: Vec<Function>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: Identifier,
    pub params: Vec<Identifier>,
    pub body: Compound,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Compound {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Declaration {
        name: Identifier,
        value: Option<Expression>,
    },

    Assign {
        lhs: Expression,
        rhs: Expression,
    },

    ExprStmt(Expression),

    Return(Option<Expression>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
pub struct Expression {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Variable(Identifier),
    Int(i64),
    String(String),

    BinaryOp {
        op: BinOp,
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },

    Call {
        callee: Identifier,
        args: Vec<Expression>,
    },
}

impl Statement {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Statement { kind, span }
    }
}

impl Expression {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expression { kind, span }
    }

    pub fn boxed(kind: ExprKind, span: Span) -> Box<Self> {
        Box::new(Expression::new(kind, span))
    }
}
