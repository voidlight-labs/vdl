//! End-to-end tests for the schema document form.
//!
//! These tests exercise the public API the way an author would: parse a `.vdl`
//! file, validate it against its own schema, then compose a root entity into
//! Markdown.

use std::path::{Path, PathBuf};
use vdl::codegen::compose::compose;
use vdl::error::VdlError;
use vdl::model::SchemaDoc;
use vdl::parser::Document;
use vdl::validator::schema::collect_schema_errors;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema")
}

fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("fixture should be readable")
}

fn parse(src: &str, file: &Path) -> SchemaDoc {
    let tokens = vdl::lexer::lex(src, file).expect("lexing should succeed");
    match vdl::parse_document(&tokens, src, file).expect("parsing should succeed") {
        Document::Schema(doc) => doc,
        Document::Legacy(_) => panic!("expected a schema document"),
    }
}

fn load(name: &str) -> SchemaDoc {
    let file = fixture(name);
    let src = read(name);
    parse(&src, &file)
}

fn diag(doc: &SchemaDoc) -> Vec<String> {
    collect_schema_errors(doc)
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[test]
fn acceptance_document_validates() {
    let doc = load("runbooks.vdl");
    assert!(diag(&doc).is_empty(), "{:?}", diag(&doc));
}

#[test]
fn both_runbooks_compose_to_complete_markdown() {
    let doc = load("runbooks.vdl");

    for (root, label) in [("core-guide", "Core Guide"), ("ops-guide", "Ops Guide")] {
        let md = compose(&doc, root).unwrap_or_else(|e| panic!("{}: {:?}", root, e));
        assert!(md.starts_with("---\n"), "{}", md);
        assert!(md.contains(&format!("id: \"{}\"", root)), "{}", md);
        assert!(md.contains("type: \"Runbook\""), "{}", md);
        assert!(md.contains(&format!("\n# {}\n", label)), "{}", md);
        assert!(md.contains("## Introduction"), "{}", md);
        assert!(md.contains("## Rules"), "{}", md);
        assert!(md.ends_with('\n'), "{}", md);
    }
}

#[test]
fn shared_section_body_is_identical_in_both_outputs() {
    let doc = load("runbooks.vdl");
    let core = compose(&doc, "core-guide").unwrap();
    let ops = compose(&doc, "ops-guide").unwrap();

    let body = "Rule one: keep the body verbatim.\n\nRule two: never reorder authored lists.";
    assert!(core.contains(body));
    assert!(ops.contains(body));
}

#[test]
fn reordering_changes_rendered_order() {
    let doc = load("runbooks.vdl");
    let core = compose(&doc, "core-guide").unwrap();
    let ops = compose(&doc, "ops-guide").unwrap();

    assert!(core.find("## Introduction").unwrap() < core.find("## Rules").unwrap());
    assert!(ops.find("## Rules").unwrap() < ops.find("## Introduction").unwrap());
}

#[test]
fn ordinary_reference_excludes_target_body() {
    let doc = load("runbooks.vdl");
    let core = compose(&doc, "core-guide").unwrap();

    assert!(core.contains("  - \"appendix\""), "{}", core);
    assert!(
        !core.contains("Appendix body. This text must never be inlined by an ordinary reference."),
        "{}",
        core
    );
}

#[test]
fn body_fidelity_after_documented_transform() {
    let doc = load("runbooks.vdl");
    let md = compose(&doc, "core-guide").unwrap();

    // Source body is indented four spaces, CRLF-free, with a blank line right
    // after the opening delimiter and a trailing whitespace-only line before
    // the closing one. Rules 1-5 produce exactly this.
    let expected = "Prose starts here without a heading line.\n\n### Internal heading stays as written\n\nText with **bold**, a “curly quote”, and an — em dash.";
    assert!(md.contains(expected), "composed document:\n{}", md);
}

#[test]
fn crlf_source_renders_lf_body() {
    let src = read("runbooks.vdl");
    let crlf = src.replace('\n', "\r\n");
    let file = fixture("runbooks.vdl");
    let lf_doc = parse(&src, &file);
    let crlf_doc = parse(&crlf, &file);

    let lf = compose(&lf_doc, "core-guide").unwrap();
    let w = compose(&crlf_doc, "core-guide").unwrap();
    assert_eq!(lf, w);
    assert!(!w.contains('\r'));
}

// ---------------------------------------------------------------------------
// Negative cases
// ---------------------------------------------------------------------------

#[test]
fn unresolved_reference_is_rejected() {
    let doc = load("invalid_unresolved.vdl");
    let msgs = diag(&doc);
    assert!(
        msgs.iter().any(|m| m.contains("unresolved entity 'missing'")),
        "{:?}",
        msgs
    );
}

#[test]
fn wrong_target_type_is_rejected() {
    let doc = load("invalid_wrong_type.vdl");
    let msgs = diag(&doc);
    assert!(
        msgs.iter()
            .any(|m| m.contains("expects type 'Chapter'") && m.contains("'other'")),
        "{:?}",
        msgs
    );
}

#[test]
fn undeclared_target_type_rejected_even_with_empty_list() {
    let doc = load("invalid_undeclared_target.vdl");
    let msgs = diag(&doc);
    assert!(
        msgs.iter().any(|m| m.contains("undeclared type 'Chaptre'")),
        "{:?}",
        msgs
    );
}

#[test]
fn duplicate_field_value_rejected() {
    let doc = load("invalid_duplicate_value.vdl");
    let msgs = diag(&doc);
    assert!(
        msgs.iter()
            .any(|m| m.contains("supplies field 'label' 2 times")),
        "{:?}",
        msgs
    );
}

// ---------------------------------------------------------------------------
// Decisive check: no domain vocabulary in the compiler
// ---------------------------------------------------------------------------

#[test]
fn renaming_types_needs_no_rust_change() {
    let src = read("runbooks.vdl")
        .replace("Runbook", "Manual")
        .replace("Chapter", "Sop")
        .replace("chapters", "procedures")
        .replace("label", "caption")
        .replace("content", "prose");

    let file = Path::new("renamed.vdl");
    let doc = parse(&src, file);
    assert!(diag(&doc).is_empty(), "{:?}", diag(&doc));

    let md = compose(&doc, "core-guide").unwrap();
    assert!(md.contains("type: \"Manual\""), "{}", md);
    assert!(md.contains("\n# Core Guide\n"), "{}", md);
    assert!(md.contains("## Introduction"), "{}", md);
    assert!(md.contains("caption: \"Core Guide\""), "{}", md);
    assert!(md.contains("### Internal heading stays as written"), "{}", md);
}

/// Same check at the grammar level: type and field names are free-form strings,
/// so a completely synthetic vocabulary works with zero compiler changes.
#[test]
fn synthetic_vocabulary_validates() {
    let src = r#"
schema "anything" {
    type "Widget" {
        field "name" text heading
        field "slots" ref "Slot" compose
    }
    type "Slot" {
        field "name" text heading
        field "markup" body
    }
}
entity "Slot" "s1" {
    field "name" "Slot One"
    field "markup" """Slot body."""
}
entity "Widget" "w1" {
    field "name" "Widget One"
    field "slots" [ "s1" ]
}
"#;
    let doc = parse(src, Path::new("synthetic.vdl"));
    assert!(diag(&doc).is_empty(), "{:?}", diag(&doc));
    let md = compose(&doc, "w1").unwrap();
    assert!(md.contains("# Widget One"));
    assert!(md.contains("## Slot One"));
}

// ---------------------------------------------------------------------------
// Dispatch and legacy regression
// ---------------------------------------------------------------------------

#[test]
fn legacy_fixture_still_parses_as_legacy() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/seven_laws.vdl");
    let src = std::fs::read_to_string(&path).expect("legacy fixture readable");
    let tokens = vdl::lexer::lex(&src, &path).expect("legacy fixture lexes");
    match vdl::parse_document(&tokens, &src, &path).expect("legacy fixture parses") {
        Document::Legacy(module) => assert!(!module.entities.is_empty()),
        Document::Schema(_) => panic!("legacy fixture must not be parsed as a schema document"),
    }
}

#[test]
fn entity_first_schema_file_is_rejected() {
    let src = r#"entity "A" "a" { field "x" "1" }"#;
    let file = Path::new("entity_first.vdl");
    let tokens = vdl::lexer::lex(src, file).unwrap();
    match vdl::parse_document(&tokens, src, file) {
        Err(VdlError::Parser { message, .. }) => {
            assert!(message.contains("exactly one schema block"), "{}", message)
        }
        other => panic!("expected parser error, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// UTF-8 in ordinary quoted strings
// ---------------------------------------------------------------------------

#[test]
fn non_ascii_title_reaches_heading_and_frontmatter() {
    let src = r#"
schema "naskah" {
    type "Naskah" {
        field "label" text heading
        field "bab" ref "Bab" compose
    }
    type "Bab" {
        field "label" text heading
        field "content" body
    }
}

entity "Bab" "satu" {
    field "label" "Bab Satu — “Pembuka”"
    field "content" """Isi bab."""
}

entity "Naskah" "utama" {
    field "label" "Naskah — “Kutip” ✓"
    field "bab" [ "satu" ]
}
"#;
    let doc = parse(src, Path::new("naskah.vdl"));
    assert!(diag(&doc).is_empty(), "{:?}", diag(&doc));

    let md = compose(&doc, "utama").unwrap();
    assert!(md.contains("# Naskah — “Kutip” ✓"), "{}", md);
    assert!(
        md.contains("label: \"Naskah — “Kutip” ✓\""),
        "{}",
        md
    );
    assert!(md.contains("## Bab Satu — “Pembuka”"), "{}", md);
}
