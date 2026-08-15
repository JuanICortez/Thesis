//! CAst → C source.
//!
//! Exists for three reasons: golden tests that read as C instead of as `Debug`
//! output, the round-trip property `lower(parse(print(ast))) ≡ ast`, and
//! normalized C that can go straight into the dissertation as a figure.
//!
//! The CAst does not model types, so every declaration prints as `int`. That
//! is lossless in both directions today — nothing downstream reads a type —
//! but it is the first thing to revisit when types are added.

use std::fmt::Write as _;

use super::{
    collect_unsupported, BinOp, Compound, ExprKind, Expression, Function, Identifier, Statement,
    StmtKind, TranslationUnit, Unsupported,
};

const INDENT: &str = "    ";

/// Precedence of a printed expression, used to decide where parentheses are
/// needed. Higher binds tighter; `PRIMARY` never needs them.
const PREC_ADDITIVE: u8 = 1;
const PREC_MULTIPLICATIVE: u8 = 2;
const PRIMARY: u8 = 3;
const PREC_NEGATIVE_LITERAL: u8 = 0;

/// Prints `unit` as compilable C.
///
/// Fails if the tree still contains `Unsupported` nodes. There is no C text
/// that means "a construct stage 1 could not model", and inventing one — `0`,
/// an empty statement — would produce exactly the confidently-wrong output
/// this pipeline exists to avoid. Callers that only want to eyeball a tree
/// should use `Debug`; callers that want C should fix the gap the `Err`
/// reports.
pub fn print(unit: &TranslationUnit) -> Result<String, Vec<Unsupported>> {
    let unsupported = collect_unsupported(unit);
    if !unsupported.is_empty() {
        return Err(unsupported);
    }

    let mut out = String::new();
    for (index, function) in unit.functions().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        write_function(&mut out, function);
    }
    Ok(out)
}

fn write_function(out: &mut String, function: &Function) {
    let params = if function.params.is_empty() {
        "void".to_string()
    } else {
        function
            .params
            .iter()
            .map(|Identifier(name)| format!("int {name}"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let _ = writeln!(out, "int {}({}) {{", function.name.0, params);
    write_compound(out, &function.body, 1);
    out.push_str("}\n");
}

fn write_compound(out: &mut String, compound: &Compound, depth: usize) {
    for statement in &compound.statements {
        write_statement(out, statement, depth);
    }
}

fn write_statement(out: &mut String, statement: &Statement, depth: usize) {
    for _ in 0..depth {
        out.push_str(INDENT);
    }

    let _ = match &statement.kind {
        StmtKind::Declaration { name, value } => match value {
            Some(value) => writeln!(out, "int {} = {};", name.0, print_expression(value)),
            None => writeln!(out, "int {};", name.0),
        },
        StmtKind::Assign { lhs, rhs } => {
            writeln!(
                out,
                "{} = {};",
                print_expression(lhs),
                print_expression(rhs)
            )
        }
        StmtKind::ExprStmt(value) => writeln!(out, "{};", print_expression(value)),
        StmtKind::Return(Some(value)) => writeln!(out, "return {};", print_expression(value)),
        StmtKind::Return(None) => writeln!(out, "return;"),
        StmtKind::Unsupported { kind } => writeln!(out, "/* unsupported: {kind} */"),
    };
}

fn print_expression(expression: &Expression) -> String {
    match &expression.kind {
        ExprKind::Variable(Identifier(name)) => name.clone(),
        ExprKind::Int(value) => value.to_string(),
        ExprKind::String(value) => quote(value),
        ExprKind::BinaryOp { op, lhs, rhs } => {
            let precedence = op_precedence(op);
            format!(
                "{} {} {}",
                operand(lhs, precedence, Side::Left),
                op_symbol(op),
                operand(rhs, precedence, Side::Right),
            )
        }
        ExprKind::Call { callee, args } => {
            let args = args
                .iter()
                .map(print_expression)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", callee.0, args)
        }
        ExprKind::Unsupported { kind } => format!("/* unsupported: {kind} */"),
    }
}

enum Side {
    Left,
    Right,
}

fn operand(inner: &Expression, parent: u8, side: Side) -> String {
    let text = print_expression(inner);
    let child = precedence(inner);

    let needs_parens = child < parent || (matches!(side, Side::Right) && child == parent);
    if needs_parens {
        format!("({text})")
    } else {
        text
    }
}

fn precedence(expression: &Expression) -> u8 {
    match &expression.kind {
        ExprKind::BinaryOp { op, .. } => op_precedence(op),
        ExprKind::Int(value) if *value < 0 => PREC_NEGATIVE_LITERAL,
        _ => PRIMARY,
    }
}

fn op_precedence(op: &BinOp) -> u8 {
    match op {
        BinOp::Add | BinOp::Sub => PREC_ADDITIVE,
        BinOp::Mul | BinOp::Div => PREC_MULTIPLICATIVE,
    }
}

fn op_symbol(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
    }
}

pub(super) fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(character),
        }
    }
    out.push('"');
    out
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

    fn printed(src: &str) -> String {
        print(&lower(src)).expect("no unsupported nodes")
    }

    #[test]
    fn prints_a_whole_function() {
        assert_eq!(
            printed("int f(int a, int b) { int c = a + b; c = g(c, 1); return c; }"),
            "int f(int a, int b) {\n    \
                 int c = a + b;\n    \
                 c = g(c, 1);\n    \
                 return c;\n\
             }\n",
        );
    }

    #[test]
    fn empty_parameter_list_prints_as_void() {
        assert_eq!(
            printed("int f() { return 1; }"),
            "int f(void) {\n    return 1;\n}\n"
        );
    }

    #[test]
    fn functions_are_separated_by_a_blank_line() {
        assert_eq!(
            printed("int f(void) { return 1; } int g(void) { return 2; }"),
            "int f(void) {\n    return 1;\n}\n\nint g(void) {\n    return 2;\n}\n",
        );
    }

    /// Parentheses appear exactly where dropping them would change the tree,
    /// and nowhere else.
    #[test]
    fn parenthesizes_only_where_grouping_matters() {
        let cases = [
            ("1 + 2 * 3", "1 + 2 * 3"),
            ("(1 + 2) * 3", "(1 + 2) * 3"),
            ("1 - 2 - 3", "1 - 2 - 3"),
            ("1 - (2 - 3)", "1 - (2 - 3)"),
            ("1 / (2 * 3)", "1 / (2 * 3)"),
            ("1 * 2 / 3", "1 * 2 / 3"),
            ("((1)) + (2)", "1 + 2"),
        ];
        for (src, expected) in cases {
            let printed = printed(&format!("int f(void) {{ return {src}; }}"));
            assert_eq!(
                printed,
                format!("int f(void) {{\n    return {expected};\n}}\n")
            );
        }
    }

    #[test]
    fn uninitialised_declaration_and_bare_return() {
        assert_eq!(
            printed("int f(void) { int a; return; }"),
            "int f(void) {\n    int a;\n    return;\n}\n",
        );
    }

    /// A single unmodelled top-level construct makes the whole unit
    /// unprintable. That is deliberate — there is no C text meaning "a typedef
    /// we could not model" — but it does mean real-world files will usually
    /// fail here, and `dump` is the tool for those.
    #[test]
    fn an_unsupported_item_makes_the_unit_unprintable() {
        let unit = lower("typedef int myint; int f(void) { return 1; }");
        let error = print(&unit).expect_err("typedefs are not modelled");
        assert_eq!(error.len(), 1);
        assert_eq!(error[0].kind, "type_definition");
    }

    /// The printer promises compilable C, so a tree that still holds an
    /// unmodelled construct is an error rather than a plausible-looking lie.
    #[test]
    fn unsupported_nodes_are_an_error_not_a_guess() {
        let unit = lower("int f(void) { while (1) { } return 1; }");
        let error = print(&unit).expect_err("while is not modelled yet");
        assert_eq!(error.len(), 1);
        assert_eq!(error[0].kind, "while_statement");
    }
}
