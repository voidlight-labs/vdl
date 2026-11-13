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
fn test_validate_seven_laws_success() {
    let output = run_vdl(["validate", "tests/fixtures/seven_laws.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Expected `vdl validate` to succeed for seven_laws.vdl.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("Validated"),
        "Expected 'Validated' in output. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("No errors"),
        "Expected 'No errors' in output. stdout: {}",
        stdout
    );
}

#[test]
fn test_validate_invalid_cycle_failure() {
    let output = run_vdl(["validate", "tests/fixtures/invalid_cycle.vdl"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "Expected `vdl validate` to fail for invalid_cycle.vdl (circular dependency)."
    );
    let stderr_lower = stderr.to_lowercase();
    assert!(
        stderr_lower.contains("error") || stderr_lower.contains("cycle"),
        "Expected error message about cycle. stderr: {}",
        stderr
    );
}

// ---------------------------------------------------------------------------
// compile
// ---------------------------------------------------------------------------

#[test]
fn test_compile_produces_expected_outputs() {
    // Clean up any pre-existing output directory so we get a fresh run.
    let _ = fs::remove_dir_all("output");

    let output = run_vdl(["compile", "tests/fixtures/seven_laws.vdl"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Expected `vdl compile` to succeed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    assert!(
        Path::new("output/graph.json").exists(),
        "Expected output/graph.json to be created."
    );
    assert!(
        Path::new("output/soul").exists(),
        "Expected output/soul/ directory to be created."
    );
    assert!(
        Path::new("output/search.json").exists(),
        "Expected output/search.json to be created."
    );
    assert!(
        Path::new("output/graph.dot").exists(),
        "Expected output/graph.dot to be created."
    );

    // Optional tidy-up so the workspace stays clean.
    let _ = fs::remove_dir_all("output");
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

#[test]
fn test_compose_rejects_legacy_document() {
    let output = run_vdl([
        "compose",
        "tests/fixtures/seven_laws.vdl",
        "--root",
        "anything",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("schema document"),
        "expected a message about schema documents, got: {}",
        stderr
    );
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
