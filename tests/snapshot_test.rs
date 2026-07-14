use std::path::Path;
use vdl::{
    codegen::{dot, json, markdown, search},
    graph::KnowledgeGraph,
    lexer, parser, validator,
};

/// Helper: parse multiple VDL files, merge into one module, validate, and build graph.
fn compile_files(paths: &[&str]) -> KnowledgeGraph {
    let mut merged = parser::ast::Module::default();
    for path_str in paths {
        let source = std::fs::read_to_string(path_str).unwrap();
        let file = Path::new(path_str);
        let tokens = lexer::lex(&source, file).unwrap();
        let module = parser::parse(&tokens, &source, file).unwrap();
        merged.entities.extend(module.entities);
    }
    validator::validate(&merged).unwrap();
    KnowledgeGraph::from_module(&merged).unwrap()
}

/// End-to-end snapshot test: compile the seven_laws fixture and verify JSON output.
#[test]
fn snapshot_json_output() {
    let source = std::fs::read_to_string("tests/fixtures/seven_laws.vdl").unwrap();
    let file = Path::new("tests/fixtures/seven_laws.vdl");

    let tokens = lexer::lex(&source, file).unwrap();
    let module = parser::parse(&tokens, &source, file).unwrap();
    validator::validate(&module).unwrap();
    let graph = KnowledgeGraph::from_module(&module).unwrap();

    let tmpdir = tempfile::tempdir().unwrap();
    let path = tmpdir.path().join("graph.json");
    std::env::set_var("VDL_TEST_TIMESTAMP", "2026-07-13T23:54:46Z");
    json::generate(&graph, &path).unwrap();
    std::env::remove_var("VDL_TEST_TIMESTAMP");

    let content = std::fs::read_to_string(&path).unwrap();
    // Snapshot the JSON structure (insta will create/update .snap files)
    insta::with_settings!({
        snapshot_path => "tests/snapshot",
        snapshot_suffix => "json_graph",
    }, {
        insta::assert_snapshot!(content);
    });
}

/// End-to-end snapshot test: compile both fixtures and verify search index.
#[test]
fn snapshot_search_output() {
    let graph = compile_files(&[
        "tests/fixtures/seven_laws.vdl",
        "tests/fixtures/divine_alignment.vdl",
    ]);

    let tmpdir = tempfile::tempdir().unwrap();
    let path = tmpdir.path().join("search.json");
    search::generate(&graph, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    insta::with_settings!({
        snapshot_path => "tests/snapshot",
        snapshot_suffix => "search_index",
    }, {
        insta::assert_snapshot!(content);
    });
}

/// End-to-end snapshot test: compile and verify DOT output.
#[test]
fn snapshot_dot_output() {
    let source = std::fs::read_to_string("tests/fixtures/seven_laws.vdl").unwrap();
    let file = Path::new("tests/fixtures/seven_laws.vdl");

    let tokens = lexer::lex(&source, file).unwrap();
    let module = parser::parse(&tokens, &source, file).unwrap();
    validator::validate(&module).unwrap();
    let graph = KnowledgeGraph::from_module(&module).unwrap();

    let tmpdir = tempfile::tempdir().unwrap();
    let path = tmpdir.path().join("graph.dot");
    dot::generate(&graph, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    insta::with_settings!({
        snapshot_path => "tests/snapshot",
        snapshot_suffix => "dot_graph",
    }, {
        insta::assert_snapshot!(content);
    });
}

/// End-to-end snapshot test: compile both fixtures and verify Markdown output.
#[test]
fn snapshot_markdown_output() {
    let graph = compile_files(&[
        "tests/fixtures/seven_laws.vdl",
        "tests/fixtures/divine_alignment.vdl",
    ]);

    let tmpdir = tempfile::tempdir().unwrap();
    let out_dir = tmpdir.path().join("soul");
    markdown::generate(&graph, &out_dir).unwrap();

    // Snapshot the divine_alignment markdown file
    let md_path = out_dir.join("divine-alignment.md");
    let content = std::fs::read_to_string(&md_path).unwrap();
    insta::with_settings!({
        snapshot_path => "tests/snapshot",
        snapshot_suffix => "markdown",
    }, {
        insta::assert_snapshot!(content);
    });
}
