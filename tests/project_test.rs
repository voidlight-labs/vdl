//! End-to-end tests for multi-file schema projects.
//!
//! A project is a directory containing `vdl.toml` and any number of `.vdl`
//! sources. Exactly one source declares the schema; every other source
//! declares instances, in any folder. Entity IDs are project-global, so moving
//! a file must not change any rendered output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_project() -> PathBuf {
    manifest_dir().join("tests/fixtures/project")
}

fn run_vdl<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new("cargo")
        .current_dir(manifest_dir())
        .arg("run")
        .arg("--quiet")
        .arg("--")
        .args(args)
        .output()
        .expect("failed to execute `cargo run`")
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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create destination directory");
    for entry in fs::read_dir(from).expect("read source directory") {
        let entry = entry.expect("directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Copy the checked-in fixture project into a fresh temporary directory so a
/// test can move files and write build output without touching the repository.
fn fresh_project() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("project");
    copy_dir(&fixture_project(), &root);
    (temp, root)
}

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (name, contents) in files {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directory");
        }
        fs::write(&path, contents).expect("write fixture file");
    }
}

const SCHEMA: &str = r#"
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "chapters" ref "Section" compose
    }

    type "Section" {
        field "label" text heading
        field "content" body
    }
}
"#;

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[test]
fn manifest_numbering_is_per_output_and_follows_composition() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        root,
        &[
            ("model.vdl", r#"
schema "manual" {
    type "Item" {
        field "label" text heading
        field "content" body optional
        field "children" ref "Item" compose optional
        field "appendices" ref "Item" compose optional
        field "related" ref "Item" optional
    }
}
Item root { label "Guide" children [first, second] appendices [appendix] related [shared] }
Item first { label "First" children [group, shared] }
Item group { label "Group" children [shared] }
Item second { label "Second" children [shared] }
Item shared {
    label "Shared"
    content """
    #### Authored heading
    """
}
Item appendix { label "Appendix" }
"#),
            ("vdl.toml", r#"
[build]
out_dir = "dist"
[[build.markdown]]
root = "root"
output = "numbered.md"
number_headings = true
[[build.markdown]]
root = "root"
output = "plain.md"
"#),
        ],
    );
    let result = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(result.status.success(), "{}", stderr(&result));

    let numbered = fs::read_to_string(root.join("dist/numbered.md")).unwrap();
    let plain = fs::read_to_string(root.join("dist/plain.md")).unwrap();
    let (numbered_meta, numbered_body) = numbered.split_once("\n---\n").unwrap();
    let (plain_meta, plain_body) = plain.split_once("\n---\n").unwrap();
    assert_eq!(numbered_meta, plain_meta);
    let headings = |body: &str| {
        body.lines()
            .filter(|line| line.starts_with('#'))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        headings(numbered_body),
        [
            "# Guide",
            "## 1. First",
            "### 1.1 Group",
            "#### 1.1.1 Shared",
            "#### Authored heading",
            "### 1.2 Shared",
            "#### Authored heading",
            "## 2. Second",
            "### 2.1 Shared",
            "#### Authored heading",
            "## 3. Appendix",
        ]
    );
    assert_eq!(
        headings(plain_body),
        [
            "# Guide",
            "## First",
            "### Group",
            "#### Shared",
            "#### Authored heading",
            "### Shared",
            "#### Authored heading",
            "## Second",
            "### Shared",
            "#### Authored heading",
            "## Appendix",
        ]
    );
    let preview = run_vdl(["compose", root.to_str().unwrap(), "--root", "root"]);
    assert!(preview.status.success(), "{}", stderr(&preview));
    assert_eq!(stdout(&preview), plain);
}

#[test]
fn packages_resolve_local_names_and_survive_moving_source_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        root,
        &[
            ("types.vdl", r#"
package soul.alignment

schema "soul" {
    type "framework" {
        field "title" text heading
        field "based_on" ref "axiom"
        field "includes" ref "artifact" compose
    }
    type "artifact" { field "title" text heading }
    type "axiom" { field "title" text }
}

framework divine-alignment {
    title "Divine Alignment"
    based_on [soul.axiom-zero]
    includes [sleep-and-death, elsewhere.sleep-and-death, "legacy-id"]
}
"#),
            ("foundation.vdl", r#"
package soul
axiom axiom-zero { title "Axiom Zero" }
"#),
            ("one/part.vdl", r#"
package soul.alignment
artifact sleep-and-death { title "Local Sleep and Death" }
"#),
            ("two/part.vdl", r#"
package elsewhere
artifact sleep-and-death { title "External Sleep and Death" }
artifact "legacy-id" { title "Quoted ID" }
"#),
            ("vdl.toml", r#"
[build]
out_dir = "dist"
[[build.markdown]]
root = "soul.alignment.divine-alignment"
output = "alignment.md"
"#),
        ],
    );

    let result = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(result.status.success(), "{}", stderr(&result));
    let output = root.join("dist/alignment.md");
    let before = fs::read_to_string(&output).unwrap();
    assert!(before.contains("\"based_on\":\n  - \"soul.axiom-zero\"\n"));
    assert!(before.ends_with(
        "## Local Sleep and Death\n\n## External Sleep and Death\n\n## Quoted ID\n"
    ));

    fs::rename(root.join("one/part.vdl"), root.join("two/local.vdl")).unwrap();
    let result = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(fs::read_to_string(output).unwrap(), before);
}

#[test]
fn compact_declarations_compile_with_project_defined_types_and_fields() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        root,
        &[
            ("types.vdl", r#"
schema "soul" {
    type "framework" {
        field "title" text heading
        field "version" text
        field "based_on" ref "axiom"
        field "includes" ref "artifact" compose
    }
    type "axiom" { field "title" text }
    type "artifact" {
        field "title" text heading
        field "body" body
        field "includes" ref "artifact" compose optional
    }
}
"#),
            ("foundation/axiom.vdl", r#"
axiom "soul.axiom-zero" { title "Axiom Zero" }
"#),
            ("framework.vdl", r#"
framework "soul.divine-alignment" {
    title "Divine Alignment"
    version "5.0.7"
    based_on ["soul.axiom-zero"]
    includes [
        "soul.alignment.foundation",
        "soul.alignment.sleep-and-death"
    ]
}
"#),
            ("parts/artifacts.vdl", r#"
entity "artifact" "soul.alignment.foundation" {
    field "title" "Foundation"
    field "body" """First section."""
}

artifact "soul.alignment.sleep-and-death" {
    title "Sleep and Death"
    body {
        Prosa lengkap bagian ini.
        Baris, paragraf, dan heading ditulis di sini.

        ~~~js
        function example() {
            return { title: "Rūḥ" };
        }
        ~~~
    }
}
"#),
            ("vdl.toml", r#"
[build]
out_dir = "dist"
[[build.markdown]]
root = "soul.divine-alignment"
output = "alignment.md"
"#),
        ],
    );

    let result = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(result.status.success(), "{}", stderr(&result));
    let markdown = fs::read_to_string(root.join("dist/alignment.md")).unwrap();
    assert!(markdown.contains("\"based_on\":\n  - \"soul.axiom-zero\"\n"));
    assert!(markdown.ends_with(
        "## Foundation\n\nFirst section.\n\n## Sleep and Death\n\n\
         Prosa lengkap bagian ini.\nBaris, paragraf, dan heading ditulis di sini.\n\n\
         ~~~js\nfunction example() {\n    return { title: \"Rūḥ\" };\n}\n~~~\n"
    ));
}

#[test]
fn acceptance_compile_project_then_move_a_source_without_changing_output() {
    let (_temp, root) = fresh_project();

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "compile failed.\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );

    let deploy = root.join("dist/runbooks/deploy.md");
    let recovery = root.join("dist/runbooks/recovery.md");
    let deploy_bytes = fs::read(&deploy).expect("deploy markdown rendered");
    let recovery_bytes = fs::read(&recovery).expect("recovery markdown rendered");

    let deploy_text = String::from_utf8_lossy(&deploy_bytes);
    assert!(deploy_text.contains("# Deploy Runbook"), "{}", deploy_text);
    assert!(deploy_text.contains("## Introduction"), "{}", deploy_text);
    assert!(deploy_text.contains("## Deploy Steps"), "{}", deploy_text);
    assert!(
        deploy_text.contains("Shared introduction used by every runbook."),
        "{}",
        deploy_text
    );

    // Move the shared section to another nested directory. The manifest, the
    // IDs, and every reference stay exactly as they were.
    let moved_dir = root.join("sections/elsewhere");
    fs::create_dir_all(&moved_dir).expect("create moved directory");
    fs::rename(
        root.join("sections/nested/intro.vdl"),
        moved_dir.join("moved-intro.vdl"),
    )
    .expect("move shared section file");

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "recompile after move failed.\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );

    assert_eq!(
        fs::read(&deploy).expect("deploy markdown still rendered"),
        deploy_bytes,
        "moving a source file must not change deploy.md"
    );
    assert_eq!(
        fs::read(&recovery).expect("recovery markdown still rendered"),
        recovery_bytes,
        "moving a source file must not change recovery.md"
    );
}

#[test]
fn validate_directory_uses_the_project_loader_without_a_manifest() {
    let (_temp, root) = fresh_project();
    fs::remove_file(root.join("vdl.toml")).expect("remove manifest");

    let output = run_vdl(["validate", root.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "validate failed.\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("Validated"), "{}", stdout(&output));
}

#[test]
fn compose_directory_uses_the_project_loader_without_a_manifest() {
    let (_temp, root) = fresh_project();
    fs::remove_file(root.join("vdl.toml")).expect("remove manifest");

    let output = run_vdl(["compose", root.to_str().unwrap(), "--root", "deploy"]);
    assert!(
        output.status.success(),
        "compose failed.\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );

    let text = stdout(&output);
    assert!(text.contains("# Deploy Runbook"), "{}", text);
    let intro = text
        .find("## Introduction")
        .expect("shared section rendered");
    let steps = text.find("## Deploy Steps").expect("own section rendered");
    assert!(intro < steps, "{}", text);
}

// ---------------------------------------------------------------------------
// Cross-file diagnostics
// ---------------------------------------------------------------------------

#[test]
fn unresolved_cross_file_reference_points_at_the_usage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        &root,
        &[
            ("schema.vdl", SCHEMA),
            (
                "a/run.vdl",
                r#"entity "Runbook" "deploy" {
    field "label" "Deploy"
    field "chapters" [ "missing" ]
}
"#,
            ),
        ],
    );

    let output = run_vdl(["validate", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("run.vdl"), "expected usage file in: {}", err);
    assert!(err.contains("unresolved entity 'missing'"), "{}", err);
}

#[test]
fn wrong_cross_file_reference_type_points_at_the_usage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        &root,
        &[
            ("schema.vdl", SCHEMA),
            (
                "a/other.vdl",
                r#"entity "Runbook" "other" {
    field "label" "Other"
    field "chapters" [ ]
}
"#,
            ),
            (
                "b/run.vdl",
                r#"entity "Runbook" "deploy" {
    field "label" "Deploy"
    field "chapters" [ "other" ]
}
"#,
            ),
        ],
    );

    let output = run_vdl(["validate", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("run.vdl"), "expected usage file in: {}", err);
    // miette may wrap between words depending on terminal width and path length.
    let normalized = err.lines().map(|line| line.trim().trim_start_matches('│').trim()).collect::<Vec<_>>().join(" ");
    assert!(normalized.contains("expects type 'Section' but 'other' is of type 'Runbook'"), "{}", err);
}

#[test]
fn duplicate_entity_id_reports_both_locations() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        &root,
        &[
            ("schema.vdl", SCHEMA),
            (
                "one/first.vdl",
                r#"entity "Section" "shared" {
    field "label" "First"
    field "content" """First."""
}
"#,
            ),
            (
                "two/second.vdl",
                r#"entity "Section" "shared" {
    field "label" "Second"
    field "content" """Second."""
}
"#,
            ),
        ],
    );

    let output = run_vdl(["validate", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("duplicate entity ID"), "{}", err);
    assert!(err.contains("'shared'"), "{}", err);
    assert!(
        err.contains("first.vdl"),
        "expected first location in: {}",
        err
    );
    assert!(
        err.contains("second.vdl"),
        "expected duplicate location in: {}",
        err
    );
}

#[test]
fn entity_only_project_reports_a_missing_schema() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        &root,
        &[(
            "orphan.vdl",
            r#"entity "Section" "intro" {
    field "label" "Intro"
    field "content" """Body."""
}
"#,
        )],
    );

    let output = run_vdl(["validate", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("schema document"), "{}", err);
}

#[test]
fn two_schema_blocks_across_files_report_both_locations() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(
        &root,
        &[
            ("a.vdl", &SCHEMA.replace("runbooks", "first")),
            ("nested/b.vdl", &SCHEMA.replace("runbooks", "second")),
        ],
    );

    let output = run_vdl(["validate", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("2 schema blocks"), "{}", err);
    assert!(err.contains("a.vdl"), "expected first location in: {}", err);
    assert!(
        err.contains("b.vdl"),
        "expected second location in: {}",
        err
    );
}

#[test]
fn mixing_schema_sources_with_legacy_documents_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let legacy = r#"axiom "old" { version "1.0" }"#;
    write_files(
        &root,
        &[
            ("schema.vdl", SCHEMA),
            (
                "run.vdl",
                r#"entity "Runbook" "deploy" {
    field "label" "Deploy"
    field "chapters" [ ]
}
"#,
            ),
            ("legacy.vdl", legacy),
        ],
    );

    let output = run_vdl(["validate", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("legacy"), "{}", err);
    assert!(err.contains("legacy.vdl"), "{}", err);
}

// ---------------------------------------------------------------------------
// Manifest and target diagnostics
// ---------------------------------------------------------------------------

#[test]
fn compile_without_a_manifest_explains_how_to_create_one() {
    let (_temp, root) = fresh_project();
    fs::remove_file(root.join("vdl.toml")).expect("remove manifest");

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("vdl.toml"), "{}", err);
    assert!(err.contains("out_dir"), "{}", err);
    assert!(err.contains("build.markdown"), "{}", err);
}

#[test]
fn malformed_manifest_is_rejected_with_the_manifest_path() {
    let (_temp, root) = fresh_project();
    fs::write(root.join("vdl.toml"), "[build\n").expect("write malformed manifest");

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("invalid build manifest"), "{}", err);
    assert!(err.contains("vdl.toml"), "{}", err);
}

#[test]
fn unknown_root_fails_before_writing_and_keeps_existing_output() {
    let (_temp, root) = fresh_project();
    let first = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(first.status.success(), "{}", stderr(&first));

    let deploy = root.join("dist/runbooks/deploy.md");
    let before = fs::read(&deploy).expect("previous output readable");

    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/deploy.md"

[[build.markdown]]
root = "nope"
output = "runbooks/nope.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("nope"), "{}", err);
    assert_eq!(fs::read(&deploy).expect("output still readable"), before);
    assert!(!root.join("dist/runbooks/nope.md").exists());
}

#[test]
fn colliding_destinations_are_rejected_before_writing() {
    let (_temp, root) = fresh_project();
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/same.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/same.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("collides"), "{}", err);
    assert!(!root.join("dist/runbooks/same.md").exists());
}

#[test]
fn output_escaping_the_output_directory_is_rejected() {
    let (_temp, root) = fresh_project();
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "../escaped.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("escapes the project directory"), "{}", err);
    assert!(!root.join("escaped.md").exists());
    assert!(!root.join("dist/../escaped.md").exists());
}

#[test]
fn destination_matching_a_source_file_is_rejected() {
    let (_temp, root) = fresh_project();
    let schema = root.join("schema.vdl");
    let before = fs::read(&schema).expect("schema readable");

    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "."

[[build.markdown]]
root = "deploy"
output = "schema.vdl"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains(".vdl source extension"), "{}", err);
    assert_eq!(fs::read(&schema).expect("schema still readable"), before);
}

#[test]
fn destination_matching_the_manifest_is_rejected() {
    let (_temp, root) = fresh_project();
    let manifest = root.join("vdl.toml");
    let before = fs::read(&manifest).expect("manifest readable");

    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "."

[[build.markdown]]
root = "deploy"
output = "vdl.toml"
"#,
        )],
    );
    let configured = fs::read(&manifest).expect("configured manifest readable");
    assert_ne!(before, configured);

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("over the build manifest"), "{}", err);
    assert_eq!(
        fs::read(&manifest).expect("manifest still readable"),
        configured
    );
}

#[cfg(unix)]
#[test]
fn symlinked_destination_that_aliases_a_source_is_rejected() {
    let (_temp, root) = fresh_project();
    let schema = root.join("schema.vdl");
    let before = fs::read(&schema).expect("schema readable");
    fs::create_dir_all(root.join("dist")).expect("dist directory");
    std::os::unix::fs::symlink(&schema, root.join("dist/alias.md")).expect("symlink created");

    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "alias.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("alias.md"), "{}", err);
    assert_eq!(fs::read(&schema).expect("schema still readable"), before);
}

#[cfg(any(unix, windows))]
#[test]
fn hard_linked_destination_that_aliases_a_source_is_rejected() {
    let (_temp, root) = fresh_project();
    let schema = root.join("schema.vdl");
    let before = fs::read(&schema).expect("schema readable");
    fs::create_dir_all(root.join("dist")).expect("dist directory");
    fs::hard_link(&schema, root.join("dist/alias.md")).expect("hard link created");

    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "alias.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("alias.md"), "{}", err);
    assert_eq!(fs::read(&schema).expect("schema still readable"), before);
}

#[test]
fn destination_that_is_an_existing_directory_is_rejected() {
    let (_temp, root) = fresh_project();
    fs::create_dir_all(root.join("dist/blocked.md")).expect("blocked directory");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "blocked.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("existing directory"), "{}", err);
    assert!(root.join("dist/blocked.md").is_dir());
}

#[cfg(unix)]
#[test]
fn dangling_symlink_collision_between_targets_is_rejected() {
    let (_temp, root) = fresh_project();
    fs::create_dir_all(root.join("dist")).expect("dist directory");
    std::os::unix::fs::symlink("deploy.md", root.join("dist/link.md")).expect("symlink created");

    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "deploy.md"

[[build.markdown]]
root = "recovery"
output = "link.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("collides"), "{}", err);
    assert!(!root.join("dist/deploy.md").exists());
}

#[test]
fn project_validation_failure_keeps_existing_outputs_intact() {
    let (_temp, root) = fresh_project();
    let first = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(first.status.success(), "{}", stderr(&first));

    let deploy = root.join("dist/runbooks/deploy.md");
    let recovery = root.join("dist/runbooks/recovery.md");
    let deploy_before = fs::read(&deploy).expect("deploy readable");
    let recovery_before = fs::read(&recovery).expect("recovery readable");

    write_files(
        &root,
        &[(
            "runbooks/recovery.vdl",
            r#"entity "Runbook" "recovery" {
    field "label" "Recovery Runbook"
    field "chapters" [ "recovery-steps", "gone" ]
    field "see_also" [ "intro" ]
}
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(!output.status.success(), "expected failure");
    assert_eq!(
        fs::read(&deploy).expect("deploy still readable"),
        deploy_before
    );
    assert_eq!(
        fs::read(&recovery).expect("recovery still readable"),
        recovery_before
    );
}

// ---------------------------------------------------------------------------
// compose --out protects every project source
// ---------------------------------------------------------------------------

#[test]
fn compose_out_refuses_another_project_source() {
    let (_temp, root) = fresh_project();
    let target = root.join("runbooks/recovery.vdl");
    let before = fs::read(&target).expect("source readable");

    let output = run_vdl([
        "compose",
        root.to_str().unwrap(),
        "--root",
        "deploy",
        "--out",
        target.to_str().unwrap(),
    ]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(
        err.contains("refusing to write output over"),
        "stderr: {}",
        err
    );
    assert_eq!(fs::read(&target).expect("source still readable"), before);
}

#[test]
fn compose_out_refuses_the_manifest() {
    let (_temp, root) = fresh_project();
    let manifest = root.join("vdl.toml");
    let before = fs::read(&manifest).expect("manifest readable");

    let output = run_vdl([
        "compose",
        root.to_str().unwrap(),
        "--root",
        "deploy",
        "--out",
        manifest.to_str().unwrap(),
    ]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(
        err.contains("refusing to write output over"),
        "stderr: {}",
        err
    );
    assert_eq!(
        fs::read(&manifest).expect("manifest still readable"),
        before
    );
}

#[test]
fn ancestor_and_descendant_targets_are_rejected_without_writes_in_either_order() {
    for descendant_first in [false, true] {
        let (_temp, root) = fresh_project();
        let output_dir = root.join("dist/runbooks");
        fs::create_dir_all(&output_dir).expect("create output directory");
        let first = output_dir.join("first.md");
        fs::write(&first, b"previous output").expect("write existing output");

        let conflicting_targets = if descendant_first {
            r#"[[build.markdown]]
root = "recovery"
output = "runbooks/node.md/child.md"

[[build.markdown]]
root = "deploy"
output = "runbooks/node.md"
"#
        } else {
            r#"[[build.markdown]]
root = "deploy"
output = "runbooks/node.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/node.md/child.md"
"#
        };
        let manifest = format!(
            "[build]\nout_dir = \"dist\"\n\n[[build.markdown]]\nroot = \"deploy\"\noutput = \"runbooks/first.md\"\n\n{}",
            conflicting_targets
        );
        write_files(&root, &[("vdl.toml", &manifest)]);

        let output = run_vdl(["compile", root.to_str().unwrap()]);
        let err = stderr(&output);
        assert!(!output.status.success(), "expected failure");
        assert!(err.contains("collid"), "{}", err);
        assert_eq!(fs::read(&first).unwrap(), b"previous output");
        assert!(!output_dir.join("node.md").exists());
    }
}

#[cfg(unix)]
#[test]
fn ancestor_conflict_through_symlinked_parent_is_rejected_before_writing() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist/runbooks");
    fs::create_dir_all(output_dir.join("real")).expect("create real output directory");
    std::os::unix::fs::symlink("real", output_dir.join("alias"))
        .expect("create output-directory alias");
    let first = output_dir.join("first.md");
    fs::write(&first, b"previous output").expect("write existing output");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/first.md"

[[build.markdown]]
root = "deploy"
output = "runbooks/real/node.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/alias/node.md/child.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("collid"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"previous output");
    assert!(!output_dir.join("real/node.md").exists());
}

#[test]
fn vdl_extension_output_is_rejected_before_other_outputs_are_written() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist/runbooks");
    fs::create_dir_all(&output_dir).expect("create output directory");
    let first = output_dir.join("first.md");
    fs::write(&first, b"previous output").expect("write existing output");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/first.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/generated.vdl"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("build.markdown[2]"), "{}", err);
    assert!(err.contains("generated.vdl"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"previous output");
    assert!(!output_dir.join("generated.vdl").exists());
}

#[cfg(unix)]
#[test]
fn symlink_output_with_effective_vdl_extension_is_rejected() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist/runbooks");
    fs::create_dir_all(&output_dir).expect("create output directory");
    let first = output_dir.join("first.md");
    fs::write(&first, b"previous output").expect("write existing output");
    let alias = output_dir.join("generated.md");
    std::os::unix::fs::symlink("effective.vdl", &alias).expect("create dangling symlink");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/first.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/generated.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("build.markdown[2]"), "{}", err);
    assert!(err.contains("effective.vdl"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"previous output");
    assert!(!output_dir.join("effective.vdl").exists());
}

#[test]
fn two_builds_with_an_unchanged_valid_manifest_both_succeed() {
    let (_temp, root) = fresh_project();

    let first = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(first.status.success(), "{}", stderr(&first));
    let deploy = fs::read(root.join("dist/runbooks/deploy.md")).expect("first output");
    let recovery = fs::read(root.join("dist/runbooks/recovery.md")).expect("second output");

    let second = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(second.status.success(), "{}", stderr(&second));
    assert_eq!(
        fs::read(root.join("dist/runbooks/deploy.md")).unwrap(),
        deploy
    );
    assert_eq!(
        fs::read(root.join("dist/runbooks/recovery.md")).unwrap(),
        recovery
    );
}

#[cfg(unix)]
#[test]
fn resolved_out_dir_outside_project_is_rejected_without_touching_external_files() {
    for (out_dir, link_path) in [("dist", "dist"), ("middle/output", "middle")] {
        let (temp, root) = fresh_project();
        let external = temp.path().join("external");
        fs::create_dir_all(&external).expect("create external directory");
        let sentinel = external.join("keep.txt");
        fs::write(&sentinel, b"external data").expect("write external sentinel");
        std::os::unix::fs::symlink(&external, root.join(link_path))
            .expect("create external symlink");
        write_files(
            &root,
            &[(
                "vdl.toml",
                &format!(
                    "[build]\nout_dir = \"{}\"\n\n[[build.markdown]]\nroot = \"deploy\"\noutput = \"runbooks/deploy.md\"\n",
                    out_dir
                ),
            )],
        );

        let output = run_vdl(["compile", root.to_str().unwrap()]);
        let err = stderr(&output);
        assert!(!output.status.success(), "expected failure");
        assert!(err.contains("out_dir"), "{}", err);
        assert!(err.contains("outside the project"), "{}", err);
        assert_eq!(fs::read(&sentinel).unwrap(), b"external data");
        assert!(!external.join("runbooks/deploy.md").exists());
    }
}

#[test]
fn project_root_is_a_valid_output_directory() {
    let (_temp, root) = fresh_project();
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "."

[[build.markdown]]
root = "deploy"
output = "root-output/deploy.md"
"#,
        )],
    );
    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(root.join("root-output/deploy.md").is_file());
}

#[cfg(unix)]
#[test]
fn in_project_symlink_is_a_valid_output_directory() {
    let (_temp, root) = fresh_project();
    fs::create_dir_all(root.join("internal-output")).expect("create internal directory");
    std::os::unix::fs::symlink("internal-output", root.join("output-link"))
        .expect("create in-project symlink");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "output-link"

[[build.markdown]]
root = "deploy"
output = "deploy.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(root.join("internal-output/deploy.md").is_file());
}

#[test]
fn file_parent_blocks_later_target_before_any_output_is_written() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist");
    fs::create_dir_all(&output_dir).expect("create output directory");
    let first = output_dir.join("first.md");
    let blocker = output_dir.join("blocked.md");
    fs::write(&first, b"old first output").expect("write existing output");
    fs::write(&blocker, b"blocker file").expect("write blocking file");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "first.md"

[[build.markdown]]
root = "recovery"
output = "blocked.md/child.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("build.markdown[2]"), "{}", err);
    assert!(err.contains("blocked.md"), "{}", err);
    assert!(err.contains("non-directory parent"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"old first output");
    assert_eq!(fs::read(&blocker).unwrap(), b"blocker file");
    assert!(!blocker.join("child.md").exists());
}

#[test]
fn file_parent_several_levels_above_target_is_rejected_in_preflight() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist");
    let blocker = output_dir.join("tree/level-1/blocked.md");
    fs::create_dir_all(blocker.parent().unwrap()).expect("create blocker parents");
    let first = output_dir.join("first.md");
    fs::write(&first, b"old first output").expect("write existing output");
    fs::write(&blocker, b"deep blocker").expect("write blocking file");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "first.md"

[[build.markdown]]
root = "recovery"
output = "tree/level-1/blocked.md/level-2/level-3/output.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("build.markdown[2]"), "{}", err);
    assert!(err.contains("blocked.md"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"old first output");
    assert_eq!(fs::read(&blocker).unwrap(), b"deep blocker");
    assert!(!blocker.join("level-2/level-3/output.md").exists());
}

#[cfg(unix)]
#[test]
fn symlink_to_file_cannot_be_used_as_output_parent() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist");
    fs::create_dir_all(&output_dir).expect("create output directory");
    let first = output_dir.join("first.md");
    let blocker = output_dir.join("blocked-link");
    fs::write(&first, b"old first output").expect("write existing output");
    std::os::unix::fs::symlink("../schema.vdl", &blocker).expect("link to source file");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "first.md"

[[build.markdown]]
root = "recovery"
output = "blocked-link/child.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("build.markdown[2]"), "{}", err);
    assert!(err.contains("blocked-link"), "{}", err);
    assert!(err.contains("non-directory parent"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"old first output");
    assert!(!blocker.join("child.md").exists());
}

#[test]
fn missing_nested_output_parents_are_created_after_preflight() {
    let (_temp, root) = fresh_project();
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "new/nested/runbooks/deploy.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(root.join("dist/new/nested/runbooks/deploy.md").is_file());
}

#[cfg(unix)]
#[test]
fn symlink_output_with_missing_effective_parent_fails_before_any_write() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist");
    fs::create_dir_all(&output_dir).expect("create output directory");
    let first = output_dir.join("first.md");
    let alias = output_dir.join("alias.md");
    fs::write(&first, b"old first output").expect("write existing output");
    std::os::unix::fs::symlink("missing-dir/actual.md", &alias)
        .expect("create symlink with missing target parent");
    let alias_target = fs::read_link(&alias).expect("read symlink target");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "first.md"

[[build.markdown]]
root = "recovery"
output = "alias.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("build.markdown[2]"), "{}", err);
    assert!(err.contains("alias.md"), "{}", err);
    assert!(err.contains("missing-dir"), "{}", err);
    assert!(err.contains("effective parent"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"old first output");
    assert!(fs::symlink_metadata(&alias)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_link(&alias).unwrap(), alias_target);
    assert!(!output_dir.join("missing-dir").exists());
}

#[cfg(unix)]
#[test]
fn symlink_output_with_existing_effective_parent_succeeds() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist");
    fs::create_dir_all(output_dir.join("generated")).expect("create effective parent");
    let alias = output_dir.join("alias.md");
    std::os::unix::fs::symlink("generated/actual.md", &alias)
        .expect("create symlink with existing target parent");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "alias.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(fs::symlink_metadata(&alias)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(output_dir.join("generated/actual.md").is_file());
}

#[cfg(unix)]
#[test]
fn symlink_output_with_missing_component_before_parent_dir_fails_before_any_write() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist");
    fs::create_dir_all(output_dir.join("existing")).expect("create effective parent");
    let first = output_dir.join("first.md");
    let alias = output_dir.join("alias.md");
    let alias_target = PathBuf::from("missing/../existing/actual.md");
    fs::write(&first, b"old first output").expect("write existing output");
    std::os::unix::fs::symlink(&alias_target, &alias)
        .expect("create symlink with missing component");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "first.md"

[[build.markdown]]
root = "recovery"
output = "alias.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "expected failure");
    assert!(err.contains("build.markdown[2]"), "{}", err);
    assert!(err.contains("alias.md"), "{}", err);
    assert!(err.contains("missing"), "{}", err);
    assert_eq!(fs::read(&first).unwrap(), b"old first output");
    assert!(fs::symlink_metadata(&alias)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_link(&alias).unwrap(), alias_target);
    assert!(!output_dir.join("missing").exists());
    assert!(!output_dir.join("existing/actual.md").exists());
}

#[cfg(unix)]
#[test]
fn symlink_output_with_existing_component_before_parent_dir_succeeds() {
    let (_temp, root) = fresh_project();
    let output_dir = root.join("dist");
    fs::create_dir_all(output_dir.join("missing")).expect("create traversed directory");
    fs::create_dir_all(output_dir.join("existing")).expect("create effective parent");
    let alias = output_dir.join("alias.md");
    std::os::unix::fs::symlink("missing/../existing/actual.md", &alias)
        .expect("create symlink with parent-directory component");
    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "alias.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(fs::symlink_metadata(&alias)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(output_dir.join("existing/actual.md").is_file());
}

// ---------------------------------------------------------------------------
// Graph build targets ([[build.graph]])
// ---------------------------------------------------------------------------

const GRAPH_PROJECT_FILES: &[(&str, &str)] = &[
    ("model.vdl", SCHEMA),
    (
        "runbooks.vdl",
        r#"
entity "Section" "intro" {
    field "label" "Intro"
    field "content" """Intro body."""
}

entity "Runbook" "guide" {
    field "label" "Guide"
    field "chapters" [ "intro" ]
}
"#,
    ),
    (
        "vdl.toml",
        r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "guide"
output = "guide.md"

[[build.graph]]
format = "json"
output = "graph.json"

[[build.graph]]
format = "dot"
output = "graph.dot"

[[build.graph]]
format = "index"
output = "search.json"
"#,
    ),
];

#[test]
fn compile_writes_graph_targets_alongside_markdown() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(root, GRAPH_PROJECT_FILES);

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    for artifact in ["guide.md", "graph.json", "graph.dot", "search.json"] {
        assert!(out.contains(artifact), "stdout: {}", out);
    }

    assert!(root.join("dist/guide.md").is_file());
    let json = fs::read_to_string(root.join("dist/graph.json")).expect("graph.json written");
    assert!(json.contains("\"id\": \"guide\""), "{json}");
    assert!(json.contains("\"field\": \"chapters\""), "{json}");
    let dot = fs::read_to_string(root.join("dist/graph.dot")).expect("graph.dot written");
    assert!(dot.contains("digraph VDL_Graph {"), "{dot}");
    let index = fs::read_to_string(root.join("dist/search.json")).expect("search.json written");
    assert!(index.contains("\"id\": \"intro\""), "{index}");

    // The build artifact is byte-identical to the `graph` command's stdout.
    let cli_graph = run_vdl(["graph", root.to_str().unwrap()]);
    assert_eq!(cli_graph.stdout, json.as_bytes());
}

#[test]
fn graph_json_output_is_deterministic_across_compiles() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(root, GRAPH_PROJECT_FILES);

    let first = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(first.status.success(), "{}", stderr(&first));
    let before = fs::read(root.join("dist/graph.json")).expect("first graph.json");

    fs::remove_dir_all(root.join("dist")).expect("clean dist between compiles");
    let second = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(second.status.success(), "{}", stderr(&second));
    let after = fs::read(root.join("dist/graph.json")).expect("second graph.json");

    assert_eq!(before, after, "equal inputs must produce equal bytes");
}

#[test]
fn graph_output_hard_linked_to_a_source_is_rejected() {
    let (_temp, root) = fresh_project();
    let schema = root.join("schema.vdl");
    let before = fs::read(&schema).expect("schema readable");
    fs::create_dir_all(root.join("dist")).expect("dist directory");
    fs::hard_link(&schema, root.join("dist/copy.json")).expect("hard link created");

    write_files(
        &root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/deploy.md"

[[build.graph]]
format = "json"
output = "copy.json"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "{}", err);
    assert!(
        err.contains("refusing to write build output"),
        "stderr: {}",
        err
    );
    assert!(err.contains("schema.vdl"), "stderr: {}", err);
    assert_eq!(fs::read(&schema).expect("schema still readable"), before);
    assert!(
        !root.join("dist/runbooks/deploy.md").exists(),
        "markdown must not be written"
    );
}

#[test]
fn unknown_graph_format_is_rejected_by_the_manifest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(root, GRAPH_PROJECT_FILES);
    write_files(
        root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "guide"
output = "guide.md"

[[build.graph]]
format = "svg"
output = "graph.svg"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "{}", err);
    assert!(err.contains("invalid build manifest"), "stderr: {}", err);
    assert!(
        err.contains("\"json\", \"dot\", \"index\""),
        "stderr: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// Cross-file cite links ([[build.markdown]] cite_links)
// ---------------------------------------------------------------------------

const CITE_LINK_FILES: &[(&str, &str)] = &[
    (
        "model.vdl",
        r#"
schema "net" {
    type "Doc" {
        field "label" text heading
        field "hubs" ref "Hub" compose
    }
    type "Hub" {
        field "label" text heading
        field "refs" ref "Leaf" cite
    }
    type "Leaf" {
        field "label" text heading
        field "content" body
    }
}
"#,
    ),
    (
        "net.vdl",
        r#"
entity "Leaf" "net.leaf.alpha" {
    field "label" "Alpha"
    field "content" """Leaf body."""
}

entity "Hub" "net.hub" {
    field "label" "Hub"
    field "refs" [ "net.leaf.alpha" ]
}

entity "Doc" "net.doc" {
    field "label" "Net"
    field "hubs" [ "net.hub" ]
}
"#,
    ),
    (
        "vdl.toml",
        r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "net.doc"
output = "doc.md"
cites = "link"
cite_links = true

[[build.markdown]]
root = "net.leaf.alpha"
output = "leaf/alpha.md"
"#,
    ),
];

#[test]
fn compile_renders_cites_as_relative_links_between_outputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(root, CITE_LINK_FILES);

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));

    let doc_md = fs::read_to_string(root.join("dist/doc.md")).expect("doc written");
    assert!(
        doc_md.contains("**Cites:** [net.leaf.alpha](leaf/alpha.md)"),
        "doc: {}",
        doc_md
    );
}

#[test]
fn cite_links_absent_keeps_plain_ids() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(root, CITE_LINK_FILES);
    write_files(
        root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "net.doc"
output = "doc.md"
cites = "link"

[[build.markdown]]
root = "net.leaf.alpha"
output = "leaf/alpha.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let doc_md = fs::read_to_string(root.join("dist/doc.md")).expect("doc written");
    assert!(doc_md.contains("**Cites:** net.leaf.alpha"), "doc: {}", doc_md);
    assert!(!doc_md.contains("leaf/alpha.md"), "doc: {}", doc_md);
}

// ---------------------------------------------------------------------------
// TOC build targets ([[build.toc]])
// ---------------------------------------------------------------------------

#[test]
fn compile_writes_a_toc_target_from_the_composed_tree() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(root, GRAPH_PROJECT_FILES);
    write_files(
        root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "guide"
output = "guide.md"

[[build.toc]]
root = "guide"
output = "nested/toc.md"
number_headings = true
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));

    let toc = fs::read_to_string(root.join("dist/nested/toc.md")).expect("toc written");
    assert!(toc.contains("- Guide"), "{toc}");
    // The guide composes its single section with a position number.
    assert!(toc.contains("  - 1. Intro"), "{toc}");
    // TOC carries headings only; no body prose leaks in.
    assert!(!toc.contains("Intro body"), "{toc}");
}

#[test]
fn toc_target_colliding_with_markdown_output_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_files(root, GRAPH_PROJECT_FILES);
    write_files(
        root,
        &[(
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "guide"
output = "guide.md"

[[build.toc]]
root = "guide"
output = "guide.md"
"#,
        )],
    );

    let output = run_vdl(["compile", root.to_str().unwrap()]);
    let err = stderr(&output);
    assert!(!output.status.success(), "{}", err);
    assert!(err.contains("collides"), "stderr: {}", err);
}
