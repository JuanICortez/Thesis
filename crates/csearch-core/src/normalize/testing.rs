//! Shared test fixtures for stage 2.
//!
//! Lives here rather than in each pass's own test module so that a pass tests
//! itself against the same inputs the contract properties use.

use crate::cast::TranslationUnit;
use crate::lower::build_translation_unit;
use tree_sitter::Parser;

pub fn lower(src: &str) -> TranslationUnit {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("C parser");
    let tree = parser.parse(src, None).expect("parse");
    build_translation_unit(&tree, src)
}

/// Input for the contract properties, with at least one case per planned pass.
///
/// Deliberately includes trees that already hold `Unsupported` nodes:
/// `normalize` runs before stage 3 rejects anything, so every pass has to
/// tolerate them rather than assume a fully-modelled tree.
pub const CORPUS: &[&str] = &[
    "",
    "int f(void) { return 1; }",
    "int f(int a, int b) { int c = a + b; c = g(c, 1); return c; }",
    // split_declarations
    "int f(void) { int a, b = 2; return b; }",
    // desugar_compound_assign — including the aliasing trap
    "int f(void) { int a = 0; a += 1; return a; }",
    "int f(int *arr) { int i = 0; arr[i++] += 1; return i; }",
    // desugar_incdec, in statement and in value position
    "int f(void) { int i = 0; i++; return i; }",
    "int f(void) { int i = 0; int j = i++; return j; }",
    // for_to_while — the second case is the `continue` trap
    "int f(void) { for (int i = 0; i < 10; i++) { g(i); } return 0; }",
    "int f(void) { for (int i = 0; i < 10; i++) { if (i) continue; g(i); } return 0; }",
    // flatten_blocks — the second case is the shadowing trap
    "int f(void) { { int a = 1; { return a; } } }",
    "int f(void) { { int a = 1; g(a); } { int a = 2; g(a); } return 0; }",
    // Top-level constructs stage 1 does not model.
    "typedef int myint; int g = 1; int f(void) { return 1; }",
];
