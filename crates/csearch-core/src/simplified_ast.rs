use tree_sitter::{Node, Tree};

// Nodes

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier(pub String);

#[derive(Debug, Clone)]
pub struct TranslationUnit {
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: Identifier,
    pub params: Vec<Identifier>,
    pub body: Compound,
}

#[derive(Debug, Clone)]
pub struct Compound {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Statement {
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
pub enum Expression {
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

// Builders

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
        TranslationUnit { functions }
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
            body: Compound { statements },
        }
    }

    fn deconstruct_declarator(&self, node: Node) -> (Identifier, Vec<Identifier>) {
        assert_eq!(node.kind(), "function_declarator");

        let name_node = node
            .child_by_field_name("declarator")
            .expect("Function Identifier");
        let name = self.extract_identifier(name_node);

        let params = if let Some(param_list) = node.child_by_field_name("parameters") {
            let mut param_identifiers = Vec::new();
            let mut cursor = param_list.walk();

            for child in param_list.named_children(&mut cursor) {
                if child.kind() == "parameter_declaration" {
                    let param_node = child
                        .child_by_field_name("declarator")
                        .expect("Parameter Declarator");
                    let param_identifier = self.extract_identifier(param_node);
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
        match node.kind() {
            "declaration" => self
                .process_declaration(node)
                .into_iter()
                .map(|(name, value)| Statement::Declaration { name, value })
                .collect(),
            "expression_statement" => vec![self.process_expression_statement(node)],
            "return_statement" => vec![Statement::Return(
                node.named_child(0).map(|e| self.build_expression(e)),
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
                "identifier" | "pointer_declarator" => {
                    let identifier = self.extract_identifier(declarator);
                    declarations.push((identifier, None));
                }
                "init_declarator" => {
                    let identifier = self.extract_identifier(
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
                _ => panic!("Unknown declarator kind: {}", declarator.kind()),
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
                    .child_by_field_name("rhs")
                    .expect("Right Hand Side expression in Assignment");

                Statement::Assign {
                    lhs: self.build_expression(lhs),
                    rhs: self.build_expression(rhs),
                }
            }
            _ => Statement::ExprStmt(self.build_expression(expression_child)),
        }
    }

    fn build_expression(&self, node: Node) -> Expression {
        match node.kind() {
            "identifier" => Expression::Variable(self.extract_identifier(node)),
            "number_literal" => Expression::Int(self.extract_number(node)),
            "binary_expression" => {
                let lhs = node
                    .child_by_field_name("left")
                    .expect("Left Hand Side in Binary Expression");
                let rhs = node
                    .child_by_field_name("right")
                    .expect("Right Hand Side in Binary Expression");

                Expression::BinaryOp {
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

                Expression::Call {
                    callee: self.extract_identifier(callee),
                    args,
                }
            }
            _ => todo!("Unknown Expression"),
        }
    }
}
