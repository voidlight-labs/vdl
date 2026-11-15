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
    for command in ["graph", "diff", "search"] {
        let output = run_vdl([command]);
        assert!(!output.status.success(), "{command} must not be available");
        assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
    }
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
