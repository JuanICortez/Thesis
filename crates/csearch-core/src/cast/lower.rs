//! CST → CAst.
//! Translation only — no semantic rewriting happens here.

use tree_sitter::{Node, Tree};

use super::{
    BinOp, Compound, ExprKind, Expression, Function, Identifier, Item, ItemKind, Statement,
    StmtKind, TranslationUnit,
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
    fn unwrap_declarator(&self, node: Node) -> Option<(Identifier, usize)> {
        match node.kind() {
            "identifier" => Some((self.extract_identifier(node), 0)),
            "pointer_declarator" => {
                let inner = node.child_by_field_name("declarator")?;
                let (name, depth) = self.unwrap_declarator(inner)?;
                Some((name, depth + 1))
            }
            "array_declarator" | "function_declarator" => {
                let inner = node.child_by_field_name("declarator")?;
                self.unwrap_declarator(inner)
            }
            "parenthesized_declarator" => {
                let inner = node.named_child(0)?;
                self.unwrap_declarator(inner)
            }
            _ => None,
        }
    }

    /// Parses an integer literal, honouring the C radix prefixes and the
    /// `u`/`l` suffixes. Returns `None` for anything that is not an integer
    /// this can represent exactly — floats, character constants, overflow —
    /// so the caller can mark it unsupported instead of silently reading it
    /// as `0`.
    fn extract_number(&self, node: Node) -> Option<i64> {
        assert_eq!(node.kind(), "number_literal");
        let text = self.extract_text(node);
        let text = text.trim_end_matches(['u', 'U', 'l', 'L']);

        let (digits, radix) = match text.get(..2) {
            Some("0x") | Some("0X") => (&text[2..], 16),
            Some("0b") | Some("0B") => (&text[2..], 2),
            _ if text.len() > 1 && text.starts_with('0') && !text.contains(['.', 'e', 'E']) => {
                (&text[1..], 8)
            }
            _ => (text, 10),
        };

        i64::from_str_radix(digits, radix).ok()
    }

    fn get_operation(&self, node: Node) -> Option<BinOp> {
        let operator = node.child_by_field_name("operator")?;
        match self.extract_text(operator).as_str() {
            "+" => Some(BinOp::Add),
            "-" => Some(BinOp::Sub),
            "*" => Some(BinOp::Mul),
            "/" => Some(BinOp::Div),
            _ => None,
        }
    }

    // ======================
    // Builders
    // ======================

    pub fn build_translation_unit(&self, tree: &Tree) -> TranslationUnit {
        let root = tree.root_node();
        assert_eq!(root.kind(), "translation_unit");

        let mut items = Vec::new();
        let mut cursor = root.walk();

        for child in root.named_children(&mut cursor) {
            // A `function_definition` this cannot name — an unnamed parameter,
            // a declarator too convoluted to unwrap — becomes `Unsupported`
            // like any other top-level construct. Dropping it would understate
            // coverage silently, which is the failure mode this stage exists
            // to avoid.
            let kind = match child.kind() {
                "function_definition" => match self.build_function(child) {
                    Some(function) => ItemKind::Function(function),
                    None => self.unsupported_item_kind(child),
                },
                _ => self.unsupported_item_kind(child),
            };
            items.push(Item::new(kind, self.span(child)));
        }
        TranslationUnit {
            items,
            span: self.span(root),
        }
    }

    fn unsupported_item_kind(&self, node: Node) -> ItemKind {
        ItemKind::Unsupported {
            kind: node.kind().to_string(),
        }
    }

    fn build_function(&self, node: Node) -> Option<Function> {
        assert_eq!(node.kind(), "function_definition");

        let declarator = node.child_by_field_name("declarator")?;
        let (name, params) = self.deconstruct_declarator(declarator)?;

        let body = node.child_by_field_name("body")?;
        let mut statements = Vec::new();
        let mut cursor = body.walk();

        for child in body.named_children(&mut cursor) {
            statements.extend(self.build_statement(child));
        }

        Some(Function {
            name,
            params,
            body: Compound {
                statements,
                span: self.span(body),
            },
            span: self.span(node),
        })
    }

    /// `None` when the definition has no name we can resolve — `int (*f(void))(int)`
    /// and friends. Unnamed parameters (`void f(int)`) make the whole
    /// parameter list `None` rather than silently changing the arity.
    fn deconstruct_declarator(&self, node: Node) -> Option<(Identifier, Vec<Identifier>)> {
        if node.kind() != "function_declarator" {
            return None;
        }

        let (name, _pointer_depth) =
            self.unwrap_declarator(node.child_by_field_name("declarator")?)?;

        let params = match node.child_by_field_name("parameters") {
            Some(param_list) if self.is_void_parameter_list(param_list) => Vec::new(),
            Some(param_list) => {
                let mut param_identifiers = Vec::new();
                let mut cursor = param_list.walk();

                for child in param_list.named_children(&mut cursor) {
                    if child.kind() == "parameter_declaration" {
                        let param_node = child.child_by_field_name("declarator")?;
                        let (param_identifier, _pointer_depth) =
                            self.unwrap_declarator(param_node)?;
                        param_identifiers.push(param_identifier);
                    }
                }
                param_identifiers
            }
            None => Vec::new(),
        };
        Some((name, params))
    }

    fn is_void_parameter_list(&self, param_list: Node) -> bool {
        let mut cursor = param_list.walk();
        let mut children = param_list.named_children(&mut cursor);

        let Some(only) = children.next() else {
            return false;
        };
        if children.next().is_some() || only.kind() != "parameter_declaration" {
            return false;
        }

        only.child_by_field_name("declarator").is_none()
            && only
                .child_by_field_name("type")
                .is_some_and(|node| self.extract_text(node) == "void")
    }

    fn build_statement(&self, node: Node) -> Vec<Statement> {
        let span = self.span(node);
        match node.kind() {
            "declaration" => self
                .process_declaration(node)
                .into_iter()
                .map(|kind| Statement::new(kind, span))
                .collect(),
            "expression_statement" => vec![self.process_expression_statement(node)],
            "return_statement" => vec![Statement::new(
                StmtKind::Return(node.named_child(0).map(|e| self.build_expression(e))),
                span,
            )],
            kind => vec![Statement::new(
                StmtKind::Unsupported {
                    kind: kind.to_string(),
                },
                span,
            )],
        }
    }

    /// One `StmtKind` per declarator, so `int a, b = 2;` yields two. Splitting
    /// them into separate statements is stage 2's job; this only reports what
    /// the declarator list contains.
    fn process_declaration(&self, node: Node) -> Vec<StmtKind> {
        assert_eq!(node.kind(), "declaration");

        let mut declarations = Vec::new();
        let mut cursor = node.walk();

        let declarators = node.children_by_field_name("declarator", &mut cursor);

        for declarator in declarators {
            declarations.push(self.process_declarator(declarator).unwrap_or_else(|| {
                StmtKind::Unsupported {
                    kind: declarator.kind().to_string(),
                }
            }));
        }
        declarations
    }

    fn process_declarator(&self, declarator: Node) -> Option<StmtKind> {
        match declarator.kind() {
            "init_declarator" => {
                let (name, _pointer_depth) =
                    self.unwrap_declarator(declarator.child_by_field_name("declarator")?)?;
                let value = self.build_expression(declarator.child_by_field_name("value")?);
                Some(StmtKind::Declaration {
                    name,
                    value: Some(value),
                })
            }
            // Everything else is an uninitialized declarator: `int a;`,
            // `int *p;`, `int a[10];`, `int (*f)(void);`
            _ => {
                let (name, _pointer_depth) = self.unwrap_declarator(declarator)?;
                Some(StmtKind::Declaration { name, value: None })
            }
        }
    }

    fn process_expression_statement(&self, node: Node) -> Statement {
        assert_eq!(node.kind(), "expression_statement");
        let span = self.span(node);

        // An empty statement (`;`) has no child.
        let Some(expression_child) = node.named_child(0) else {
            return Statement::new(
                StmtKind::Unsupported {
                    kind: node.kind().to_string(),
                },
                span,
            );
        };

        let kind = self
            .build_expression_statement_kind(expression_child)
            .unwrap_or_else(|| StmtKind::Unsupported {
                kind: expression_child.kind().to_string(),
            });
        Statement::new(kind, span)
    }

    fn build_expression_statement_kind(&self, node: Node) -> Option<StmtKind> {
        match node.kind() {
            "assignment_expression" => {
                let lhs = node.child_by_field_name("left")?;
                let rhs = node.child_by_field_name("right")?;

                // `a += b` is not a plain assignment — modelling it as one
                // would silently drop the operator. Desugaring is stage 2's
                // job, so anything but `=` is unsupported here.
                let operator = node.child_by_field_name("operator")?;
                if self.extract_text(operator) != "=" {
                    return None;
                }

                Some(StmtKind::Assign {
                    lhs: self.build_expression(lhs),
                    rhs: self.build_expression(rhs),
                })
            }
            _ => Some(StmtKind::ExprStmt(self.build_expression(node))),
        }
    }

    fn build_expression(&self, node: Node) -> Expression {
        let span = self.span(node);
        let kind = self
            .build_expression_kind(node)
            .unwrap_or_else(|| self.unsupported_expr_kind(node));
        Expression::new(kind, span)
    }

    /// `None` means "stage 1 cannot model this node", which the caller turns
    /// into [`ExprKind::Unsupported`]. Every `?` in here is a construct we do
    /// not handle yet, not an internal error.
    fn build_expression_kind(&self, node: Node) -> Option<ExprKind> {
        match node.kind() {
            "identifier" => Some(ExprKind::Variable(self.extract_identifier(node))),
            "parenthesized_expression" => Some(self.build_expression(node.named_child(0)?).kind),
            "number_literal" => Some(ExprKind::Int(self.extract_number(node)?)),
            "binary_expression" => {
                let lhs = node.child_by_field_name("left")?;
                let rhs = node.child_by_field_name("right")?;

                Some(ExprKind::BinaryOp {
                    op: self.get_operation(node)?,
                    lhs: Box::new(self.build_expression(lhs)),
                    rhs: Box::new(self.build_expression(rhs)),
                })
            }
            "call_expression" => {
                let callee = node.child_by_field_name("function")?;
                // Calls through a pointer or a member (`f->cb(x)`) are not
                // modelled: the callee is an `Identifier`, not an expression.
                if callee.kind() != "identifier" {
                    return None;
                }
                let args = node
                    .child_by_field_name("arguments")
                    .map_or(Vec::new(), |args_list| {
                        args_list
                            .named_children(&mut args_list.walk())
                            .map(|arg| self.build_expression(arg))
                            .collect()
                    });

                Some(ExprKind::Call {
                    callee: self.extract_identifier(callee),
                    args,
                })
            }
            _ => None,
        }
    }

    fn unsupported_expr_kind(&self, node: Node) -> ExprKind {
        ExprKind::Unsupported {
            kind: node.kind().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast::{collect_unsupported, ExprKind, StmtKind};
    use tree_sitter::Parser;

    fn lower(src: &str) -> TranslationUnit {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .expect("C parser");
        let tree = parser.parse(src, None).expect("parse");
        build_translation_unit(&tree, src)
    }

    fn first_function(unit: &TranslationUnit) -> &Function {
        unit.functions().next().expect("a function definition")
    }

    /// Regression: the C grammar field is `right`, not `rhs`.
    #[test]
    fn plain_assignment_lowers() {
        let unit = lower("int f() { int a; a = 2; }");
        let stmts = &first_function(&unit).body.statements;
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
        let names: Vec<_> = first_function(&unit)
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
        let params: Vec<_> = first_function(&unit)
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
        let ret = &first_function(&unit).body.statements[0];
        assert_eq!(ret.span.slice(src), "return 1 + 2;");
        match &ret.kind {
            StmtKind::Return(Some(e)) => assert_eq!(e.span.slice(src), "1 + 2"),
            other => panic!("expected Return, got {other:?}"),
        }
    }

    // ---- Stage 1 totality (step 2) ----

    #[test]
    fn unmodelled_statements_become_unsupported() {
        let unit = lower("int f() { for (;;) { } goto end; }");
        let kinds: Vec<_> = first_function(&unit)
            .body
            .statements
            .iter()
            .map(|s| match &s.kind {
                StmtKind::Unsupported { kind } => kind.clone(),
                other => panic!("expected Unsupported, got {other:?}"),
            })
            .collect();
        assert_eq!(kinds, ["for_statement", "goto_statement"]);
    }

    #[test]
    fn compound_assignment_is_unsupported_not_mistranslated() {
        let unit = lower("int f() { int a; a += 2; }");
        assert!(matches!(
            &first_function(&unit).body.statements[1].kind,
            StmtKind::Unsupported { kind } if kind == "assignment_expression"
        ));
    }

    #[test]
    fn unmodelled_expressions_become_unsupported() {
        let unit = lower(r#"int f() { return "hi"; }"#);
        match &first_function(&unit).body.statements[0].kind {
            StmtKind::Return(Some(e)) => assert!(matches!(
                &e.kind,
                ExprKind::Unsupported { kind } if kind == "string_literal"
            )),
            other => panic!("expected Return, got {other:?}"),
        }
    }

    #[test]
    fn integer_literals_parse_by_radix() {
        let unit = lower("int f() { int a = 0x1f; int b = 010; int c = 42u; int d = 1.5; }");
        let values: Vec<_> = first_function(&unit)
            .body
            .statements
            .iter()
            .map(|s| match &s.kind {
                StmtKind::Declaration { value: Some(v), .. } => v.kind.clone(),
                other => panic!("expected initialised Declaration, got {other:?}"),
            })
            .collect();
        assert!(matches!(values[0], ExprKind::Int(31)));
        assert!(matches!(values[1], ExprKind::Int(8)));
        assert!(matches!(values[2], ExprKind::Int(42)));
        // A float is not an i64 — better unsupported than silently 0.
        assert!(matches!(&values[3], ExprKind::Unsupported { kind } if kind == "number_literal"));
    }

    #[test]
    fn collect_unsupported_reports_kind_and_span() {
        let src = "int f() { while (1) { } return 1 + \"x\"; }";
        let found = collect_unsupported(&lower(src));
        let reported: Vec<_> = found
            .iter()
            .map(|u| (u.kind.as_str(), u.span.slice(src)))
            .collect();
        assert_eq!(
            reported,
            [
                ("while_statement", "while (1) { }"),
                ("string_literal", "\"x\""),
            ]
        );
    }

    // ---- Top-level items (step 4) ----

    /// Globals, typedefs and preprocessor directives used to be dropped
    /// without a trace, which was the one place stage 1 broke its own
    /// no-silent-drops contract. They are now reported.
    #[test]
    fn top_level_constructs_are_reported_not_dropped() {
        let src = "int g = 1;\ntypedef int myint;\nstruct S { int x; };\nint f(void) { return 1; }";
        let unit = lower(src);

        let kinds: Vec<_> = unit
            .items
            .iter()
            .map(|item| match &item.kind {
                ItemKind::Function(function) => format!("fn {}", function.name.0),
                ItemKind::Unsupported { kind } => kind.clone(),
            })
            .collect();
        assert_eq!(
            kinds,
            ["declaration", "type_definition", "struct_specifier", "fn f"]
        );

        // And the diagnostics walker sees them, with spans that slice back.
        let reported: Vec<_> = collect_unsupported(&unit)
            .iter()
            .map(|u| u.span.slice(src))
            .collect();
        // `struct_specifier` is the declaration's *type*, so its span stops
        // before the `;` the other two include.
        assert_eq!(
            reported,
            ["int g = 1;", "typedef int myint;", "struct S { int x; }"]
        );
    }

    /// A function definition stage 1 cannot name — here, an unnamed parameter —
    /// is an unsupported item rather than a missing one. Lowering it with the
    /// wrong arity would be the worse answer.
    #[test]
    fn unnameable_function_definitions_are_unsupported_items() {
        let unit = lower("void f(int) { }");
        assert_eq!(unit.functions().count(), 0);
        assert!(matches!(
            &unit.items[0].kind,
            ItemKind::Unsupported { kind } if kind == "function_definition"
        ));
    }

    #[test]
    fn functions_skips_unsupported_items_in_source_order() {
        let unit = lower("typedef int a; int f(void) { } typedef int b; int g(void) { }");
        let names: Vec<_> = unit.functions().map(|f| f.name.0.clone()).collect();
        assert_eq!(names, ["f", "g"]);
    }

    /// The stage-1 contract: total, and never panics.
    #[test]
    fn lowering_never_panics_on_awkward_input() {
        let sources = [
            "",
            "int;",
            "void f(int) { }",
            "int (*f(void))(int) { }",
            "struct S { int x; }; int g(struct S *s) { return s->x; }",
            "typedef int myint; myint h(void) { myint v = 1; return v; }",
            "#define M 1\nint i(void) { return M; }",
            "int j(void) { int *p, **q, a[3]; p = &a[0]; return **q; }",
            "int k(void) { switch (1) { case 1: break; default: ; } return 0; }",
            "int l(void) { f->cb(1); (*g)(2); return 0; }",
            "int broken(void) { int a = ; return",
        ];
        for src in sources {
            let unit = lower(src);
            // Exercising the walker too — it must not panic either.
            let _ = collect_unsupported(&unit);
        }
    }
}
