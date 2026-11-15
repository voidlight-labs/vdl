//! Stable schema composition output, including metadata and heading order.
use std::path::Path;
use vdl::{codegen::compose::compose, lexer, parser, validator::schema::validate_schema};

#[test]
fn schema_markdown_snapshot() {
    let source = r#"
schema "guide" {
    type "Page" {
        field "name" text heading
        field "children" ref "Page" compose optional
    }
}
Page "root" { name "Guide" children ["child"] }
Page "child" { name "Introduction" }
"#;
    let path = Path::new("snapshot.vdl");
    let tokens = lexer::lex(source, path).unwrap();
    let doc = parser::parse_document(&tokens, source, path).unwrap();
    validate_schema(&doc).unwrap();
    assert_eq!(compose(&doc, "root").unwrap(),
        "---\nid: \"root\"\ntype: \"Page\"\n\"name\": \"Guide\"\n---\n\n# Guide\n\n## Introduction\n");
}
