//! Stage 1: CST → CAst.
//!
//! Translation only — no semantic rewriting happens here. See
//! `CST_TO_CAST_PLAN.md`.

use tree_sitter::{Node, Tree};

use super::{
    BinOp, Compound, ExprKind, Expression, Function, Identifier, Statement, StmtKind,
    TranslationUnit,
};
use crate::span::Span;

struct ASTBuilder<'a> {
    src: &'a str,
}

pub fn build_translation_unit(tree: &Tree, source: &str) -> TranslationUnit {
    let builder = ASTBuilder::new(source);
    builder.build_translation_unit(tree)
}

impl<'a> ASTBuilder<'a> {
    pub fn new(src: &'a str) -> Self {
        ASTBuilder { src }
    }

    // ======================
    // Extractors
    // ======================

    fn span(&self, node: Node) -> Span {
        Span::new(node.start_byte(), node.end_byte())
    }

    fn extract_text(&self, node: Node) -> String {
        node.utf8_text(self.src.as_bytes())
            .unwrap_or_default()
            .to_string()
    }

    fn extract_identifier(&self, node: Node) -> Identifier {
        assert_eq!(node.kind(), "identifier");
        let name = self.extract_text(node);
        Identifier(name)
    }

    /// Peels a declarator down to the identifier it eventually names.
    ///
    /// C declarators nest: `int *p`, `int a[10]`, `int (*f)(void)`. Only the
    /// innermost node is an `identifier`, so this recurses rather than
    /// asserting. The `usize` is the pointer depth stripped on the way down;
    /// callers currently discard it, since the CAst does not model types yet.
    fn unwrap_declarator(&self, node: Node) -> (Identifier, usize) {
        match node.kind() {
            "identifier" => (self.extract_identifier(node), 0),
            "pointer_declarator" => {
                let inner = node
                    .child_by_field_name("declarator")
                    .expect("Inner declarator of pointer_declarator");
                let (name, depth) = self.unwrap_declarator(inner);
                (name, depth + 1)
            }
            "array_declarator" | "function_declarator" => {
                let inner = node
                    .child_by_field_name("declarator")
                    .expect("Inner declarator");
                self.unwrap_declarator(inner)
            }
            "parenthesized_declarator" => {
                let inner = node
                    .named_child(0)
                    .expect("Inner declarator of parenthesized_declarator");
                self.unwrap_declarator(inner)
            }
            _ => panic!("Unknown declarator kind: {}", node.kind()),
        }
    }

    fn extract_number(&self, node: Node) -> i64 {
        assert_eq!(node.kind(), "number_literal");
        let text = self.extract_text(node);
        text.parse::<i64>().unwrap_or_default()
    }

    fn get_operation(&self, node: Node) -> BinOp {
        let operator = node
            .child_by_field_name("operator")
            .expect("Operator field in node");
        match self.extract_text(operator).as_str() {
            "+" => BinOp::Add,
            "-" => BinOp::Sub,
            "*" => BinOp::Mul,
            "/" => BinOp::Div,
            _ => panic!("Unknown binary operator: {}", node.kind()),
        }
    }

    // ======================
    // Builders
    // ======================

    pub fn build_translation_unit(&self, tree: &Tree) -> TranslationUnit {
        let root = tree.root_node();
        assert_eq!(root.kind(), "translation_unit");

        let mut functions = Vec::new();
        let mut cursor = root.walk();

        for child in root.named_children(&mut cursor) {
            if child.kind() == "function_definition" {
                functions.push(self.build_function(child));
            }
        }
        TranslationUnit {
            functions,
            span: self.span(root),
        }
    }

    fn build_function(&self, node: Node) -> Function {
        assert_eq!(node.kind(), "function_definition");

        let declarator = node
            .child_by_field_name("declarator")
            .expect("Function Declarator");

        let (name, params) = self.deconstruct_declarator(declarator);

        let body = node.child_by_field_name("body").expect("Function Body");
        let mut statements = Vec::new();
        let mut cursor = body.walk();

        for child in body.named_children(&mut cursor) {
            statements.extend(self.build_statement(child));
        }

        Function {
            name,
            params,
            body: Compound {
                statements,
                span: self.span(body),
            },
            span: self.span(node),
        }
    }

    fn deconstruct_declarator(&self, node: Node) -> (Identifier, Vec<Identifier>) {
        assert_eq!(node.kind(), "function_declarator");

        let name_node = node
            .child_by_field_name("declarator")
            .expect("Function Identifier");
        let (name, _pointer_depth) = self.unwrap_declarator(name_node);

        let params = if let Some(param_list) = node.child_by_field_name("parameters") {
            let mut param_identifiers = Vec::new();
            let mut cursor = param_list.walk();

            for child in param_list.named_children(&mut cursor) {
                if child.kind() == "parameter_declaration" {
                    let param_node = child
                        .child_by_field_name("declarator")
                        .expect("Parameter Declarator");
                    let (param_identifier, _pointer_depth) = self.unwrap_declarator(param_node);
                    param_identifiers.push(param_identifier);
                }
            }
            param_identifiers
        } else {
            Vec::new()
        };
        (name, params)
    }

    fn build_statement(&self, node: Node) -> Vec<Statement> {
        let span = self.span(node);
        match node.kind() {
            "declaration" => self
                .process_declaration(node)
                .into_iter()
                .map(|(name, value)| Statement::new(StmtKind::Declaration { name, value }, span))
                .collect(),
            "expression_statement" => vec![self.process_expression_statement(node)],
            "return_statement" => vec![Statement::new(
                StmtKind::Return(node.named_child(0).map(|e| self.build_expression(e))),
                span,
            )],
            _ => Vec::new(),
        }
    }

    fn process_declaration(&self, node: Node) -> Vec<(Identifier, Option<Expression>)> {
        assert_eq!(node.kind(), "declaration");

        let mut declarations = Vec::new();
        let mut cursor = node.walk();

        let declarators = node.children_by_field_name("declarator", &mut cursor);

        for declarator in declarators {
            match declarator.kind() {
                "init_declarator" => {
                    let (identifier, _pointer_depth) = self.unwrap_declarator(
                        declarator
                            .child_by_field_name("declarator")
                            .expect("Declaration Identifier"),
                    );
                    let value = self.build_expression(
                        declarator
                            .child_by_field_name("value")
                            .expect("Declaration Value"),
                    );
                    declarations.push((identifier, Some(value)));
                }
                // Everything else is an uninitialized declarator: `int a;`,
                // `int *p;`, `int a[10];`, `int (*f)(void);`
                _ => {
                    let (identifier, _pointer_depth) = self.unwrap_declarator(declarator);
                    declarations.push((identifier, None));
                }
            }
        }
        declarations
    }

    fn process_expression_statement(&self, node: Node) -> Statement {
        assert_eq!(node.kind(), "expression_statement");

        let expression_child = node
            .named_child(0)
            .expect("Expression Statement to have child");

        match expression_child.kind() {
            "assignment_expression" => {
                let lhs = expression_child
                    .child_by_field_name("left")
                    .expect("Left Hand Side expression in Assignment");
                let rhs = expression_child
                    .child_by_field_name("right")
                    .expect("Right Hand Side expression in Assignment");

                // `a += b` and friends are *not* plain assignments. Modelling
                // them as one would silently drop the operator; desugaring is
                // stage 2's job. Loud until stage 1 grows an `Unsupported`.
                let operator = expression_child
                    .child_by_field_name("operator")
                    .expect("Operator field in Assignment");
                assert_eq!(
                    self.extract_text(operator),
                    "=",
                    "compound assignment is not modelled yet"
                );

                Statement::new(
                    StmtKind::Assign {
                        lhs: self.build_expression(lhs),
                        rhs: self.build_expression(rhs),
                    },
                    self.span(node),
                )
            }
            _ => Statement::new(
                StmtKind::ExprStmt(self.build_expression(expression_child)),
                self.span(node),
            ),
        }
    }

    fn build_expression(&self, node: Node) -> Expression {
        let span = self.span(node);
        let kind = match node.kind() {
            "identifier" => ExprKind::Variable(self.extract_identifier(node)),
            "number_literal" => ExprKind::Int(self.extract_number(node)),
            "binary_expression" => {
                let lhs = node
                    .child_by_field_name("left")
                    .expect("Left Hand Side in Binary Expression");
                let rhs = node
                    .child_by_field_name("right")
                    .expect("Right Hand Side in Binary Expression");

                ExprKind::BinaryOp {
                    op: self.get_operation(node),
                    lhs: Box::new(self.build_expression(lhs)),
                    rhs: Box::new(self.build_expression(rhs)),
                }
            }
            "call_expression" => {
                let callee = node
                    .child_by_field_name("function")
                    .expect("Function Name in Call Expression");
                let args = node
                    .child_by_field_name("arguments")
                    .map_or(Vec::new(), |args_list| {
                        args_list
                            .named_children(&mut args_list.walk())
                            .map(|arg| self.build_expression(arg))
                            .collect()
                    });

                ExprKind::Call {
                    callee: self.extract_identifier(callee),
                    args,
                }
            }
            _ => todo!("Unknown Expression"),
        };
        Expression::new(kind, span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast::{ExprKind, StmtKind};
    use tree_sitter::Parser;

    fn lower(src: &str) -> TranslationUnit {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .expect("C parser");
        let tree = parser.parse(src, None).expect("parse");
        build_translation_unit(&tree, src)
    }

    /// Regression: the C grammar field is `right`, not `rhs`.
    #[test]
    fn plain_assignment_lowers() {
        let unit = lower("int f() { int a; a = 2; }");
        let stmts = &unit.functions[0].body.statements;
        match &stmts[1].kind {
            StmtKind::Assign { lhs, rhs } => {
                assert!(matches!(&lhs.kind, ExprKind::Variable(Identifier(n)) if n == "a"));
                assert!(matches!(rhs.kind, ExprKind::Int(2)));
            }
            other => panic!("expected Assign, got {other:?}"),
        }
    }

    /// Regression: nested declarators used to be asserted as `identifier`.
    #[test]
    fn nested_declarators_lower() {
        let unit = lower("int f() { int *p; int a[10]; int **q; }");
        let names: Vec<_> = unit.functions[0]
            .body
            .statements
            .iter()
            .map(|s| match &s.kind {
                StmtKind::Declaration { name, .. } => name.0.clone(),
                other => panic!("expected Declaration, got {other:?}"),
            })
            .collect();
        assert_eq!(names, ["p", "a", "q"]);
    }

    #[test]
    fn pointer_parameters_lower() {
        let unit = lower("int f(int *x, int y) { return y; }");
        let params: Vec<_> = unit.functions[0]
            .params
            .iter()
            .map(|p| p.0.clone())
            .collect();
        assert_eq!(params, ["x", "y"]);
    }

    #[test]
    fn spans_slice_back_to_source() {
        let src = "int f() { return 1 + 2; }";
        let unit = lower(src);
        let ret = &unit.functions[0].body.statements[0];
        assert_eq!(ret.span.slice(src), "return 1 + 2;");
        match &ret.kind {
            StmtKind::Return(Some(e)) => assert_eq!(e.span.slice(src), "1 + 2"),
            other => panic!("expected Return, got {other:?}"),
        }
    }
}
