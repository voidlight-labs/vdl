//! Validate every build target, render all Markdown, then write.
//!
//! Nothing is written until every target has been checked: the root exists,
//! the destination stays inside `out_dir` (symlinks included), it does not
//! alias a project source or the manifest, and it does not collide with
//! another target. A validation failure therefore leaves existing output
//! untouched.

use crate::codegen::compose::compose_with_numbering;
use crate::error::{VdlError, VdlResult};
use crate::fsutil::{canonical_anchor, paths_alias};
use std::fs;
use std::path::{Path, PathBuf};

use super::loader::load_schema_project;
use super::manifest::{load_manifest, missing_manifest_error};

/// What a successful build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub outputs: Vec<PathBuf>,
}

struct PlannedTarget {
    destination: PathBuf,
    anchor: PathBuf,
    markdown: String,
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
        if !destination.starts_with(&out_dir) {
            return Err(VdlError::Other {
                message: format!(
                    "{} output '{}' escapes out_dir '{}'",
                    label,
                    target.output.display(),
                    out_dir.display()
                ),
            });
        }
        check_target_parent_directories(&label, &destination)?;
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
                target.output.display(),
                destination.display(),
                e
            ),
        })?;
        if destination_anchor.extension().and_then(|ext| ext.to_str()) == Some("vdl") {
            return Err(VdlError::Other {
                message: format!(
                    "{} output '{}' resolves to '{}' with the .vdl source extension; choose a non-.vdl output path",
                    label,
                    target.output.display(),
                    destination_anchor.display()
                ),
            });
        }
        if !destination_anchor.starts_with(&out_anchor) {
            return Err(VdlError::Other {
                message: format!(
                    "{} output '{}' resolves outside out_dir '{}' (symlink target '{}')",
                    label,
                    target.output.display(),
                    out_dir.display(),
                    destination_anchor.display()
                ),
            });
        }
        check_symlink_effective_parent(&label, &destination, &destination_anchor)?;

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

        for previous in &rendered {
            let overlaps = destination_anchor.starts_with(&previous.anchor)
                || previous.anchor.starts_with(&destination_anchor);
            let aliases =
                paths_alias(&destination, &previous.destination).map_err(|e| VdlError::Io {
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

        let markdown = compose_with_numbering(&project.doc, &target.root, target.number_headings)
            .map_err(|e| VdlError::Other {
                message: format!("{} root '{}': {}", label, target.root, e),
            })?;
        rendered.push(PlannedTarget {
            destination,
            anchor: destination_anchor,
            markdown,
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
        fs::write(&target.destination, target.markdown).map_err(|e| VdlError::Io {
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
}
