// C source → CAst translation
use std::fs::read_to_string;
use std::path::Path;
use tree_sitter::{Parser, Tree};
use tree_sitter_c::LANGUAGE as CLanguage;

pub fn parse_c<P: AsRef<Path>>(file_path: P) -> Result<Tree, Box<dyn std::error::Error>> {
    let source_code = read_to_string(file_path)?;

    let mut parser = Parser::new();
    parser
        .set_language(&CLanguage.into())
        .expect("Error loading C parser");

    let tree = parser
        .parse(&source_code, None)
        .ok_or("Failed to parse the C source code")?;

    Ok(tree)
}
