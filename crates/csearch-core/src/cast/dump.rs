//! CAst → an indented tree, for reading and for comparing.
//!
//! Where [`print`](super::print) emits compilable C and refuses to guess at
//! anything it cannot express, this module renders whatever is in the tree —
//! `Unsupported` nodes included. It is total: every CAst has a dump.
//!
//! Two uses:
//!
//! 1. Reading a tree while debugging, without wading through `Debug` output.
//! 2. Structural equality. [`dump`] omits spans, so two trees that differ only
//!    in source position produce byte-identical output and `dump(a) == dump(b)`
//!    is the spans-ignoring comparison the round-trip property needs — with a
//!    readable diff when it fails. Use [`dump_with_spans`] when positions are
//!    the thing under test.

use super::{
    BinOp, Compound, ExprKind, Expression, Function, Identifier, Item, ItemKind, Statement,
    StmtKind, TranslationUnit,
};
use crate::span::Span;

/// Renders `unit` as a tree, without spans.
pub fn dump(unit: &TranslationUnit) -> String {
    render(&unit_node(unit, Spans::Hide))
}

/// Renders `unit` as a tree, annotating every node with its byte range.
pub fn dump_with_spans(unit: &TranslationUnit) -> String {
    render(&unit_node(unit, Spans::Show))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Spans {
    Show,
    Hide,
}

/// An already-labelled node. Building this first keeps the walk over the CAst
/// separate from the box-drawing, which only needs to know how many children
/// each node has.
struct Node {
    label: String,
    children: Vec<Node>,
}

impl Node {
    fn new(label: impl Into<String>, span: Span, spans: Spans, children: Vec<Node>) -> Self {
        let mut label = label.into();
        if spans == Spans::Show {
            label.push_str(&format!(" @{}..{}", span.start, span.end));
        }
        Node { label, children }
    }

    fn leaf(label: impl Into<String>, span: Span, spans: Spans) -> Self {
        Node::new(label, span, spans, Vec::new())
    }
}

const LAST: &str = "└─ ";
const BRANCH: &str = "├─ ";
const GAP: &str = "   ";
const TRUNK: &str = "│  ";

fn render(root: &Node) -> String {
    let mut out = String::new();
    out.push_str(&root.label);
    out.push('\n');
    write_children(&mut out, root, "");
    out
}

fn write_children(out: &mut String, node: &Node, prefix: &str) {
    let Some(last) = node.children.len().checked_sub(1) else {
        return;
    };

    for (index, child) in node.children.iter().enumerate() {
        let is_last = index == last;
        out.push_str(prefix);
        out.push_str(if is_last { LAST } else { BRANCH });
        out.push_str(&child.label);
        out.push('\n');

        let deeper = format!("{prefix}{}", if is_last { GAP } else { TRUNK });
        write_children(out, child, &deeper);
    }
}

fn unit_node(unit: &TranslationUnit, spans: Spans) -> Node {
    let items = unit
        .items
        .iter()
        .map(|item| item_node(item, spans))
        .collect();
    Node::new("TranslationUnit", unit.span, spans, items)
}

/// A function `Item` renders as its `Function` directly. The two carry the same
/// span and the same information, so a separate `Item` level would only add a
/// line of noise to every dump.
fn item_node(item: &Item, spans: Spans) -> Node {
    match &item.kind {
        ItemKind::Function(function) => function_node(function, spans),
        ItemKind::Unsupported { kind } => {
            Node::leaf(format!("Unsupported item {kind}"), item.span, spans)
        }
    }
}

fn function_node(function: &Function, spans: Spans) -> Node {
    // Parameters are `Identifier`s, not nodes — they carry no span of their
    // own, so they belong in the label rather than as children.
    let params = function
        .params
        .iter()
        .map(|Identifier(name)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    Node::new(
        format!("Function {} params: [{params}]", function.name.0),
        function.span,
        spans,
        vec![compound_node(&function.body, spans)],
    )
}

fn compound_node(compound: &Compound, spans: Spans) -> Node {
    let statements = compound
        .statements
        .iter()
        .map(|statement| statement_node(statement, spans))
        .collect();
    Node::new("Compound", compound.span, spans, statements)
}

fn statement_node(statement: &Statement, spans: Spans) -> Node {
    let (label, children) = match &statement.kind {
        StmtKind::Declaration { name, value } => (
            format!("Declaration {}", name.0),
            value
                .iter()
                .map(|value| expression_node(value, spans))
                .collect(),
        ),
        StmtKind::Assign { lhs, rhs } => (
            "Assign".to_string(),
            vec![expression_node(lhs, spans), expression_node(rhs, spans)],
        ),
        StmtKind::ExprStmt(value) => ("ExprStmt".to_string(), vec![expression_node(value, spans)]),
        StmtKind::Return(value) => (
            "Return".to_string(),
            value
                .iter()
                .map(|value| expression_node(value, spans))
                .collect(),
        ),
        StmtKind::Unsupported { kind } => (format!("Unsupported statement {kind}"), Vec::new()),
    };

    Node::new(label, statement.span, spans, children)
}

fn expression_node(expression: &Expression, spans: Spans) -> Node {
    let span = expression.span;
    match &expression.kind {
        ExprKind::Variable(Identifier(name)) => Node::leaf(format!("Variable {name}"), span, spans),
        ExprKind::Int(value) => Node::leaf(format!("Int {value}"), span, spans),
        ExprKind::String(value) => Node::leaf(format!("String {}", quote(value)), span, spans),
        ExprKind::BinaryOp { op, lhs, rhs } => Node::new(
            format!("BinaryOp {}", op_name(op)),
            span,
            spans,
            vec![expression_node(lhs, spans), expression_node(rhs, spans)],
        ),
        ExprKind::Call { callee, args } => Node::new(
            format!("Call {}", callee.0),
            span,
            spans,
            args.iter().map(|arg| expression_node(arg, spans)).collect(),
        ),
        ExprKind::Unsupported { kind } => {
            Node::leaf(format!("Unsupported expression {kind}"), span, spans)
        }
    }
}

fn op_name(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "Add",
        BinOp::Sub => "Sub",
        BinOp::Mul => "Mul",
        BinOp::Div => "Div",
    }
}

/// Escapes as C would, so a string literal never introduces a newline that
/// would be read as another line of the tree.
fn quote(value: &str) -> String {
    super::print::quote(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::build_translation_unit;
    use tree_sitter::Parser;

    fn lower(src: &str) -> TranslationUnit {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .expect("C parser");
        let tree = parser.parse(src, None).expect("parse");
        build_translation_unit(&tree, src)
    }

    fn dumped(src: &str) -> String {
        dump(&lower(src))
    }

    #[test]
    fn dumps_a_whole_function() {
        assert_eq!(
            dumped("int f(int a, int b) { int c = a + b; c = g(c, 1); return c; }"),
            "\
TranslationUnit
└─ Function f params: [a, b]
   └─ Compound
      ├─ Declaration c
      │  └─ BinaryOp Add
      │     ├─ Variable a
      │     └─ Variable b
      ├─ Assign
      │  ├─ Variable c
      │  └─ Call g
      │     ├─ Variable c
      │     └─ Int 1
      └─ Return
         └─ Variable c
",
        );
    }

    /// Nesting on the left and on the right, so both the `│` trunk and the
    /// blank gap under a last child are exercised.
    #[test]
    fn nested_expressions_indent_under_their_parent() {
        assert_eq!(
            dumped("int f(void) { return (1 - 2) * (3 + 4); }"),
            "\
TranslationUnit
└─ Function f params: []
   └─ Compound
      └─ Return
         └─ BinaryOp Mul
            ├─ BinaryOp Sub
            │  ├─ Int 1
            │  └─ Int 2
            └─ BinaryOp Add
               ├─ Int 3
               └─ Int 4
",
        );
    }

    /// Nodes whose optional child is absent print as leaves, not as a parent
    /// with an empty branch.
    #[test]
    fn absent_optional_children_leave_no_branch() {
        assert_eq!(
            dumped("int f(void) { int a; return; }"),
            "\
TranslationUnit
└─ Function f params: []
   └─ Compound
      ├─ Declaration a
      └─ Return
",
        );
    }

    /// Unlike `print`, which errors on an unmodelled construct, the dump shows
    /// it — seeing where lowering gave up is the point of reading a dump.
    #[test]
    fn unsupported_nodes_are_shown_not_an_error() {
        assert_eq!(
            dumped("int f(void) { while (1) { } return 1; }"),
            "\
TranslationUnit
└─ Function f params: []
   └─ Compound
      ├─ Unsupported statement while_statement
      └─ Return
         └─ Int 1
",
        );
    }

    /// A literal newline in a string would otherwise be read as another line
    /// of the tree.
    ///
    /// Built by hand: `lower` does not produce `ExprKind::String` yet — string
    /// literals still become `Unsupported` — so there is no source text that
    /// reaches this branch.
    #[test]
    fn string_literals_are_escaped() {
        let span = Span::new(0, 0);
        let unit = TranslationUnit {
            items: vec![Item::new(
                ItemKind::Function(Function {
                    name: Identifier("f".to_string()),
                    params: Vec::new(),
                    body: Compound {
                        statements: vec![Statement::new(
                            StmtKind::Return(Some(Expression::new(
                                ExprKind::String("a\nb".to_string()),
                                span,
                            ))),
                            span,
                        )],
                        span,
                    },
                    span,
                }),
                span,
            )],
            span,
        };

        assert_eq!(
            dump(&unit),
            "\
TranslationUnit
└─ Function f params: []
   └─ Compound
      └─ Return
         └─ String \"a\\nb\"
",
        );
    }

    /// The property the round-trip test will lean on: same tree, different
    /// positions, identical dump.
    #[test]
    fn dump_ignores_spans_but_dump_with_spans_does_not() {
        let tight = lower("int f(void){return 1+2;}");
        let loose = lower("int f ( void )\n{\n    return 1 + 2;\n}\n");

        assert_eq!(dump(&tight), dump(&loose));
        assert_ne!(dump_with_spans(&tight), dump_with_spans(&loose));
    }

    #[test]
    fn spans_annotate_every_node() {
        assert_eq!(
            dump_with_spans(&lower("int f(void) { return 1; }")),
            "\
TranslationUnit @0..25
└─ Function f params: [] @0..25
   └─ Compound @12..25
      └─ Return @14..23
         └─ Int 1 @21..22
",
        );
    }

    #[test]
    fn an_empty_unit_is_a_lone_root() {
        assert_eq!(dumped(""), "TranslationUnit\n");
    }

    /// Top-level constructs stage 1 does not model appear in the dump, in
    /// source order, alongside the functions.
    #[test]
    fn unsupported_items_appear_beside_functions() {
        assert_eq!(
            dumped("typedef int myint;\nint x = 1;\nint f(void) { return 1; }"),
            "\
TranslationUnit
├─ Unsupported item type_definition
├─ Unsupported item declaration
└─ Function f params: []
   └─ Compound
      └─ Return
         └─ Int 1
",
        );
    }
}
