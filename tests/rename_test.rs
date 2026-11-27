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

fn copy_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    copy_tree(&fixture, &root);
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
    assert_eq!(
        fs::read(root.join("sections/intro.vdl")).unwrap(),
        before_intro.as_bytes()
    );
    assert_eq!(
        fs::read(root.join("sections/detail.vdl")).unwrap(),
        before_detail.as_bytes()
    );
}

#[test]
fn rename_writes_every_reference_form_and_stays_valid() {
    let (_dir, root) = fresh_project();

    let output = run_vdl(["rename", OLD, NEW, root.to_str().unwrap(), "--write"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(out.contains("Renamed"), "stdout: {}", out);

    let intro = read(&root, "sections/introduction.vdl");
    let detail = read(&root, "sections/detail.vdl");
    // The package-local declaration remains local and is no longer quoted.
    assert!(intro.contains("entity \"Section\" introduction"), "{intro}");
    // The sibling include directive is untouched.
    assert!(intro.contains("include guide.sections.detail"), "{intro}");
    // The old ID is gone in its exact spellings (the new ID shares the old
    // one as a prefix, so raw substring checks would lie).
    assert!(
        !intro.contains("\"guide.sections.intro\""),
        "old ID must be gone: {intro}"
    );
    // Inline self-reference rewritten to the quoted new ID.
    assert!(intro.contains("\"guide.sections.introduction\""), "{intro}");
    // The include of the renamed entity is rewritten; the bare reference
    // keeps its bare spelling with the new final segment.
    assert!(
        detail.contains("include guide.sections.introduction"),
        "{detail}"
    );
    assert!(
        detail.contains("field \"next\" [ introduction ]"),
        "{detail}"
    );
    assert!(
        !detail.contains("include guide.sections.intro\n"),
        "{detail}"
    );
    assert!(!detail.contains("[ intro ]"), "{detail}");

    // The rewritten project still validates and the new ID resolves.
    let validate = run_vdl(["validate", root.to_str().unwrap()]);
    assert!(validate.status.success(), "{}", stderr(&validate));
    let compose = run_vdl([
        "compose",
        root.to_str().unwrap(),
        "--root",
        "guide.sections.introduction",
    ]);
    assert!(compose.status.success(), "{}", stderr(&compose));
    assert!(
        stdout(&compose).contains("Detail body."),
        "{}",
        stdout(&compose)
    );
}

#[test]
fn rename_v2_demo_preserves_forms_moves_paths_and_keeps_entity_count() {
    let (_dir, root) = copy_fixture("rename-v2");
    let old = "guide.sections.child-a";
    let new = "guide.sections.child-z";
    let before_count = 5;
    let dry = run_vdl(["rename", old, new, root.to_str().unwrap()]);
    let report = stdout(&dry);
    assert!(dry.status.success(), "{}", stderr(&dry));
    assert!(report.contains("Reference: child-a -> child-z"), "{report}");
    assert!(
        report.contains("Reference: \"guide.sections.child-a\" -> \"guide.sections.child-z\""),
        "{report}"
    );
    assert!(
        report.contains("guide.sections.child-b: 2 -> 1"),
        "{report}"
    );
    assert!(report.contains("child-a.vdl ->"), "{report}");
    assert!(
        root.join("children/child-a/child-a.vdl").exists(),
        "dry-run must not move paths"
    );

    let write = run_vdl(["rename", old, new, root.to_str().unwrap(), "--write"]);
    assert!(write.status.success(), "{}", stderr(&write));
    assert!(!root.join("children/child-a").exists());
    assert!(root.join("children/child-z/child-z.vdl").exists());
    assert!(
        root.join("unrelated/child-a.vdl").exists(),
        "same-named unrelated source must not move"
    );
    let target = read(&root, "children/child-z/child-z.vdl");
    assert!(target.contains("entity \"Page\" child-z"), "{target}");
    let sibling = read(&root, "children/child-b/child-b.vdl");
    assert!(sibling.contains("[ child-z ]"), "{sibling}");
    assert!(
        sibling.contains("        }\n        include guide.sections.child-z"),
        "{sibling}"
    );
    assert!(
        sibling.contains("include guide.sections.child-z\n        include \"guide.sections.child-z\"\n        include child-z"),
        "{sibling}"
    );
    assert_eq!(
        sibling
            .lines()
            .filter(|line| line.contains("include guide.sections.child-a"))
            .count(),
        2,
        "fenced and indented examples stay literal: {sibling}"
    );
    let external = read(&root, "external/reader.vdl");
    assert!(
        external.contains("\"guide.sections.child-z\", guide.sections.child-z"),
        "{external}"
    );
    let validate = run_vdl(["validate", root.to_str().unwrap()]);
    assert!(validate.status.success(), "{}", stderr(&validate));
    let graph = run_vdl(["stats", root.to_str().unwrap(), "--json"]);
    assert!(graph.status.success(), "{}", stderr(&graph));
    assert!(
        stdout(&graph).contains(&format!("\"entities\": {before_count}")),
        "{}",
        stdout(&graph)
    );
}

#[test]
fn rename_rejects_case_folded_entity_collision() {
    let (_dir, root) = copy_fixture("rename-v2");
    fs::write(root.join("collision.vdl"), "package guide.sections\nentity \"Page\" \"guide.sections.Child-Z\" { field \"label\" \"collision\" }\n").unwrap();
    let output = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.child-z",
        root.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("already exists"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn rename_rejects_non_local_declaration_and_stale_explicit_import() {
    let (_dir, root) = copy_fixture("rename-v2");
    let cross_package = run_vdl([
        "rename",
        "guide.sections.child-a",
        "other.child-z",
        root.to_str().unwrap(),
    ]);
    assert!(!cross_package.status.success());
    assert!(stderr(&cross_package).contains("package-local declaration"));
    let invalid_bare = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.-new",
        root.to_str().unwrap(),
    ]);
    assert!(!invalid_bare.status.success());
    assert!(stderr(&invalid_bare).contains("not a valid bare name"));
    let trailing_hyphen = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.new-",
        root.to_str().unwrap(),
    ]);
    assert!(!trailing_hyphen.status.success());
    assert!(stderr(&trailing_hyphen).contains("not a valid bare name"));

    fs::write(
        root.join("external/alias.vdl"),
        r#"
package external
import guide.sections.child-a as target
entity "Page" "external.alias-reader" {
    field "label" "Alias reader"
    field "related" [ target ]
}
"#,
    )
    .unwrap();
    let imported = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.child-z",
        root.to_str().unwrap(),
    ]);
    assert!(!imported.status.success());
    assert!(stderr(&imported).contains("explicit import"));

    let (_other_dir, import_only_root) = copy_fixture("rename-v2");
    fs::write(
        import_only_root.join("import-only.vdl"),
        "import guide.sections.child-a as unused\n",
    )
    .unwrap();
    let import_only = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.child-z",
        import_only_root.to_str().unwrap(),
    ]);
    assert!(!import_only.status.success());
    assert!(stderr(&import_only).contains("explicit import"));
}

#[test]
fn rename_rejects_bare_reference_retargeting_through_wildcard_imports() {
    let (_dir, root) = copy_fixture("rename-v2");
    fs::write(
        root.join("external/alias.vdl"),
        r#"
package alt
entity "Page" "alt.child-z" { field "label" "Alternative" }
"#,
    )
    .unwrap();
    fs::write(
        root.join("external/reader-extra.vdl"),
        r#"
package external
import alt.*
import guide.sections.*
entity "Page" "external.reader-extra" {
    field "label" "Wildcard reader"
    field "related" [ child-z ]
}
"#,
    )
    .unwrap();
    let output = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.child-z",
        root.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("preserving bare reference"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn rename_rejects_include_retargeting_and_case_folded_folder_collision() {
    let (_dir, root) = copy_fixture("rename-v2");
    fs::create_dir_all(root.join("alternate")).unwrap();
    fs::write(
        root.join("alternate/child-z.vdl"),
        r#"
package alt
entity "Page" "alt.child-z" {
    field "label" "Alternate"
    field "content" {
        Alternate content.
    }
}
"#,
    )
    .unwrap();
    let child_b = root.join("children/child-b/child-b.vdl");
    let source = fs::read_to_string(&child_b).unwrap().replace(
        "package guide.sections",
        "package guide.sections\nimport alt.*",
    );
    fs::write(
        &child_b,
        source.replace(
            "include child-a",
            "include child-a\n        include child-z",
        ),
    )
    .unwrap();
    let include = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.child-z",
        root.to_str().unwrap(),
    ]);
    assert!(!include.status.success());
    assert!(
        stderr(&include).contains("preserving include"),
        "{}",
        stderr(&include)
    );

    let (_dir2, collision_root) = copy_fixture("rename-v2");
    fs::create_dir_all(collision_root.join("children/CHILD-Z")).unwrap();
    fs::write(collision_root.join("children/CHILD-Z/keep.txt"), "keep").unwrap();
    let collision = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.child-z",
        collision_root.to_str().unwrap(),
    ]);
    assert!(!collision.status.success());
    let err = stderr(&collision);
    assert!(
        err.contains("collides case-insensitively") || err.contains("destination already exists"),
        "{err}"
    );

    let nested_dir = tempfile::tempdir().unwrap();
    let nested_root = nested_dir.path();
    fs::create_dir_all(nested_root.join("old/NEW")).unwrap();
    fs::create_dir_all(nested_root.join("old/old")).unwrap();
    fs::write(nested_root.join("old/NEW/keep.txt"), "keep").unwrap();
    fs::write(
        nested_root.join("schema.vdl"),
        "schema \"s\" { type \"Page\" { field \"label\" text heading } }\n",
    )
    .unwrap();
    fs::write(
        nested_root.join("old/old/old.vdl"),
        "package p\nentity \"Page\" \"p.old\" { field \"label\" \"Old\" }\n",
    )
    .unwrap();
    let nested_collision = run_vdl(["rename", "p.old", "p.new", nested_root.to_str().unwrap()]);
    assert!(
        !nested_collision.status.success(),
        "{}",
        stdout(&nested_collision)
    );
    assert!(
        stderr(&nested_collision).contains("case-insensitively"),
        "{}",
        stderr(&nested_collision)
    );
}

#[test]
fn rename_single_file_invocation_reloads_its_new_path() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.vdl");
    fs::write(
        &source,
        r#"
schema "solo" {
    type "Page" {
        field "label" text heading
    }
}

entity "Page" "old" {
    field "label" "Old"
}
"#,
    )
    .unwrap();
    let output = run_vdl(["rename", "old", "new", source.to_str().unwrap(), "--write"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!source.exists());
    assert!(dir.path().join("new.vdl").exists());
}

#[test]
fn rename_preserves_dotted_ids_without_a_package_header() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("schema.vdl");
    fs::write(
        &source,
        r#"
schema "solo" {
    type "Page" {
        field "label" text heading
    }
}
entity "Page" "old.id" {
    field "label" "Dotted"
}
"#,
    )
    .unwrap();
    let output = run_vdl([
        "rename",
        "old.id",
        "new.id",
        source.to_str().unwrap(),
        "--write",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let rewritten = read(dir.path(), "schema.vdl");
    assert!(rewritten.contains("entity \"Page\" new.id"), "{rewritten}");
    let validate = run_vdl(["validate", source.to_str().unwrap()]);
    assert!(validate.status.success(), "{}", stderr(&validate));
}

#[test]
fn rename_moves_a_container_sibling_folder_with_its_source_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("schema.vdl"),
        r#"
schema "s" {
    type "Page" {
        field "label" text heading
        field "children" ref "Page" optional
    }
}

"#,
    )
    .unwrap();
    fs::write(
        root.join("container.vdl"),
        r#"
package p
entity "Page" "p.container" {
    field "label" "Container"
    field "children" [ child ]
}
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("container")).unwrap();
    fs::write(
        root.join("container/child.vdl"),
        r#"
package p
entity "Page" child {
    field "label" "Child"
}
"#,
    )
    .unwrap();
    let output = run_vdl([
        "rename",
        "p.container",
        "p.vessel",
        root.to_str().unwrap(),
        "--write",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(root.join("vessel.vdl").exists());
    assert!(root.join("vessel/child.vdl").exists());
    assert!(!root.join("container").exists());
    let validate = run_vdl(["validate", root.to_str().unwrap()]);
    assert!(validate.status.success(), "{}", stderr(&validate));
    let stats = run_vdl(["stats", root.to_str().unwrap(), "--json"]);
    assert!(stdout(&stats).contains("\"entities\": 2"));
}

#[test]
fn rename_maps_sibling_container_through_matching_ancestor() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("schema.vdl"),
        "schema \"s\" { type \"Page\" { field \"label\" text heading } }\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("old/old")).unwrap();
    fs::write(
        root.join("old/old.vdl"),
        "package p\nentity \"Page\" \"p.old\" { field \"label\" \"Old\" }\n",
    )
    .unwrap();
    fs::write(
        root.join("old/old/child.vdl"),
        "package p\nentity \"Page\" \"p.child\" { field \"label\" \"Child\" }\n",
    )
    .unwrap();

    let dry = run_vdl(["rename", "p.old", "p.new", root.to_str().unwrap()]);
    assert!(dry.status.success(), "{}", stderr(&dry));
    let report = stdout(&dry);
    let expected_folder_move = format!(
        "folder: {} -> {}",
        root.join("old").join("old").display(),
        root.join("new").join("new").display()
    );
    assert!(report.contains(&expected_folder_move), "{report}");

    let write = run_vdl([
        "rename",
        "p.old",
        "p.new",
        root.to_str().unwrap(),
        "--write",
    ]);
    assert!(write.status.success(), "{}", stderr(&write));
    assert!(root.join("new/new.vdl").is_file());
    assert!(root.join("new/new/child.vdl").is_file());
    assert!(!root.join("old").exists());
    let validate = run_vdl(["validate", root.to_str().unwrap()]);
    assert!(validate.status.success(), "{}", stderr(&validate));
    let stats = run_vdl(["stats", root.to_str().unwrap(), "--json"]);
    assert!(stats.status.success(), "{}", stderr(&stats));
    assert!(stdout(&stats).contains("\"entities\": 2"), "{}", stdout(&stats));
}

#[test]
fn rename_resolves_bare_include_before_rewriting_same_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("schema.vdl"),
        r#"
schema "s" {
    type "Page" {
        field "label" text heading
        field "content" body compose ["Page"] optional
    }
}

"#,
    )
    .unwrap();
    fs::write(
        root.join("global.vdl"),
        r#"
entity "Page" "old" {
    field "label" "Global old"
}
"#,
    )
    .unwrap();
    fs::write(
        root.join("package.vdl"),
        r#"
package p
entity "Page" old {
    field "label" "Package local old"
    field "content" {
        Local body.
    }
}
entity "Page" owner {
    field "label" "Owner"
    field "content" {
        include old
    }
}
"#,
    )
    .unwrap();
    let output = run_vdl(["rename", "old", "new", root.to_str().unwrap(), "--write"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let package = read(root, "package.vdl");
    assert!(
        package.contains("include old"),
        "include resolves to p.old and must stay unchanged: {package}"
    );
    let validate = run_vdl(["validate", root.to_str().unwrap()]);
    assert!(validate.status.success(), "{}", stderr(&validate));
}

#[test]
fn rename_maps_include_offsets_after_lone_cr_newlines() {
    let (_dir, root) = copy_fixture("rename-v2");
    let path = root.join("children/child-b/child-b.vdl");
    let source = fs::read_to_string(&path).unwrap();
    let mixed = source.replacen(
        "include guide.sections.child-a",
        "Text\r        include guide.sections.child-a",
        1,
    );
    fs::write(&path, mixed).unwrap();
    let output = run_vdl([
        "rename",
        "guide.sections.child-a",
        "guide.sections.child-z",
        root.to_str().unwrap(),
        "--write",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let rewritten = read(&root, "children/child-b/child-b.vdl");
    assert!(
        rewritten.contains("Text\r        include guide.sections.child-z"),
        "{rewritten}"
    );
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
    let output = run_vdl([
        "rename",
        OLD,
        "guide.sections.detail",
        root.to_str().unwrap(),
    ]);
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
    assert!(err.contains("package-local declaration"), "stderr: {}", err);
    // Nothing was written: the dry-run contract holds even under --write.
    let before = read(&root, "sections/intro.vdl");
    assert!(before.contains(OLD), "source must be untouched: {before}");
}
