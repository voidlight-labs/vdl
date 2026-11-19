//! End-to-end tests for `vdl rename`: reference rewrites across files,
//! dry-run safety, and validation of the rewritten project.

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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    // Collapse whitespace so assertions survive miette's terminal-width
    // line wrapping, which differs between terminals and CI.
    String::from_utf8_lossy(&output.stderr)
        .replace('│', " ") // miette's box-drawing line prefix
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const SCHEMA: &str = r#"
schema "guide" {
    type "Guide" {
        field "label" text heading
        field "sections" ref "Section" compose
    }
    type "Section" {
        field "label" text heading
        field "content" body compose ["Section"] optional
        field "next" ref "Section" optional
        field "flow" ref "Section" inline optional
    }
}
"#;

const INTRO: &str = r#"package guide.sections

entity "Section" "guide.sections.intro" {
    field "label" "Intro"
    field "content" {
        Intro prose.

        include guide.sections.detail
    }
    field "flow" [
        detail,
        prose {
            Keep going.
        },
        "guide.sections.intro"
    ]
}
"#;

const DETAIL: &str = r#"package guide.sections

entity "Section" "guide.sections.detail" {
    field "label" "Detail"
    field "content" """Detail body."""
}

entity "Section" "guide.sections.final" {
    field "label" "Final"
    field "content" {
        Final prose.

        include guide.sections.intro
    }
    field "next" [ intro ]
}
"#;

const OLD: &str = "guide.sections.intro";
const NEW: &str = "guide.sections.introduction";

fn fresh_project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    fs::write(root.join("schema.vdl"), SCHEMA).expect("schema written");
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(root.join("sections").join("intro.vdl"), INTRO).expect("intro written");
    fs::write(root.join("sections").join("detail.vdl"), DETAIL).expect("detail written");
    (dir, root)
}

fn read(root: &Path, relative: &str) -> String {
    fs::read_to_string(root.join(relative)).expect("source readable")
}

#[test]
fn rename_dry_run_reports_without_writing() {
    let (_dir, root) = fresh_project();
    let before_intro = read(&root, "sections/intro.vdl");
    let before_detail = read(&root, "sections/detail.vdl");

    let output = run_vdl(["rename", OLD, NEW, root.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(out.contains("Dry run"), "stdout: {}", out);
    // Two edits live in intro.vdl (declaration + inline self-reference);
    // the bare reference is in detail.vdl.
    assert!(out.contains("intro.vdl"), "stdout: {}", out);
    assert!(out.contains("detail.vdl"), "stdout: {}", out);
    assert_eq!(fs::read(root.join("sections/intro.vdl")).unwrap(), before_intro.as_bytes());
    assert_eq!(fs::read(root.join("sections/detail.vdl")).unwrap(), before_detail.as_bytes());
}

#[test]
fn rename_writes_every_reference_form_and_stays_valid() {
    let (_dir, root) = fresh_project();

    let output = run_vdl(["rename", OLD, NEW, root.to_str().unwrap(), "--write"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(out.contains("Renamed"), "stdout: {}", out);

    let intro = read(&root, "sections/intro.vdl");
    let detail = read(&root, "sections/detail.vdl");
    // Declaration becomes the quoted new ID.
    assert!(intro.contains("entity \"Section\" \"guide.sections.introduction\""), "{intro}");
    // The sibling include directive is untouched.
    assert!(intro.contains("include guide.sections.detail"), "{intro}");
    // The old ID is gone in its exact spellings (the new ID shares the old
    // one as a prefix, so raw substring checks would lie).
    assert!(!intro.contains("\"guide.sections.intro\""), "old ID must be gone: {intro}");
    // Inline self-reference rewritten to the quoted new ID.
    assert!(intro.contains("\"guide.sections.introduction\""), "{intro}");
    // The include of the renamed entity is rewritten; the bare reference
    // keeps its bare spelling with the new final segment.
    assert!(detail.contains("include guide.sections.introduction"), "{detail}");
    assert!(detail.contains("field \"next\" [ introduction ]"), "{detail}");
    assert!(!detail.contains("include guide.sections.intro\n"), "{detail}");
    assert!(!detail.contains("[ intro ]"), "{detail}");

    // The rewritten project still validates and the new ID resolves.
    let validate = run_vdl(["validate", root.to_str().unwrap()]);
    assert!(validate.status.success(), "{}", stderr(&validate));
}

#[test]
fn rename_rejects_an_unknown_old_id() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["rename", "guide.sections.nope", NEW, root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("entity not found"), "stderr: {}", err);
}

#[test]
fn rename_rejects_a_taken_new_id() {
    let (_dir, root) = fresh_project();
    let output = run_vdl(["rename", OLD, "guide.sections.detail", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("already exists"), "stderr: {}", err);
}

#[test]
fn rename_rejects_a_breaking_rewrite() {
    let (_dir, root) = fresh_project();
    // Renaming to the same ID is rejected before any file is read.
    let output = run_vdl(["rename", OLD, OLD, root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("identical"), "stderr: {}", err);
}

#[test]
fn rename_refuses_bare_references_that_would_lose_their_target() {
    let (_dir, root) = fresh_project();
    // The bare `intro` reference in detail.vdl resolves through the local
    // package; a rename into another namespace cannot keep that spelling.
    let output = run_vdl([
        "rename",
        OLD,
        "elsewhere.intro",
        root.to_str().unwrap(),
        "--write",
    ]);
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("bare reference"), "stderr: {}", err);
    // Nothing was written: the dry-run contract holds even under --write.
    let before = read(&root, "sections/intro.vdl");
    assert!(before.contains(OLD), "source must be untouched: {before}");
}
