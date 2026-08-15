//! The C AST (`CAst`) — an owned, tree-sitter-independent representation of C.
//!
//! This module is the tree and the two ways to render it ([`print`], [`dump`]);
//! nothing here rewrites. The stages that do live beside it, one module per
//! stage: [`crate::lower`] builds these types from a tree-sitter CST,
//! [`crate::normalize`] rewrites them, and [`crate::abt`] lowers them into the
//! e-graph language.
//!
//! Every node is a struct carrying a `kind` and a `Span`, so pattern matching
//! never has to bind or ignore the span. `Identifier` is the exception: it is
//! a name, not a node, and it is used as a hash key — giving it a span would
//! make two occurrences of the same variable compare unequal.

pub mod dump;
pub mod print;

pub use dump::{dump, dump_with_spans};
pub use print::print;

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier(pub String);

#[derive(Debug, Clone)]
pub struct TranslationUnit {
    pub items: Vec<Item>,
    pub span: Span,
}

/// A top-level construct. Anything that is not a function definition — a
/// global, a `typedef`, a struct definition, a preprocessor directive — is
/// [`ItemKind::Unsupported`] rather than absent, so the no-silent-drops
/// contract holds at the top level too.
#[derive(Debug, Clone)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    Function(Function),

    /// See [`StmtKind::Unsupported`]. Note that a purely type-level construct
    /// belongs here permanently, not as a gap to close: the CAst models no
    /// types, so a `typedef` has nothing to lower *into*.
    Unsupported {
        kind: String,
    },
}

impl Item {
    pub fn new(kind: ItemKind, span: Span) -> Self {
        Item { kind, span }
    }
}

impl TranslationUnit {
    /// The function definitions, in source order, skipping everything stage 1
    /// could not model. Callers that need to know what was skipped should ask
    /// [`collect_unsupported`] rather than walking `items` themselves.
    pub fn functions(&self) -> impl Iterator<Item = &Function> {
        self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::Function(function) => Some(function),
            ItemKind::Unsupported { .. } => None,
        })
    }
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
    for item in &unit.items {
        match &item.kind {
            ItemKind::Unsupported { kind } => out.push(Unsupported {
                kind: kind.clone(),
                span: item.span,
            }),
            ItemKind::Function(function) => {
                for statement in &function.body.statements {
                    collect_in_statement(statement, &mut out);
                }
            }
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
