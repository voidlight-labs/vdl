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
