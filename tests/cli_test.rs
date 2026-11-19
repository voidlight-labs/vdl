use std::fs;
use std::path::Path;
use std::process::Command;

/// Helper: run the `vdl` binary via `cargo run -- <args>`.
fn run_vdl<I, S>(args: I) -> std::process::Output
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

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

#[test]
fn validate_rejects_removed_legacy_form() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.vdl");
    fs::write(&path, "axiom \"old\" { version \"1.0\" }").unwrap();
    let output = run_vdl(["validate", path.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("legacy form is no longer supported"), "{stderr}");
}

#[test]
fn validate_rejects_legacy_annotations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.vdl");
    fs::write(&path, "@author(\"person\")
axiom \"old\" {}").unwrap();
    let output = run_vdl(["validate", path.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("legacy form is no longer supported"), "{stderr}");
}

#[test]
fn removed_commands_are_rejected() {
    for command in ["diff"] {
        let output = run_vdl([command]);
        assert!(!output.status.success(), "{command} must not be available");
        assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
    }
}

// ---------------------------------------------------------------------------
// graph & search (schema-form)
// ---------------------------------------------------------------------------

#[test]
fn graph_json_lists_nodes_and_edges_in_declaration_order() {
    let output = run_vdl(["graph", "tests/fixtures/schema/runbooks.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("\"generator\""), "stdout: {}", stdout);
    assert!(stdout.contains("\"id\": \"core-guide\""), "stdout: {}", stdout);
    assert!(stdout.contains("\"field\": \"chapters\""), "stdout: {}", stdout);
    let intro = stdout.find("\"id\": \"intro\"").expect("intro node present");
    let core = stdout
        .find("\"id\": \"core-guide\"")
        .expect("core-guide node present");
    assert!(intro < core, "nodes must follow declaration order");
}

#[test]
fn graph_dot_outputs_a_digraph() {
    let output = run_vdl([
        "graph",
        "tests/fixtures/schema/runbooks.vdl",
        "--format",
        "dot",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("digraph VDL_Graph {"), "stdout: {}", stdout);
    assert!(
        stdout.contains("\"core-guide\" -> \"intro\" [label=\"chapters\"]"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn graph_index_outputs_a_search_array() {
    let output = run_vdl([
        "graph",
        "tests/fixtures/schema/runbooks.vdl",
        "--format",
        "index",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.trim_start().starts_with('['), "stdout: {}", stdout);
    assert!(stdout.contains("\"content\""), "stdout: {}", stdout);
    assert!(stdout.contains("\"id\": \"appendix\""), "stdout: {}", stdout);
    // Bodies are searchable content in the index.
    assert!(
        stdout.contains("keep the body verbatim"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn graph_root_filters_the_one_hop_neighborhood() {
    let from_core = run_vdl([
        "graph",
        "tests/fixtures/schema/runbooks.vdl",
        "--root",
        "core-guide",
    ]);
    let stdout = String::from_utf8_lossy(&from_core.stdout);
    assert!(from_core.status.success());
    assert!(stdout.contains("\"id\": \"core-guide\""), "stdout: {}", stdout);
    for neighbor in ["intro", "rules", "appendix"] {
        assert!(stdout.contains(&format!("\"id\": \"{neighbor}\"")), "stdout: {}", stdout);
    }
    assert!(!stdout.contains("ops-guide"), "stdout: {}", stdout);

    // Intro is referenced by both runbooks, so they enter the neighborhood.
    let from_intro = run_vdl([
        "graph",
        "tests/fixtures/schema/runbooks.vdl",
        "--root",
        "intro",
    ]);
    let stdout = String::from_utf8_lossy(&from_intro.stdout);
    assert!(from_intro.status.success());
    for expected in ["intro", "core-guide", "ops-guide"] {
        assert!(stdout.contains(&format!("\"id\": \"{expected}\"")), "stdout: {}", stdout);
    }
    assert!(!stdout.contains("\"id\": \"rules\""), "stdout: {}", stdout);
    assert!(!stdout.contains("\"id\": \"appendix\""), "stdout: {}", stdout);
}

#[test]
fn graph_unknown_root_fails() {
    let output = run_vdl([
        "graph",
        "tests/fixtures/schema/runbooks.vdl",
        "--root",
        "nope",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("entity not found"), "stderr: {}", stderr);
}

#[test]
fn graph_writes_output_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("graph.json");

    let output = run_vdl([
        "graph",
        "tests/fixtures/schema/runbooks.vdl",
        "--out",
        out.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "stderr: {}", stderr);
    assert!(stdout.contains("Graph written"), "stdout: {}", stdout);
    let content = fs::read_to_string(&out).expect("graph file should exist");
    assert!(content.contains("\"nodes\""), "{}", content);
}

#[test]
fn graph_rejects_output_over_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("runbooks.vdl");
    let src = fs::read_to_string("tests/fixtures/schema/runbooks.vdl").expect("fixture readable");
    fs::write(&input, src).expect("fixture copy written");
    let before = fs::read(&input).expect("input readable");

    let output = run_vdl([
        "graph",
        input.to_str().unwrap(),
        "--out",
        input.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("refusing to write output over the input"),
        "stderr: {}",
        stderr
    );
    assert_eq!(fs::read(&input).expect("input still readable"), before);
}

#[test]
fn search_matches_id_heading_and_body() {
    let output = run_vdl(["search", "appendix", "tests/fixtures/schema/runbooks.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("appendix | Chapter | Appendix"),
        "stdout: {}",
        stdout
    );

    // Body content is searchable.
    let output = run_vdl(["search", "verbatim", "tests/fixtures/schema/runbooks.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("rules | Chapter | Rules"), "stdout: {}", stdout);
}

#[test]
fn search_is_case_insensitive() {
    let output = run_vdl(["search", "CORE GUIDE", "tests/fixtures/schema/runbooks.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("core-guide | Runbook | Core Guide"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn search_prints_no_matches() {
    let output = run_vdl(["search", "zzz-nothing", "tests/fixtures/schema/runbooks.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("No entities matching 'zzz-nothing'."),
        "stdout: {}",
        stdout
    );
}

// ---------------------------------------------------------------------------
// stats
// ---------------------------------------------------------------------------

#[test]
fn stats_reports_population_per_field() {
    let output = run_vdl(["stats", "tests/fixtures/schema/runbooks.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Runbook (2 entities)"), "stdout: {}", stdout);
    assert!(stdout.contains("chapters  2/2"), "stdout: {}", stdout);
    assert!(stdout.contains("required [ref Chapter]"), "stdout: {}", stdout);
    // No runbook references another runbook.
    assert!(
        stdout.contains("Never referenced (2):"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn stats_json_flag_emits_a_report_object() {
    let output = run_vdl([
        "stats",
        "tests/fixtures/schema/runbooks.vdl",
        "--json",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("\"entities\": 5"), "stdout: {}", stdout);
    assert!(stdout.contains("\"orphans\""), "stdout: {}", stdout);
}

#[test]
fn compile_empty_directory_requires_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_vdl(["compile", dir.path().to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("vdl.toml"), "{stderr}");
    assert!(stderr.contains("no build manifest"), "{stderr}");
}

#[test]
fn compose_reports_schema_composition_cycle() {
    let output = run_vdl(["compose", "tests/fixtures/invalid_cycle.vdl", "--root", "a"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("composition cycle: a -> b -> a"), "{stderr}");
}

// ---------------------------------------------------------------------------
// compose (schema documents)
// ---------------------------------------------------------------------------

#[test]
fn test_validate_schema_document_success() {
    let output = run_vdl(["validate", "tests/fixtures/schema/runbooks.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Expected `vdl validate` to accept a schema document.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert!(stdout.contains("No errors"), "stdout: {}", stdout);
}

#[test]
fn test_validate_schema_document_failure() {
    let output = run_vdl(["validate", "tests/fixtures/schema/invalid_unresolved.vdl"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "expected validation to fail");
    assert!(
        stderr.contains("missing"),
        "expected the unresolved ID in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_compose_writes_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("core-guide.md");

    let output = run_vdl([
        "compose",
        "tests/fixtures/schema/runbooks.vdl",
        "--root",
        "core-guide",
        "--out",
        out.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {}", stderr);

    let content = fs::read_to_string(&out).expect("composed file should exist");
    assert!(content.contains("# Core Guide"), "{}", content);
    assert!(content.contains("## Introduction"), "{}", content);
}

#[test]
fn test_compose_prints_to_stdout() {
    let output = run_vdl([
        "compose",
        "tests/fixtures/schema/runbooks.vdl",
        "--root",
        "core-guide",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("## Introduction"), "stdout: {}", stdout);
    assert!(stdout.contains("## Rules"), "stdout: {}", stdout);
}

#[test]
fn test_compose_unknown_root_fails() {
    let output = run_vdl([
        "compose",
        "tests/fixtures/schema/runbooks.vdl",
        "--root",
        "nope",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("nope"), "stderr: {}", stderr);
}

// ---------------------------------------------------------------------------
// compose --out must never overwrite its own input
// ---------------------------------------------------------------------------

fn copy_fixture_to(dir: &Path) -> std::path::PathBuf {
    let input = dir.join("runbooks.vdl");
    let src = fs::read_to_string("tests/fixtures/schema/runbooks.vdl").expect("fixture readable");
    fs::write(&input, src).expect("fixture copy written");
    input
}

#[test]
fn test_compose_rejects_output_over_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = copy_fixture_to(dir.path());
    let before = fs::read(&input).expect("input readable");

    let output = run_vdl([
        "compose",
        input.to_str().unwrap(),
        "--root",
        "core-guide",
        "--out",
        input.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "expected a nonzero exit status");
    assert!(
        stderr.contains("refusing to write output over the input"),
        "stderr: {}",
        stderr
    );
    assert_eq!(
        fs::read(&input).expect("input still readable"),
        before,
        "input bytes must be untouched"
    );
}

#[test]
fn test_compose_rejects_relative_alias_of_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = copy_fixture_to(dir.path());
    let before = fs::read(&input).expect("input readable");

    // Same file reached through `.` and `..`, i.e. not the literal same string.
    let alias = dir.path().join("sub").join("..").join("runbooks.vdl");
    fs::create_dir_all(dir.path().join("sub")).unwrap();

    let output = run_vdl([
        "compose",
        input.to_str().unwrap(),
        "--root",
        "core-guide",
        "--out",
        alias.to_str().unwrap(),
    ]);

    assert!(!output.status.success(), "expected a nonzero exit status");
    assert_eq!(fs::read(&input).expect("input still readable"), before);
}

#[test]
fn test_compose_writes_distinct_markdown_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = copy_fixture_to(dir.path());
    let out = dir.path().join("out.md");

    let output = run_vdl([
        "compose",
        input.to_str().unwrap(),
        "--root",
        "core-guide",
        "--out",
        out.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {}", stderr);
    assert!(out.exists());
}

#[cfg(unix)]
#[test]
fn test_compose_rejects_symlink_alias_of_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = copy_fixture_to(dir.path());
    let link = dir.path().join("alias.vdl");
    std::os::unix::fs::symlink(&input, &link).expect("symlink created");
    let before = fs::read(&input).expect("input readable");

    let output = run_vdl([
        "compose",
        input.to_str().unwrap(),
        "--root",
        "core-guide",
        "--out",
        link.to_str().unwrap(),
    ]);

    assert!(!output.status.success(), "expected a nonzero exit status");
    assert_eq!(fs::read(&input).expect("input still readable"), before);
}

#[cfg(any(unix, windows))]
#[test]
fn test_compose_rejects_hard_link_alias_of_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = copy_fixture_to(dir.path());
    let link = dir.path().join("hardlink.vdl");
    fs::hard_link(&input, &link).expect("hard link created");
    let before = fs::read(&input).expect("input readable");

    let output = run_vdl([
        "compose",
        input.to_str().unwrap(),
        "--root",
        "core-guide",
        "--out",
        link.to_str().unwrap(),
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing to write output over the input"),
        "stderr: {}",
        stderr
    );
    assert!(!output.status.success(), "expected a nonzero exit status");
    assert_eq!(fs::read(&input).expect("input still readable"), before);
}
