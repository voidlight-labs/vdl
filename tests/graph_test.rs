//! End-to-end tests for the schema-form `graph` and `search` commands over a
//! multi-file project: cross-file references, one-hop filtering, output
//! formats, and determinism.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Helper: run the `vdl` binary via `cargo run -- <args>`.
fn run_vdl<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--")
        .args(args)
        .output()
        .expect("Failed to execute `cargo run` — is the VDL project buildable?")
}

const SCHEMA: &str = r#"
schema "guide" {
    type "Guide" {
        field "label" text heading
        field "sections" ref "Section" compose
    }
    type "Section" {
        field "label" text heading
        field "content" body
        field "next" ref "Section" optional
    }
}
"#;

const GUIDE: &str = r#"
entity "Guide" "main" {
    field "label" "Main Guide"
    field "sections" [ "intro", "details" ]
}
"#;

const INTRO: &str = r#"
entity "Section" "intro" {
    field "label" "Intro"
    field "content" """Welcome text."""
}
"#;

const DETAILS: &str = r#"
entity "Section" "details" {
    field "label" "Details"
    field "content" """Deep details."""
    field "next" [ "intro" ]
}
"#;

const ORPHAN: &str = r#"
entity "Section" "orphan" {
    field "label" "Orphan"
    field "content" """Referenced by nothing."""
}
"#;

/// A small multi-file project: the guide references sections declared in
/// other files, and one section references back.
fn fresh_project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    fs::write(root.join("schema.vdl"), SCHEMA).expect("schema written");
    fs::create_dir_all(root.join("parts")).expect("parts dir");
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(root.join("parts").join("guide.vdl"), GUIDE).expect("guide written");
    fs::write(root.join("sections").join("intro.vdl"), INTRO).expect("intro written");
    fs::write(root.join("sections").join("details.vdl"), DETAILS).expect("details written");
    fs::write(root.join("sections").join("orphan.vdl"), ORPHAN).expect("orphan written");
    (dir, root)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn node_ids(json: &str) -> Vec<&str> {
    json.split("\"id\": \"")
        .skip(1) // skip everything before the first node
        .filter_map(|rest| rest.split('"').next())
        .collect()
}

#[test]
fn graph_merges_cross_file_references() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["graph", root.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = stdout(&output);
    let ids = node_ids(&json);
    // Node order follows the loader's sorted-file order: parts/guide.vdl,
    // then sections/details.vdl, sections/intro.vdl, sections/orphan.vdl.
    assert_eq!(ids, vec!["main", "details", "intro", "orphan"], "{json}");

    // Cross-file edges, in entity then field order.
    let main_idx = json.find("\"nodes\":").expect("nodes section");
    let edges = &json[main_idx..];
    let details_to_intro = edges.find("{\n      \"from\": \"details\",\n      \"to\": \"intro\",").is_some();
    assert!(details_to_intro, "cross-file edge missing: {json}");
}

#[test]
fn graph_output_is_deterministic() {
    let (_dir, root) = fresh_project();
    let first = run_vdl(["graph", root.to_str().unwrap()]);
    let second = run_vdl(["graph", root.to_str().unwrap()]);
    assert_eq!(stdout(&first), stdout(&second));
}

#[test]
fn graph_dot_renders_the_merged_project() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["graph", root.to_str().unwrap(), "--format", "dot"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let dot = stdout(&output);
    assert!(dot.contains("digraph VDL_Graph {"), "{dot}");
    assert!(dot.contains("\"main\" -> \"intro\" [label=\"sections\"]"), "{dot}");
    assert!(dot.contains("\"details\" -> \"intro\" [label=\"next\"]"), "{dot}");
    // Every Section shares one fillcolor; the Guide gets the first color.
    let guide_color = dot.split("\"main\" [").nth(1).and_then(|rest| {
        rest.split("fillcolor=\"").nth(1).and_then(|c| c.split('"').next())
    });
    assert_eq!(guide_color, Some("#4C72B0"), "{dot}");
}

#[test]
fn graph_root_filter_excludes_unrelated_entities() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["graph", root.to_str().unwrap(), "--root", "intro"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = stdout(&output);
    let ids = node_ids(&json);
    // intro is referenced by main and details; orphan stays outside. Order
    // follows the loader's sorted-file order, filtered to the neighborhood.
    assert_eq!(ids, vec!["main", "details", "intro"], "{json}");
    assert!(!json.contains("\"orphan\""), "{json}");
    assert!(json.contains("\"field\": \"sections\""), "{json}");
}

#[test]
fn graph_root_filter_can_isolate_a_leaf() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["graph", root.to_str().unwrap(), "--root", "orphan"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = stdout(&output);
    assert_eq!(node_ids(&json), vec!["orphan"], "{json}");
    assert!(json.contains("\"edges\": []"), "{json}");
}

#[test]
fn graph_index_covers_every_entity_and_supports_root_filter() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["graph", root.to_str().unwrap(), "--format", "index"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let index = stdout(&output);
    for id in ["main", "intro", "details", "orphan"] {
        assert!(index.contains(&format!("\"id\": \"{id}\"")), "{index}");
    }
    assert!(index.contains("Deep details."), "{index}");
    // Reference values stay out of the content.
    assert!(
        !index.contains("\"content\": \"main\\nGuide\\nMain Guide\\\""),
        "{index}"
    );

    let filtered = run_vdl([
        "graph",
        root.to_str().unwrap(),
        "--format",
        "index",
        "--root",
        "orphan",
    ]);
    let filtered_index = stdout(&filtered);
    assert!(filtered.status.success(), "stderr: {}", stderr(&filtered));
    assert!(filtered_index.contains("\"id\": \"orphan\""), "{filtered_index}");
    assert!(!filtered_index.contains("\"id\": \"main\""), "{filtered_index}");
}

#[test]
fn search_finds_content_across_files() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["search", "deep", root.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "details | Section | Details\n");

    let nothing = run_vdl(["search", "no-such-text", root.to_str().unwrap()]);
    assert!(nothing.status.success());
    assert_eq!(
        stdout(&nothing),
        "No entities matching 'no-such-text'.\n"
    );
}

#[test]
fn graph_output_file_refuses_project_sources() {
    let (_dir, root) = fresh_project();
    let input = root.join("schema.vdl");
    let before = fs::read(&input).expect("input readable");

    let output = run_vdl([
        "graph",
        root.to_str().unwrap(),
        "--out",
        input.to_str().unwrap(),
    ]);
    let err = stderr(&output);
    assert!(!output.status.success(), "stdout: {}", stdout(&output));
    assert!(err.contains("refusing to write output over the input"), "{err}");
    assert_eq!(fs::read(&input).expect("input still readable"), before);
}

#[test]
fn graph_output_file_writes_outside_project_sources() {
    let (_dir, root) = fresh_project();
    fs::create_dir_all(root.join("dist")).expect("dist dir created");
    let out = root.join("dist").join("graph.dot");

    let output = run_vdl([
        "graph",
        root.to_str().unwrap(),
        "--format",
        "dot",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let content = fs::read_to_string(&out).expect("dot file written");
    assert!(content.contains("digraph VDL_Graph {"), "{}", content);
}

#[test]
fn graph_rejects_invalid_projects() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    fs::write(
        root.join("broken.vdl"),
        "schema \"x\" { type \"T\" { field \"name\" text heading } }\nentity \"T\" \"a\" {}\nentity \"T\" \"a\" {}\n",
    )
    .expect("fixture written");

    let output = run_vdl(["graph", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("duplicate"), "stderr: {}", err);
}
