//! Discover `.vdl` sources and merge them into one validated schema project.
//!
//! File names and folder layout carry no meaning. Entity IDs are project-global,
//! and exactly one file may declare the `schema` block. Every AST node keeps
//! the file, line, and column it was parsed from, so a cross-file error points
//! at the source that caused it.

use crate::error::{SourceLocation, VdlError, VdlResult};
use crate::lexer;
use crate::lexer::token::Token;
use crate::model::{Instance, Schema, SchemaDoc};
use crate::parser::schema::{declares_schema, first_declaration, parse_schema_fragment};
use crate::validator;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::manifest::MANIFEST_FILE;

/// A loaded, validated schema project.
#[derive(Debug, Clone)]
pub struct SchemaProject {
    /// Directory that contains the project sources.
    pub root: PathBuf,
    pub doc: SchemaDoc,
    /// Every `.vdl` source that contributed, sorted for determinism.
    pub sources: Vec<PathBuf>,
    /// `<root>/vdl.toml` when it exists.
    pub manifest_path: Option<PathBuf>,
}

impl SchemaProject {
    /// Every file that must never be overwritten by generated output.
    pub fn protected_files(&self) -> impl Iterator<Item = &Path> {
        self.sources
            .iter()
            .map(PathBuf::as_path)
            .chain(self.manifest_path.as_deref())
    }
}

/// Collect `.vdl` files from a file or directory, sorted.
pub fn discover_vdl_files(path: &Path) -> VdlResult<Vec<PathBuf>> {
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("vdl") {
            Ok(vec![path.to_path_buf()])
        } else {
            Err(VdlError::Other {
                message: format!("Path is not a .vdl file: {}", path.display()),
            })
        }
    } else if path.is_dir() {
        let mut files = Vec::new();
        collect_recursive(path, &mut files).map_err(|e| VdlError::Other {
            message: format!("Failed to scan directory: {}", e),
        })?;
        files.sort();
        Ok(files)
    } else {
        Err(VdlError::Other {
            message: format!("Path does not exist: {}", path.display()),
        })
    }
}

fn collect_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_recursive(&path, files)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("vdl") {
            files.push(path);
        }
    }
    Ok(())
}

/// Load and validate every source under `path` as one schema project.
///
/// # Errors
///
/// * no `.vdl` files, no schema block, or more than one schema block;
/// * a mix of schema documents and legacy entity documents;
/// * any schema validation error, reported at its original location.
pub fn load_schema_project(path: &Path) -> VdlResult<SchemaProject> {
    let files = discover_vdl_files(path)?;
    if files.is_empty() {
        return Err(VdlError::Other {
            message: format!("No .vdl files found in '{}'.", path.display()),
        });
    }

    let root = if path.is_file() {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    } else {
        path.to_path_buf()
    };

    let manifest = root.join(MANIFEST_FILE);
    let manifest_path = manifest.is_file().then_some(manifest);

    let mut sources = Vec::with_capacity(files.len());
    let mut declared_types = HashSet::new();
    for file in &files {
        let source = read_source(file)?;
        let tokens = lexer::lex(&source, file)?;
        // Resolve the schema before classifying files whose bare type names
        // can also start legacy declarations, regardless of file order.
        let schema_file = if declares_schema(&tokens) {
            let fragment = parse_schema_fragment(&tokens, &source, file)?;
            if let Some(schema) = &fragment.schema {
                declared_types.extend(schema.types.iter().map(|decl| decl.name.clone()));
            }
            Some(fragment)
        } else {
            None
        };
        sources.push((file, source, tokens, schema_file));
    }

    let mut schema: Option<Schema> = None;
    let mut schema_locations: Vec<SourceLocation> = Vec::new();
    let mut entities: Vec<Instance> = Vec::new();
    let mut imports = Vec::new();
    let mut legacy_files: Vec<PathBuf> = Vec::new();

    for (file, source, tokens, schema_file) in sources {
        let fragment = match schema_file {
            Some(fragment) => fragment,
            None => {
                let declared_shorthand = first_declaration(&tokens)
                    .and_then(Token::word)
                    .is_some_and(|name| declared_types.contains(name));
                if !declared_shorthand && crate::parser::is_legacy_document(&tokens) {
                    legacy_files.push(file.clone());
                    continue;
                }
                parse_schema_fragment(&tokens, &source, file)?
            }
        };

        imports.extend(fragment.imports.iter().cloned());

        if let Some(parsed) = fragment.schema {
            schema_locations.push(parsed.source_location.clone());
            if schema.is_none() {
                schema = Some(parsed);
            }
        }
        entities.extend(fragment.entities);
    }

    if schema_locations.len() > 1 {
        let locations = schema_locations
            .iter()
            .map(|location| format!("  {}", location))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(VdlError::Other {
            message: format!(
                "project '{}' declares {} schema blocks; exactly one schema block is allowed across the project. Found at:\n{}",
                root.display(),
                schema_locations.len(),
                locations
            ),
        });
    }

    if !legacy_files.is_empty() {
        return Err(VdlError::Other {
            message: format!(
                "legacy form is no longer supported; migrate these documents to schema form:\n{}",
                legacy_files.iter().map(|file| format!("  {}", file.display())).collect::<Vec<_>>().join("\n")
            ),
        });
    }

    let Some(schema) = schema else {
        return Err(VdlError::Other {
            message: format!(
                "no schema block found in project '{}'; a schema document must declare exactly one `schema \"...\"` block.",
                root.display()
            ),
        });
    };

    let mut doc = SchemaDoc { schema, entities, imports };
    resolve_import_references(&mut doc)?;
    validator::schema::validate_schema(&doc)?;

    Ok(SchemaProject {
        root,
        doc,
        sources: files,
        manifest_path,
    })
}

pub(crate) fn resolve_import_references(doc: &mut SchemaDoc) -> VdlResult<()> {
    let all_ids = doc.entities.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
    let mut errors = Vec::new();
    for import in &doc.imports {
        let found = if import.wildcard {
            let prefix = format!("{}.", import.target);
            all_ids.iter().any(|id| id.starts_with(&prefix) && !id[prefix.len()..].contains('.'))
        } else { all_ids.iter().any(|id| id == &import.target) };
        if !found {
            let target = if import.wildcard { format!("{}.*", import.target) } else { import.target.clone() };
            errors.push(format!("{}: imported target '{}' was not found", import.source_location, target));
        }
    }
    for entity in &mut doc.entities {
        for value in &mut entity.values {
            let ids: Vec<&mut String> = match &mut value.value {
                crate::model::Value::Refs(ids) => ids.iter_mut().collect(),
                crate::model::Value::Inline(items) => items.iter_mut().filter_map(|item| match item {
                    crate::model::InlineItem::Ref(id) => Some(id),
                    _ => None,
                }).collect(),
                _ => Vec::new(),
            };
            for (index, id) in ids.into_iter().enumerate() {
                if !value.bare_ref_indices.contains(&index) { continue; }
                let name = id.as_str();
                let chosen = crate::model::resolve_short_name(
                    &entity.imports,
                    entity.package.as_deref(),
                    name,
                    |id| all_ids.iter().any(|candidate| candidate == id),
                );
                match chosen.as_slice() {
                    [one] => *id = one.clone(),
                    [] => errors.push(format!("{}: bare reference '{}' has no matching local entity or import", value.source_location, name)),
                    many => errors.push(format!("{}: bare reference '{}' is ambiguous; candidates: {}", value.source_location, name, many.join(", "))),
                }
            }
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(VdlError::Other { message: errors.join("\n") }) }
}

fn read_source(path: &Path) -> VdlResult<String> {
    fs::read_to_string(path).map_err(|e| VdlError::Io {
        message: format!("Failed to read {}: {}", path.display(), e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn write(root: &Path, name: &str, contents: &str) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, contents).expect("write source");
    }

    #[test]
    fn merges_instances_from_nested_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(
            root,
            "a/nested/run.vdl",
            r#"entity "Runbook" "r" {
    field "label" "R"
    field "chapters" [ "s" ]
}
"#,
        );
        write(
            root,
            "b/section.vdl",
            r#"entity "Section" "s" {
    field "label" "S"
    field "content" """Body."""
}
"#,
        );

        let project = load_schema_project(root).expect("project loads");
        assert_eq!(project.doc.entities.len(), 2);
        assert!(project.doc.instance("r").is_some());
        assert!(project.doc.instance("s").is_some());
        assert_eq!(project.sources.len(), 3);
    }

    #[test]
    fn schema_may_live_in_any_file_name() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "types.vdl", SCHEMA);
        write(
            root,
            "run.vdl",
            r#"entity "Runbook" "r" {
    field "label" "R"
    field "chapters" [ ]
}
"#,
        );

        let project = load_schema_project(root).expect("project loads");
        assert_eq!(project.doc.instance("r").unwrap().type_name, "Runbook");
    }

    #[test]
    fn duplicate_ids_across_files_name_both_locations() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(
            root,
            "one.vdl",
            r#"entity "Section" "dup" {
    field "label" "One"
    field "content" """One."""
}
"#,
        );
        write(
            root,
            "two.vdl",
            r#"entity "Section" "dup" {
    field "label" "Two"
    field "content" """Two."""
}
"#,
        );

        let err = load_schema_project(root).unwrap_err().to_string();
        assert!(err.contains("duplicate entity ID 'dup'"), "{}", err);
        assert!(err.contains("one.vdl"), "{}", err);
        assert!(err.contains("two.vdl"), "{}", err);
    }

    #[test]
    fn forward_reference_resolves_across_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", SCHEMA);
        write(
            root,
            "a-run.vdl",
            r#"entity "Runbook" "r" {
    field "label" "R"
    field "chapters" [ "later" ]
}
"#,
        );
        write(
            root,
            "z-section.vdl",
            r#"entity "Section" "later" {
    field "label" "Later"
    field "content" """Later body."""
}
"#,
        );

        assert!(load_schema_project(root).is_ok());
    }

    #[test]
    fn missing_schema_is_reported() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(
            root,
            "only.vdl",
            r#"entity "Section" "s" {
    field "label" "S"
    field "content" """Body."""
}
"#,
        );

        let err = load_schema_project(root).unwrap_err().to_string();
        assert!(err.contains("schema document"), "{}", err);
    }

    #[test]
    fn two_schema_blocks_are_reported_with_both_locations() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "a.vdl", SCHEMA);
        write(root, "nested/b.vdl", SCHEMA);

        let err = load_schema_project(root).unwrap_err().to_string();
        assert!(err.contains("2 schema blocks"), "{}", err);
        assert!(err.contains("a.vdl"), "{}", err);
        assert!(err.contains("b.vdl"), "{}", err);
    }

    #[test]
    fn imports_resolve_explicit_alias_and_one_level_wildcard() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", r#"schema "s" {
    type "Page" { field "title" text heading field "items" ref "Artifact" compose optional }
    type "Artifact" { field "title" text heading field "body" body }
}"#);
        write(root, "page.vdl", r#"package site
import content.*
import revelation.ar-rahman as verse
Page main { title "Main" items [topic, verse] }
"#);
        write(root, "topic.vdl", r#"Artifact "content.topic" { title "Topic" body """T""" }
Artifact "revelation.ar-rahman" { title "Verse" body """V""" }
"#);
        let project = load_schema_project(root).expect("imports resolve");
        assert_eq!(project.doc.instance("site.main").unwrap().value("items"), Some(&crate::model::Value::Refs(vec!["content.topic".into(), "revelation.ar-rahman".into()])));
        fs::create_dir_all(root.join("moved")).unwrap();
        fs::rename(root.join("page.vdl"), root.join("moved/page.vdl")).unwrap();
        let moved = load_schema_project(root).expect("imports survive source relocation");
        assert_eq!(moved.doc.instance("site.main").unwrap().value("items"), project.doc.instance("site.main").unwrap().value("items"));
    }

    #[test]
    fn ambiguous_wildcard_and_missing_explicit_imports_are_located() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "schema.vdl", r#"schema "s" {
    type "Page" { field "title" text heading field "items" ref "Artifact" optional }
    type "Artifact" { field "title" text heading field "body" body }
}"#);
        write(root, "page.vdl", r#"import first.*
import second.*
import absent.target as missing
Page main { title "Main" items [topic] }
"#);
        write(root, "items.vdl", r#"Artifact "first.topic" { title "First" body """1""" }
Artifact "second.topic" { title "Second" body """2""" }
"#);
        let err = load_schema_project(root).unwrap_err().to_string();
        assert!(err.contains("page.vdl:"), "{err}");
        assert!(err.contains("ambiguous"), "{err}");
        assert!(err.contains("absent.target"), "{err}");
    }

    #[test]
    fn nonexistent_import_in_schema_only_file_is_still_reported() {
        let temp = tempfile::tempdir().expect("tempdir");
        write(temp.path(), "schema.vdl", r#"import absent.entity as missing
schema "s" { type "T" { field "title" text heading } }
"#);
        let err = load_schema_project(temp.path()).unwrap_err().to_string();
        assert!(err.contains("schema.vdl:1:"), "{err}");
        assert!(err.contains("absent.entity"), "{err}");
    }
}
