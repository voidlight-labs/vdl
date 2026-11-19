//! Validate every build target, render everything, then write.
//!
//! Nothing is written until every target has been checked: the root exists,
//! the destination stays inside `out_dir` (symlinks included), it does not
//! alias a project source or the manifest, and it does not collide with
//! another target. A validation failure therefore leaves existing output
//! untouched.

use crate::codegen::compose::{compose_with_options, ComposeOptions};
use crate::error::{VdlError, VdlResult};
use crate::fsutil::{canonical_anchor, paths_alias};
use crate::graph::GraphFormat;
use crate::model::SchemaDoc;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::loader::{SchemaProject, load_schema_project};
use super::manifest::{load_manifest, missing_manifest_error};

/// What a successful build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub outputs: Vec<PathBuf>,
}

struct PlannedTarget {
    destination: PathBuf,
    anchor: PathBuf,
    contents: String,
}

fn check_target_parent_directories(label: &str, destination: &Path) -> VdlResult<()> {
    let Some(parent) = destination.parent() else {
        return Ok(());
    };
    let mut ancestors: Vec<_> = parent.ancestors().collect();
    ancestors.reverse();

    for ancestor in ancestors {
        if ancestor.as_os_str().is_empty() {
            continue;
        }

        let entry = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(VdlError::Io {
                    message: format!(
                        "{} output '{}' could not inspect parent '{}': {}",
                        label,
                        destination.display(),
                        ancestor.display(),
                        error
                    ),
                });
            }
        };

        let metadata = if entry.file_type().is_symlink() {
            match fs::metadata(ancestor) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(VdlError::Other {
                        message: format!(
                            "{} output '{}' is blocked by existing non-directory parent '{}' (dangling symlink)",
                            label,
                            destination.display(),
                            ancestor.display()
                        ),
                    });
                }
                Err(error) => {
                    return Err(VdlError::Io {
                        message: format!(
                            "{} output '{}' could not resolve parent symlink '{}': {}",
                            label,
                            destination.display(),
                            ancestor.display(),
                            error
                        ),
                    });
                }
            }
        } else {
            entry
        };

        if !metadata.is_dir() {
            return Err(VdlError::Other {
                message: format!(
                    "{} output '{}' is blocked by existing non-directory parent '{}'",
                    label,
                    destination.display(),
                    ancestor.display()
                ),
            });
        }
    }

    Ok(())
}

fn check_symlink_effective_parent(
    label: &str,
    destination: &Path,
    effective_destination: &Path,
) -> VdlResult<()> {
    let link_metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(VdlError::Io {
                message: format!(
                    "{} output '{}' could not inspect destination: {}",
                    label,
                    destination.display(),
                    error
                ),
            });
        }
    };
    if !link_metadata.file_type().is_symlink() {
        return Ok(());
    }

    let Some(effective_parent) = effective_destination.parent() else {
        return Err(VdlError::Other {
            message: format!(
                "{} output symlink '{}' has no effective parent",
                label,
                destination.display()
            ),
        });
    };
    let metadata = match fs::metadata(effective_parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(VdlError::Other {
                message: format!(
                    "{} output symlink '{}' resolves to missing effective parent '{}'",
                    label,
                    destination.display(),
                    effective_parent.display()
                ),
            });
        }
        Err(error) => {
            return Err(VdlError::Io {
                message: format!(
                    "{} output symlink '{}' could not inspect effective parent '{}': {}",
                    label,
                    destination.display(),
                    effective_parent.display(),
                    error
                ),
            });
        }
    };
    if !metadata.is_dir() {
        return Err(VdlError::Other {
            message: format!(
                "{} output symlink '{}' has non-directory effective parent '{}'",
                label,
                destination.display(),
                effective_parent.display()
            ),
        });
    }

    Ok(())
}

/// Validate a target destination and return its canonical anchor.
///
/// Shared by Markdown and graph targets: the destination must stay inside
/// `out_dir` (symlinks included), must not alias a project source, the
/// manifest, or a previously planned target, and must not resolve to the
/// `.vdl` source extension.
fn plan_destination(
    label: &str,
    output: &Path,
    out_dir: &Path,
    out_anchor: &Path,
    project: &SchemaProject,
    planned: &[PlannedTarget],
) -> VdlResult<PathBuf> {
    let destination = out_dir.join(output);
    if !destination.starts_with(out_dir) {
        return Err(VdlError::Other {
            message: format!(
                "{} output '{}' escapes out_dir '{}'",
                label,
                output.display(),
                out_dir.display()
            ),
        });
    }
    check_target_parent_directories(label, &destination)?;
    if destination.is_dir() {
        return Err(VdlError::Other {
            message: format!(
                "{} output '{}' is an existing directory; expected a file path",
                label,
                destination.display()
            ),
        });
    }

    let destination_anchor = canonical_anchor(&destination).map_err(|e| VdlError::Io {
        message: format!(
            "Failed to resolve {} output '{}' at '{}': {}",
            label,
            output.display(),
            destination.display(),
            e
        ),
    })?;
    if destination_anchor.extension().and_then(|ext| ext.to_str()) == Some("vdl") {
        return Err(VdlError::Other {
            message: format!(
                "{} output '{}' resolves to '{}' with the .vdl source extension; choose a non-.vdl output path",
                label,
                output.display(),
                destination_anchor.display()
            ),
        });
    }
    if !destination_anchor.starts_with(out_anchor) {
        return Err(VdlError::Other {
            message: format!(
                "{} output '{}' resolves outside out_dir '{}' (symlink target '{}')",
                label,
                output.display(),
                out_dir.display(),
                destination_anchor.display()
            ),
        });
    }
    check_symlink_effective_parent(label, &destination, &destination_anchor)?;

    for source in &project.sources {
        let aliases = paths_alias(&destination, source).map_err(|e| VdlError::Io {
            message: format!(
                "Failed to compare build target {} with project source {}: {}",
                destination.display(),
                source.display(),
                e
            ),
        })?;
        if aliases {
            return Err(VdlError::Other {
                message: format!(
                    "refusing to write build output '{}' over project source '{}'",
                    destination.display(),
                    source.display()
                ),
            });
        }
    }
    if let Some(manifest_path) = &project.manifest_path {
        let aliases = paths_alias(&destination, manifest_path).map_err(|e| VdlError::Io {
            message: format!(
                "Failed to compare build target {} with the build manifest {}: {}",
                destination.display(),
                manifest_path.display(),
                e
            ),
        })?;
        if aliases {
            return Err(VdlError::Other {
                message: format!(
                    "refusing to write build output '{}' over the build manifest '{}'",
                    destination.display(),
                    manifest_path.display()
                ),
            });
        }
    }

    for previous in planned {
        let overlaps = destination_anchor.starts_with(&previous.anchor)
            || previous.anchor.starts_with(&destination_anchor);
        let aliases = paths_alias(&destination, &previous.destination).map_err(|e| VdlError::Io {
            message: format!(
                "Failed to compare build targets {} and {}: {}",
                destination.display(),
                previous.destination.display(),
                e
            ),
        })?;
        if overlaps || aliases {
            return Err(VdlError::Other {
                message: format!(
                    "{} output '{}' collides with another target ('{}'); resolved targets '{}' and '{}' overlap or alias",
                    label,
                    destination.display(),
                    previous.destination.display(),
                    destination_anchor.display(),
                    previous.anchor.display()
                ),
            });
        }
    }

    Ok(destination_anchor)
}

/// Render one graph artifact from the whole project document.
fn render_graph(doc: &SchemaDoc, format: GraphFormat) -> String {
    let graph = crate::graph::Graph::build(doc);
    match format {
        GraphFormat::Json => crate::graph::json::to_json(&graph),
        GraphFormat::Dot => crate::graph::dot::to_dot(&graph),
        GraphFormat::Index => {
            crate::graph::search::to_json(&crate::graph::search::build_index(doc))
        }
    }
}

/// Render `to` as a Markdown link target relative to `from_dir`, with
/// forward slashes and spaces percent-encoded.
fn relative_link(from_dir: &Path, to: &Path) -> String {
    let from: Vec<_> = from_dir.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts: Vec<String> = std::iter::repeat("..".to_string())
        .take(from.len().saturating_sub(common))
        .collect();
    parts.extend(
        to_components[common.min(to_components.len())..]
            .iter()
            .map(|component| component.as_os_str().to_string_lossy().to_string()),
    );
    if parts.is_empty() {
        parts.push(
            to.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
    }
    parts.join("/").replace(' ', "%20")
}

/// Compile the schema project rooted at `root` using its `vdl.toml` targets.
pub fn build_project(root: &Path) -> VdlResult<BuildReport> {
    let project = load_schema_project(root)?;
    let manifest = load_manifest(&project.root)?.ok_or_else(|| VdlError::Other {
        message: missing_manifest_error(&project.root),
    })?;

    let project_anchor = canonical_anchor(&project.root).map_err(|e| VdlError::Io {
        message: format!(
            "Failed to resolve project directory {}: {}",
            project.root.display(),
            e
        ),
    })?;
    let out_dir = project.root.join(&manifest.out_dir);
    let out_anchor = canonical_anchor(&out_dir).map_err(|e| VdlError::Io {
        message: format!(
            "Failed to resolve output directory {}: {}",
            out_dir.display(),
            e
        ),
    })?;
    if !out_anchor.starts_with(&project_anchor) {
        return Err(VdlError::Other {
            message: format!(
                "build.out_dir '{}' resolves outside the project '{}' (resolved to '{}')",
                manifest.out_dir.display(),
                project.root.display(),
                out_anchor.display()
            ),
        });
    }

    let mut rendered: Vec<PlannedTarget> = Vec::new();

    for (index, target) in manifest.markdown.iter().enumerate() {
        let label = format!("build.markdown[{}]", index + 1);

        if project.doc.instance(&target.root).is_none() {
            return Err(VdlError::Other {
                message: format!(
                    "{} root '{}' is not an entity in project '{}'; declare it in a .vdl source or fix the manifest",
                    label,
                    target.root,
                    project.root.display()
                ),
            });
        }

        let destination = out_dir.join(&target.output);
        let destination_anchor = plan_destination(
            &label,
            &target.output,
            &out_dir,
            &out_anchor,
            &project,
            &rendered,
        )?;

        let cite_links = target
            .cite_links
            .then(|| {
                let links: HashMap<String, String> = manifest
                    .markdown
                    .iter()
                    .map(|other| {
                        let other_destination = out_dir.join(&other.output);
                        (
                            other.root.clone(),
                            relative_link(
                                destination.parent().unwrap_or(&out_dir),
                                &other_destination,
                            ),
                        )
                    })
                    .collect();
                Rc::new(links)
            });

        let markdown = compose_with_options(
            &project.doc,
            &target.root,
            &ComposeOptions {
                cites: target.cites,
                number_headings: target.number_headings,
                cite_links,
            },
        )
        .map_err(|e| VdlError::Other {
            message: format!("{} root '{}': {}", label, target.root, e),
        })?;
        rendered.push(PlannedTarget {
            destination,
            anchor: destination_anchor,
            contents: markdown,
        });
    }

    for (index, target) in manifest.graph.iter().enumerate() {
        let label = format!("build.graph[{}]", index + 1);
        let destination = out_dir.join(&target.output);
        let destination_anchor = plan_destination(
            &label,
            &target.output,
            &out_dir,
            &out_anchor,
            &project,
            &rendered,
        )?;

        rendered.push(PlannedTarget {
            destination,
            anchor: destination_anchor,
            contents: render_graph(&project.doc, target.format),
        });
    }

    for (index, target) in manifest.toc.iter().enumerate() {
        let label = format!("build.toc[{}]", index + 1);
        if project.doc.instance(&target.root).is_none() {
            return Err(VdlError::Other {
                message: format!(
                    "{} root '{}' is not an entity in project '{}'; declare it in a .vdl source or fix the manifest",
                    label,
                    target.root,
                    project.root.display()
                ),
            });
        }
        let destination = out_dir.join(&target.output);
        let destination_anchor = plan_destination(
            &label,
            &target.output,
            &out_dir,
            &out_anchor,
            &project,
            &rendered,
        )?;

        let contents = crate::codegen::compose::compose_toc(
            &project.doc,
            &target.root,
            &ComposeOptions {
                cites: crate::codegen::compose::CiteMode::None,
                number_headings: target.number_headings,
                cite_links: None,
            },
        )
        .map_err(|e| VdlError::Other {
            message: format!("{} root '{}': {}", label, target.root, e),
        })?;
        rendered.push(PlannedTarget {
            destination,
            anchor: destination_anchor,
            contents,
        });
    }

    let mut outputs = Vec::with_capacity(rendered.len());
    for target in rendered {
        if let Some(parent) = target.destination.parent() {
            fs::create_dir_all(parent).map_err(|e| VdlError::Io {
                message: format!(
                    "Failed to create output directory {}: {}",
                    parent.display(),
                    e
                ),
            })?;
        }
        fs::write(&target.destination, target.contents).map_err(|e| VdlError::Io {
            message: format!("Failed to write {}: {}", target.destination.display(), e),
        })?;
        outputs.push(target.destination);
    }

    Ok(BuildReport { outputs })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"
schema "s" {
    type "Doc" {
        field "label" text heading
    }
}
"#;

    const DOC: &str = r#"entity "Doc" "a" {
    field "label" "A"
}
"#;

    fn write(root: &Path, name: &str, contents: &str) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, contents).expect("write source");
    }

    #[test]
    fn writes_every_registered_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(root, "doc.vdl", DOC);
        write(
            root,
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "a"
output = "one/a.md"

[[build.markdown]]
root = "a"
output = "two/a.md"
"#,
        );

        let report = build_project(root).expect("build succeeds");
        assert_eq!(report.outputs.len(), 2);
        assert!(root.join("dist/one/a.md").is_file());
        assert!(root.join("dist/two/a.md").is_file());
    }

    #[test]
    fn collision_is_rejected_without_writing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(root, "doc.vdl", DOC);
        write(
            root,
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "a"
output = "same.md"

[[build.markdown]]
root = "a"
output = "./same.md"
"#,
        );

        let err = build_project(root).unwrap_err().to_string();
        assert!(err.contains("collides"), "{}", err);
        assert!(!root.join("dist/same.md").exists());
    }

    #[test]
    fn unknown_root_is_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(root, "doc.vdl", DOC);
        write(
            root,
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "missing"
output = "missing.md"
"#,
        );

        let err = build_project(root).unwrap_err().to_string();
        assert!(err.contains("missing"), "{}", err);
        assert!(!root.join("dist/missing.md").exists());
    }

    const GRAPH_MANIFEST: &str = r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "a"
output = "a.md"

[[build.graph]]
format = "json"
output = "graph.json"

[[build.graph]]
format = "dot"
output = "graph.dot"

[[build.graph]]
format = "index"
output = "search.json"
"#;

    #[test]
    fn graph_targets_are_written_alongside_markdown() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(root, "doc.vdl", DOC);
        write(root, "vdl.toml", GRAPH_MANIFEST);

        let report = build_project(root).expect("build succeeds");
        assert_eq!(report.outputs.len(), 4);

        let json = fs::read_to_string(root.join("dist/graph.json")).expect("graph.json written");
        assert!(json.contains("\"nodes\""), "{json}");
        assert!(json.contains("\"id\": \"a\""), "{json}");
        let dot = fs::read_to_string(root.join("dist/graph.dot")).expect("graph.dot written");
        assert!(dot.contains("digraph VDL_Graph {"), "{dot}");
        let index = fs::read_to_string(root.join("dist/search.json")).expect("search.json written");
        assert!(index.contains("\"content\""), "{index}");
    }

    #[test]
    fn graph_target_colliding_with_markdown_target_is_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(root, "doc.vdl", DOC);
        write(
            root,
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "a"
output = "same.md"

[[build.graph]]
format = "json"
output = "./same.md"
"#,
        );

        let err = build_project(root).unwrap_err().to_string();
        assert!(err.contains("collides"), "{}", err);
        assert!(err.contains("build.graph[1]"), "{}", err);
        assert!(!root.join("dist/same.md").exists());
    }

    #[test]
    fn graph_output_vdl_extension_is_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(root, "doc.vdl", DOC);
        write(
            root,
            "vdl.toml",
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "a"
output = "a.md"

[[build.graph]]
format = "json"
output = "generated.vdl"
"#,
        );

        let err = build_project(root).unwrap_err().to_string();
        assert!(err.contains("build.graph[1]"), "{}", err);
        assert!(err.contains(".vdl source extension"), "{}", err);
        assert!(!root.join("dist/a.md").exists());
    }
}
