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

pub mod dump;
pub mod lower;
pub mod print;

pub use dump::{dump, dump_with_spans};
pub use lower::build_translation_unit;
pub use print::print;

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

    /// A statement stage 1 does not model yet. Carries the tree-sitter node
    /// kind that produced it. Lowering never drops input silently; stage 3
    /// turns these into hard errors.
    Unsupported {
        kind: String,
    },
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

    /// An expression stage 1 does not model yet. See [`StmtKind::Unsupported`].
    Unsupported {
        kind: String,
    },
}

impl Statement {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Statement { kind, span }
    }
}

/// A construct stage 1 could not model, as reported by [`collect_unsupported`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    /// The tree-sitter node kind, e.g. `"for_statement"`.
    pub kind: String,
    pub span: Span,
}

/// Walks a unit and reports everything lowering could not model.
pub fn collect_unsupported(unit: &TranslationUnit) -> Vec<Unsupported> {
    let mut out = Vec::new();
    for function in &unit.functions {
        for statement in &function.body.statements {
            collect_in_statement(statement, &mut out);
        }
    }
    out
}

fn collect_in_statement(statement: &Statement, out: &mut Vec<Unsupported>) {
    match &statement.kind {
        StmtKind::Unsupported { kind } => out.push(Unsupported {
            kind: kind.clone(),
            span: statement.span,
        }),
        StmtKind::Declaration { value, .. } => {
            if let Some(value) = value {
                collect_in_expression(value, out);
            }
        }
        StmtKind::Assign { lhs, rhs } => {
            collect_in_expression(lhs, out);
            collect_in_expression(rhs, out);
        }
        StmtKind::ExprStmt(expression) => collect_in_expression(expression, out),
        StmtKind::Return(value) => {
            if let Some(value) = value {
                collect_in_expression(value, out);
            }
        }
    }
}

fn collect_in_expression(expression: &Expression, out: &mut Vec<Unsupported>) {
    match &expression.kind {
        ExprKind::Unsupported { kind } => out.push(Unsupported {
            kind: kind.clone(),
            span: expression.span,
        }),
        ExprKind::BinaryOp { lhs, rhs, .. } => {
            collect_in_expression(lhs, out);
            collect_in_expression(rhs, out);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                collect_in_expression(arg, out);
            }
        }
        ExprKind::Variable(_) | ExprKind::Int(_) | ExprKind::String(_) => {}
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
