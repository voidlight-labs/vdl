//! `vdl.toml`: the build manifest of a schema project.
//!
//! ```toml
//! [build]
//! out_dir = "dist"
//!
//! [[build.markdown]]
//! root = "deploy"
//! output = "runbooks/deploy.md"
//! cites = "short"
//!
//! [[build.graph]]
//! format = "json"
//! output = "graph.json"
//! ```
//!
//! `root` is an entity ID; `output` is relative to `out_dir`, and `out_dir` is
//! relative to the project root. Paths are normalized and checked here so the
//! build never has to guess whether a target can leave the output directory.
//! The optional per-target `cites` key (`none`, `link`, `short`; default
//! `none`) chooses how `cite`-flagged reference fields render at their cite
//! points; `short` renders each target's heading-field text, `link` its ID.
//! `[[build.graph]]` entries are optional graph artifacts (`format` is
//! `json`, `dot`, or `index`) rendered from the whole project; at least one
//! `[[build.markdown]]` target is always required.

use crate::codegen::compose::CiteMode;
use crate::error::{VdlError, VdlResult};
use crate::graph::GraphFormat;
use serde::Deserialize;
use std::fmt::Display;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const MANIFEST_FILE: &str = "vdl.toml";

pub const MANIFEST_EXAMPLE: &str = r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/deploy.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/recovery.md"

[[build.graph]]
format = "json"
output = "graph.json"
"#;

/// A parsed and path-checked build manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Relative to the project root. May be empty, meaning the root itself.
    pub out_dir: PathBuf,
    pub markdown: Vec<MarkdownTarget>,
    /// Optional graph artifacts, rendered after the Markdown targets.
    pub graph: Vec<GraphTarget>,
}

/// One `[[build.markdown]]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownTarget {
    pub root: String,
    /// Relative to [`Manifest::out_dir`], normalized.
    pub output: PathBuf,
    /// Prefix composed headings with their position in this output.
    pub number_headings: bool,
    /// How `cite`-flagged reference fields render at their cite points.
    pub cites: CiteMode,
}

/// One `[[build.graph]]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphTarget {
    /// Which serialization the artifact holds.
    pub format: GraphFormat,
    /// Relative to [`Manifest::out_dir`], normalized.
    pub output: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    build: Option<RawBuild>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuild {
    out_dir: Option<String>,
    markdown: Option<Vec<RawTarget>>,
    graph: Option<Vec<RawGraphTarget>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTarget {
    root: Option<String>,
    output: Option<String>,
    #[serde(default)]
    number_headings: bool,
    #[serde(default)]
    cites: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGraphTarget {
    format: Option<String>,
    output: Option<String>,
}

/// Map the manifest's `cites` key onto [`CiteMode`], rejecting unknown values
/// with a manifest-path diagnostic that names the allowed ones.
fn parse_cite_mode(raw: Option<&str>, label: &str, path: &Path) -> VdlResult<CiteMode> {
    match raw.map(str::trim) {
        None | Some("") => Ok(CiteMode::None),
        Some("none") => Ok(CiteMode::None),
        Some("link") => Ok(CiteMode::Link),
        Some("short") => Ok(CiteMode::Short),
        Some(other) => Err(invalid(
            path,
            format!("{} cites must be one of \"none\", \"link\", \"short\"; found \"{}\"", label, other),
        )),
    }
}

fn invalid(path: &Path, detail: impl Display) -> VdlError {
    VdlError::Other {
        message: format!(
            "invalid build manifest '{}': {}\n\nExpected a manifest like:\n\n{}",
            path.display(),
            detail,
            MANIFEST_EXAMPLE
        ),
    }
}

/// The message shown when a schema project is compiled without a manifest.
pub fn missing_manifest_error(root: &Path) -> String {
    format!(
        "no build manifest found at '{}'; a schema project is compiled from a manifest.\n\nCreate '{}' with:\n\n{}",
        root.join(MANIFEST_FILE).display(),
        root.join(MANIFEST_FILE).display(),
        MANIFEST_EXAMPLE
    )
}

/// Load `<root>/vdl.toml`, or `None` when it does not exist.
pub fn load_manifest(root: &Path) -> VdlResult<Option<Manifest>> {
    let path = root.join(MANIFEST_FILE);
    if !path.is_file() {
        return Ok(None);
    }

    let text = fs::read_to_string(&path).map_err(|e| VdlError::Io {
        message: format!("Failed to read {}: {}", path.display(), e),
    })?;

    let raw: RawManifest = toml::from_str(&text).map_err(|e| invalid(&path, e))?;

    let build = raw
        .build
        .ok_or_else(|| invalid(&path, "missing [build] table"))?;

    let raw_out_dir = match build.out_dir {
        Some(value) if !value.trim().is_empty() => value,
        Some(_) => {
            return Err(invalid(&path, "[build] out_dir must not be empty"));
        }
        None => {
            return Err(invalid(&path, "[build] is missing out_dir"));
        }
    };
    let out_dir = relative_path(&raw_out_dir, "build.out_dir", &path, true)?;

    let raw_targets = match build.markdown {
        Some(targets) if !targets.is_empty() => targets,
        Some(_) => {
            return Err(invalid(
                &path,
                "no Markdown targets registered; add at least one [[build.markdown]] entry",
            ));
        }
        None => {
            return Err(invalid(
                &path,
                "no Markdown targets registered; add at least one [[build.markdown]] entry",
            ));
        }
    };

    let mut markdown = Vec::with_capacity(raw_targets.len());
    for (index, target) in raw_targets.into_iter().enumerate() {
        let label = format!("build.markdown[{}]", index + 1);

        let root = match target.root {
            Some(root) if !root.trim().is_empty() => root,
            Some(_) => {
                return Err(invalid(&path, format!("{} root must not be empty", label)));
            }
            None => {
                return Err(invalid(&path, format!("{} is missing root", label)));
            }
        };

        let raw_output = match target.output {
            Some(output) if !output.trim().is_empty() => output,
            Some(_) => {
                return Err(invalid(
                    &path,
                    format!("{} output must not be empty", label),
                ));
            }
            None => {
                return Err(invalid(&path, format!("{} is missing output", label)));
            }
        };
        let output = relative_path(&raw_output, &format!("{} output", label), &path, false)?;
        let cites = parse_cite_mode(target.cites.as_deref(), &label, &path)?;

        markdown.push(MarkdownTarget {
            root,
            output,
            number_headings: target.number_headings,
            cites,
        });
    }

    let mut graph = Vec::with_capacity(build.graph.as_ref().map_or(0, Vec::len));
    for (index, target) in build.graph.unwrap_or_default().into_iter().enumerate() {
        let label = format!("build.graph[{}]", index + 1);

        let format = match target.format {
            Some(raw) if !raw.trim().is_empty() => GraphFormat::parse(&raw).ok_or_else(|| {
                invalid(
                    &path,
                    format!(
                        "{} format must be one of \"json\", \"dot\", \"index\"; found \"{}\"",
                        label,
                        raw.trim()
                    ),
                )
            })?,
            Some(_) => {
                return Err(invalid(&path, format!("{} format must not be empty", label)));
            }
            None => {
                return Err(invalid(&path, format!("{} is missing format", label)));
            }
        };

        let raw_output = match target.output {
            Some(output) if !output.trim().is_empty() => output,
            Some(_) => {
                return Err(invalid(
                    &path,
                    format!("{} output must not be empty", label),
                ));
            }
            None => {
                return Err(invalid(&path, format!("{} is missing output", label)));
            }
        };
        let output = relative_path(&raw_output, &format!("{} output", label), &path, false)?;

        graph.push(GraphTarget { format, output });
    }

    Ok(Some(Manifest { out_dir, markdown, graph }))
}

/// Normalize a manifest path and reject anything that leaves the project.
///
/// `allow_empty` lets `out_dir = "."` mean the project root while every
/// `output` still has to name a file.
fn relative_path(raw: &str, what: &str, manifest: &Path, allow_empty: bool) -> VdlResult<PathBuf> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(invalid(
            manifest,
            format!("{} must be relative, found '{}'", what, raw),
        ));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(invalid(
                        manifest,
                        format!("{} escapes the project directory: '{}'", what, raw),
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(invalid(
                    manifest,
                    format!("{} must be relative, found '{}'", what, raw),
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() && !allow_empty {
        return Err(invalid(
            manifest,
            format!("{} must name a file, found '{}'", what, raw),
        ));
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/deploy.md"
"#;

    fn write_manifest(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join(MANIFEST_FILE), contents).expect("write manifest");
        let root = temp.path().to_path_buf();
        (temp, root)
    }

    #[test]
    fn loads_out_dir_and_targets() {
        let (_temp, root) = write_manifest(VALID);
        let manifest = load_manifest(&root).unwrap().expect("manifest present");
        assert_eq!(manifest.out_dir, PathBuf::from("dist"));
        assert_eq!(manifest.markdown.len(), 1);
        assert_eq!(manifest.markdown[0].root, "deploy");
        assert_eq!(
            manifest.markdown[0].output,
            PathBuf::from("runbooks/deploy.md")
        );
    }

    #[test]
    fn absent_manifest_is_not_an_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(load_manifest(temp.path()).unwrap().is_none());
    }

    #[test]
    fn malformed_toml_is_rejected() {
        let (_temp, root) = write_manifest("[build\n");
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("invalid build manifest"), "{}", err);
        assert!(err.contains("vdl.toml"), "{}", err);
    }

    #[test]
    fn missing_out_dir_is_rejected() {
        let (_temp, root) = write_manifest(
            r#"[[build.markdown]]
root = "deploy"
output = "deploy.md"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("missing out_dir"), "{}", err);
    }

    #[test]
    fn escaping_out_dir_is_rejected() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "../elsewhere"

[[build.markdown]]
root = "deploy"
output = "deploy.md"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("escapes the project"), "{}", err);
    }

    #[test]
    fn escaping_output_is_rejected() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "../deploy.md"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("escapes the project"), "{}", err);
    }

    #[test]
    fn absolute_output_is_rejected() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "/tmp/deploy.md"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("must be relative"), "{}", err);
    }

    #[test]
    fn empty_out_dir_means_project_root() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "."

[[build.markdown]]
root = "deploy"
output = "deploy.md"
"#,
        );
        let manifest = load_manifest(&root).unwrap().expect("manifest present");
        assert!(manifest.out_dir.as_os_str().is_empty());
    }

    #[test]
    fn no_targets_is_rejected() {
        let (_temp, root) = write_manifest("[build]\nout_dir = \"dist\"\n");
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("no Markdown targets"), "{}", err);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let (_temp, root) = write_manifest("[build]\nout_dir = \"dist\"\nextra = true\n");
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("invalid build manifest"), "{}", err);
    }

    const VALID_GRAPH: &str = r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/deploy.md"

[[build.graph]]
format = "json"
output = "graph.json"

[[build.graph]]
format = "dot"
output = "./nested/graph.dot"
"#;

    #[test]
    fn graph_targets_are_parsed_and_normalized() {
        let (_temp, root) = write_manifest(VALID_GRAPH);
        let manifest = load_manifest(&root).unwrap().expect("manifest present");
        assert_eq!(manifest.graph.len(), 2);
        assert_eq!(manifest.graph[0].format, GraphFormat::Json);
        assert_eq!(manifest.graph[0].output, PathBuf::from("graph.json"));
        assert_eq!(manifest.graph[1].format, GraphFormat::Dot);
        assert_eq!(manifest.graph[1].output, PathBuf::from("nested/graph.dot"));
    }

    #[test]
    fn empty_graph_list_is_allowed() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"
graph = []

[[build.markdown]]
root = "deploy"
output = "deploy.md"
"#,
        );
        let manifest = load_manifest(&root).unwrap().expect("manifest present");
        assert!(manifest.graph.is_empty());
    }

    #[test]
    fn missing_graph_format_is_rejected() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "deploy.md"

[[build.graph]]
output = "graph.json"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("build.graph[1] is missing format"), "{}", err);
    }

    #[test]
    fn unknown_graph_format_is_rejected() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "deploy.md"

[[build.graph]]
format = "svg"
output = "graph.svg"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(
            err.contains("build.graph[1] format must be one of \"json\", \"dot\", \"index\""),
            "{}",
            err
        );
        assert!(err.contains("\"svg\""), "{}", err);
    }

    #[test]
    fn missing_graph_output_is_rejected() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "deploy.md"

[[build.graph]]
format = "json"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("build.graph[1] is missing output"), "{}", err);
    }

    #[test]
    fn graph_targets_do_not_satisfy_the_markdown_requirement() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"

[[build.graph]]
format = "json"
output = "graph.json"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("no Markdown targets"), "{}", err);
    }

    #[test]
    fn graph_unknown_keys_are_rejected() {
        let (_temp, root) = write_manifest(
            r#"[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "deploy.md"

[[build.graph]]
format = "json"
output = "graph.json"
root = "deploy"
"#,
        );
        let err = load_manifest(&root).unwrap_err().to_string();
        assert!(err.contains("invalid build manifest"), "{}", err);
    }
}
