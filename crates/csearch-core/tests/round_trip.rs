//! The round-trip property: `lower(parse(print(ast))) ≡ ast`.
//!
//! This is the safety net under stage 1. Hand-written expected trees only
//! check the cases someone thought to write down; the round-trip checks that
//! whatever `lower` produced survives a trip through real C text and a real
//! parse — which is what catches lowering silently dropping, reordering, or
//! regrouping a construct. The `int f(void)` bug that made every
//! `(void)`-prototyped function invisible was exactly this shape.
//!
//! Equality is `dump`, which omits spans: re-parsing printed C moves every
//! byte offset, so spans cannot be part of the comparison. When it fails, the
//! two dumps diff as readable trees.
//!
//! An integration test rather than a unit one, because the property spans
//! `lower`, `print`, and `dump` and only uses their public API.

use csearch_core::cast::{
    build_translation_unit, dump, print, Compound, ExprKind, Expression, Function, Identifier,
    Item, ItemKind, Statement, StmtKind, TranslationUnit,
};
use csearch_core::span::Span;
use tree_sitter::Parser;

fn lower(src: &str) -> TranslationUnit {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("C parser");
    let tree = parser.parse(src, None).expect("parse");
    build_translation_unit(&tree, src)
}

/// Asserts the property for one tree, and returns the printed C.
///
/// Also checks that printing is idempotent — `print(lower(print(ast)))` is the
/// same text. Structural equality alone would tolerate a printer that emitted
/// different-but-equivalent text on each pass, which would make printed C
/// useless as golden-test output.
fn assert_round_trips(original: &TranslationUnit) -> String {
    let printed = match print(original) {
        Ok(printed) => printed,
        Err(unsupported) => panic!(
            "round-trip input must be fully modelled, but lowering gave up on: {:?}",
            unsupported.iter().map(|u| &u.kind).collect::<Vec<_>>()
        ),
    };

    let reparsed = lower(&printed);
    assert_eq!(
        dump(original),
        dump(&reparsed),
        "tree changed on round-trip through:\n{printed}"
    );

    let reprinted = print(&reparsed).expect("a round-tripped tree still prints");
    assert_eq!(printed, reprinted, "printing is not idempotent");

    printed
}

fn assert_source_round_trips(src: &str) -> String {
    assert_round_trips(&lower(src))
}

/// One case per construct `lower` models, plus the groupings that are easy to
/// get wrong. Every entry must survive `print` — an `Unsupported` node here is
/// a failure, not a skip, so this list also pins what stage 1 covers.
#[test]
fn every_modelled_construct_round_trips() {
    let corpus = [
        // Function shapes.
        "int f(void) { return 1; }",
        "int f() { return 1; }",
        "int f(int a) { return a; }",
        "int f(int a, int b, int c) { return a; }",
        "int f(void) { return 1; } int g(int a) { return a; } int h(void) { return 2; }",
        // Statements.
        "int f(void) { int a; return; }",
        "int f(void) { int a = 1; a = 2; return a; }",
        "int f(int a) { g(a); return a; }",
        "int f(void) { return; }",
        // Calls.
        "int f(void) { return g(); }",
        "int f(int a) { return g(a, 1, h(a)); }",
        // Every operator.
        "int f(int a, int b) { return a + b; }",
        "int f(int a, int b) { return a - b; }",
        "int f(int a, int b) { return a * b; }",
        "int f(int a, int b) { return a / b; }",
        // Precedence and associativity — the cases where a printer that
        // parenthesised too little or too much would change the tree.
        "int f(void) { return 1 + 2 * 3; }",
        "int f(void) { return (1 + 2) * 3; }",
        "int f(void) { return 1 - 2 - 3; }",
        "int f(void) { return 1 - (2 - 3); }",
        "int f(void) { return 1 / 2 / 3; }",
        "int f(void) { return 1 / (2 * 3); }",
        "int f(void) { return (1 + 2) * (3 - 4) / (5 + 6); }",
        "int f(void) { return ((1)) + (2); }",
        // Integer literals normalise to decimal on the way out, so the second
        // pass parses different text than the first and must still agree.
        "int f(void) { return 0x1f + 010 + 42; }",
    ];

    for src in corpus {
        assert_source_round_trips(src);
    }
}

/// A tree with no functions is the degenerate case at both ends: `print`
/// produces the empty string and `lower` must read it back as an empty unit.
#[test]
fn an_empty_unit_round_trips() {
    assert_eq!(assert_source_round_trips(""), "");
}

/// Built by hand because no C source lowers to a negative `Int` today — unary
/// minus is still `Unsupported`. Constant folding in stage 2 will produce
/// them, and `1 - -2` has to print with parentheses to survive re-parsing, so
/// the property is worth pinning before the producer exists.
#[test]
fn negative_literals_round_trip() {
    let span = Span::new(0, 0);
    let negative = |value: i64| Expression::boxed(ExprKind::Int(value), span);

    let unit = TranslationUnit {
        items: vec![Item::new(
            ItemKind::Function(Function {
                name: Identifier("f".to_string()),
                params: Vec::new(),
                body: Compound {
                    statements: vec![Statement::new(
                        StmtKind::Return(Some(Expression::new(
                            ExprKind::BinaryOp {
                                op: csearch_core::cast::BinOp::Sub,
                                lhs: negative(1),
                                rhs: negative(-2),
                            },
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

    assert_round_trips(&unit);
}

/// The property is only worth running if it can fail. Dropping the
/// parentheses from printed C must change the dump — otherwise the comparison
/// is vacuous and every test above passes for the wrong reason.
#[test]
fn the_property_has_teeth() {
    let printed = assert_source_round_trips("int f(void) { return 1 - (2 - 3); }");
    assert!(printed.contains("1 - (2 - 3)"));

    let regrouped = lower(&printed.replace("1 - (2 - 3)", "1 - 2 - 3"));
    assert_ne!(dump(&lower(&printed)), dump(&regrouped));
}
