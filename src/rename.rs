//! Reference rewrite: rename an entity ID across a project's sources.
//!
//! The plan locates every source occurrence of one entity ID — its
//! declaration, every reference-field value (quoted, dotted, or bare), and
//! braced-body `include` directives — as byte-span edits. The rewritten
//! model is validated in memory before anything is written, so a rename that
//! would break the project fails without touching a single file.
//!
//! Spelling rules: quoted and dotted references are rewritten to the quoted
//! new ID (verbatim, resolution-independent); bare references are rewritten
//! to the new ID's final segment, which only resolves correctly when the new
//! ID shares the reference's package context — the plan verifies that
//! expectation and refuses otherwise. Include directives with bare
//! short-name spellings are reported as warnings instead of rewritten.

use crate::error::{VdlError, VdlResult};
use crate::lexer::{self, token::Token};
use crate::model::{body_include_directives, resolve_include_candidates, resolve_short_name, SchemaDoc, Value};
use crate::project::SchemaProject;
use crate::validator::schema::validate_schema;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// One source splice: replace `start..end` with `replacement`.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceEdit {
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    /// What the edit rewrites, for the dry-run summary.
    pub kind: EditKind,
}

/// What a planned edit touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// The entity's own ID declaration.
    Declaration,
    /// A reference-field value pointing at the entity.
    Reference,
    /// An `include <id>` directive inside a braced body.
    Include,
}

/// Everything needed to apply one rename: edits grouped by file, plus the
/// file contents they were computed against.
#[derive(Debug, Clone)]
pub struct RenamePlan {
    pub old: String,
    pub new: String,
    pub edits: Vec<SourceEdit>,
    /// Non-fatal findings: bare include spellings that mention the entity
    /// but cannot be rewritten mechanically.
    pub warnings: Vec<String>,
    contents: HashMap<PathBuf, String>,
}

impl RenamePlan {
    /// Total number of edits across all files.
    pub fn total(&self) -> usize {
        self.edits.len()
    }

    /// Number of edits per file, sorted by path.
    pub fn per_file(&self) -> Vec<(PathBuf, usize)> {
        let mut counts: HashMap<&Path, usize> = HashMap::new();
        for edit in &self.edits {
            *counts.entry(edit.file.as_path()).or_default() += 1;
        }
        let mut files: Vec<(PathBuf, usize)> = counts
            .into_iter()
            .map(|(file, count)| (file.to_path_buf(), count))
            .collect();
        files.sort();
        files
    }

    /// Write every planned edit back to its file.
    pub fn apply(&self) -> VdlResult<()> {
        for (file, _) in self.per_file() {
            let content = self
                .contents
                .get(&file)
                .ok_or_else(|| VdlError::Io {
                    message: format!("planned file '{}' is missing from the rename plan", file.display()),
                })?
                .clone();
            let mut edits: Vec<&SourceEdit> = self
                .edits
                .iter()
                .filter(|edit| edit.file == file)
                .collect();
            edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));

            let mut result = content;
            for edit in edits {
                result.replace_range(edit.start..edit.end, &edit.replacement);
            }
            fs::write(&file, result).map_err(|e| VdlError::Io {
                message: format!("Failed to write {}: {}", file.display(), e),
            })?;
        }
        Ok(())
    }
}

/// Plan a rename without touching any file.
///
/// # Errors
///
/// [`VdlError::Other`] when the IDs are unusable, the entity does not exist,
/// the new ID is taken, or the rewritten project would no longer validate.
pub fn plan(project: &SchemaProject, old: &str, new: &str) -> VdlResult<RenamePlan> {
    let old = old.trim();
    let new = new.trim();
    if new.is_empty() || new.chars().any(|c| c.is_whitespace() || c == '"' || c == '\\') {
        return Err(VdlError::Other {
            message: format!("invalid new entity ID: \"{new}\""),
        });
    }
    if old == new {
        return Err(VdlError::Other {
            message: "the old and new entity IDs are identical".to_string(),
        });
    }
    if project.doc.instance(old).is_none() {
        return Err(VdlError::Other {
            message: format!("entity not found: \"{old}\""),
        });
    }
    if project.doc.instance(new).is_some() {
        return Err(VdlError::Other {
            message: format!("entity \"{new}\" already exists"),
        });
    }

    let simulated = simulate(&project.doc, old, new);
    let errors = collect_validation_errors(&simulated);
    if !errors.is_empty() {
        return Err(VdlError::Other {
            message: format!(
                "renaming \"{old}\" to \"{new}\" would break the project:\n{}",
                errors.join("\n")
            ),
        });
    }

    let mut edits: Vec<SourceEdit> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut contents: HashMap<PathBuf, String> = HashMap::new();

    // Read every source once, in deterministic order.
    for source in &project.sources {
        let content = fs::read_to_string(source).map_err(|e| VdlError::Io {
            message: format!("Failed to read {}: {}", source.display(), e),
        })?;
        contents.insert(source.clone(), content.clone());
        collect_include_edits(source, &content, old, new, &mut edits);
    }

    for entity in &project.doc.entities {
        let file = &entity.source_location.file;
        if entity.id == old {
            edits.push(SourceEdit {
                file: file.clone(),
                start: entity.id_span.start,
                end: entity.id_span.end,
                replacement: format!("\"{new}\""),
                kind: EditKind::Declaration,
            });
        }
        for value in &entity.values {
            for (index, id) in value.value.reference_ids().enumerate() {
                if id != old {
                    continue;
                }
                let Some(span) = value.ref_spans.get(index) else {
                    warnings.push(format!(
                        "{}:{}: reference to \"{}\" has no recorded span; rewrite it manually",
                        file.display(),
                        value.source_location.line,
                        old
                    ));
                    continue;
                };
                let bare = value.bare_ref_indices.contains(&index);
                let replacement = match bare {
                    false => format!("\"{new}\""),
                    true => {
                        let segment = new.rsplit('.').next().unwrap_or(new).to_string();
                        let expected = resolve_short_name(
                            &entity.imports,
                            entity.package.as_deref(),
                            &segment,
                            |id| simulated.instance(id).is_some(),
                        );
                        if expected != vec![new.to_string()] {
                            return Err(VdlError::Other {
                                message: format!(
                                    "{}:{}: bare reference to \"{old}\" cannot be rewritten to the bare spelling \"{segment}\" (it would resolve to {:?}, not \"{new}\"); rename it manually or use the quoted form",
                                    file.display(),
                                    value.source_location.line,
                                    expected
                                ),
                            });
                        }
                        segment
                    }
                };
                edits.push(SourceEdit {
                    file: file.clone(),
                    start: span.start,
                    end: span.end,
                    replacement,
                    kind: EditKind::Reference,
                });
            }
        }
    }

    collect_bare_include_warnings(&project.doc, old, &mut warnings);

    Ok(RenamePlan {
        old: old.to_string(),
        new: new.to_string(),
        edits,
        warnings,
        contents,
    })
}

/// The model after the rename, used to decide whether the project survives.
fn simulate(doc: &SchemaDoc, old: &str, new: &str) -> SchemaDoc {
    let mut doc = doc.clone();
    for entity in &mut doc.entities {
        if entity.id == old {
            entity.id = new.to_string();
        }
        for value in &mut entity.values {
            match &mut value.value {
                Value::Refs(ids) => {
                    for id in ids.iter_mut() {
                        if id == old {
                            *id = new.to_string();
                        }
                    }
                }
                Value::Inline(items) => {
                    for item in items.iter_mut() {
                        if let crate::model::InlineItem::Ref(id) = item {
                            if id == old {
                                *id = new.to_string();
                            }
                        }
                    }
                }
                Value::BracedBody(text) => {
                    *text = rewrite_body_includes(text, old, new);
                }
                _ => {}
            }
        }
    }
    doc
}

/// Rewrite exact `include <old>` lines inside a braced body.
fn rewrite_body_includes(body: &str, old: &str, new: &str) -> String {
    let directives = body_include_directives(body);
    if directives.is_empty() {
        return body.to_string();
    }
    let needle = format!("include {old}");
    let replacement = format!("include {new}");
    let mut lines: Vec<String> = body.lines().map(str::to_owned).collect();
    for (line_index, id) in directives {
        if id == old {
            if let Some(line) = lines.get_mut(line_index) {
                let trimmed = line.trim_start();
                if trimmed == needle
                    || trimmed.starts_with(&format!("{needle} "))
                    || trimmed.starts_with(&needle)
                {
                    *line = line.replacen(&needle, &replacement, 1);
                }
            }
        }
    }
    let mut result = lines.join("\n");
    if body.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn collect_validation_errors(doc: &SchemaDoc) -> Vec<String> {
    match validate_schema(doc) {
        Ok(()) => Vec::new(),
        Err(error) => vec![error.to_string()],
    }
}

/// Standalone `include <id>` lines outside fenced code, scanned over RAW
/// braced-body text. Unlike [`body_include_directives`] (which runs on
/// parse-dedented model bodies and expects Markdown block indentation), the
/// raw scan accepts any indentation, matching whatever the author wrote.
fn raw_include_directives(raw: &str) -> Vec<(usize, &str)> {
    let mut directives = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for (line_index, line) in raw.lines().enumerate() {
        let trimmed = line.trim_start();
        let bytes = trimmed.as_bytes();
        if bytes.first().is_some_and(|b| *b == b'`' || *b == b'~') {
            let marker = bytes[0];
            let length = bytes.iter().take_while(|b| **b == marker).count();
            if length >= 3 {
                match fence {
                    None => fence = Some((marker, length)),
                    Some((open, open_len)) if marker == open
                        && length >= open_len
                        && bytes[length..].iter().all(|b| b.is_ascii_whitespace()) =>
                    {
                        fence = None
                    }
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_none() {
            if let Some(id) = trimmed.strip_prefix("include ") {
                let id = id.trim();
                if !id.is_empty() {
                    directives.push((line_index, id));
                }
            }
        }
    }
    directives
}

/// Rewrite `include <old>` lines inside every braced body of one file by
/// scanning the source region each braced-body token covers, so directive
/// edits carry exact byte offsets even under CRLF line endings (the token
/// text itself is newline-normalized, the source region is not).
fn collect_include_edits(
    file: &Path,
    content: &str,
    old: &str,
    new: &str,
    edits: &mut Vec<SourceEdit>,
) {
    let Ok(tokens) = lexer::lex(content, file) else {
        return;
    };
    for (token, span) in &tokens {
        let Token::BracedBody(_) = token else {
            continue;
        };
        let region = content.get(span.start..span.end).expect("token span in bounds");
        let mut line_offsets = Vec::new();
        let mut offset = 0usize;
        for line in region.split_inclusive('\n') {
            line_offsets.push(offset);
            offset += line.len();
        }
        for (line_index, id) in raw_include_directives(region) {
            if id != old {
                continue;
            }
            let Some(line_offset) = line_offsets.get(line_index) else {
                continue;
            };
            let line_start = span.start + line_offset;
            let line_end = content[line_start..span.end]
                .split(['\n', '\r'])
                .next()
                .unwrap_or("");
            let Some(offset_in_line) = line_end.find(old) else {
                continue;
            };
            let start = line_start + offset_in_line;
            edits.push(SourceEdit {
                file: file.to_path_buf(),
                start,
                end: start + old.len(),
                replacement: new.to_string(),
                kind: EditKind::Include,
            });
        }
    }
}

/// Warn about bare include spellings that resolve to the renamed entity:
/// they are found in the model but cannot be located in the source safely.
fn collect_bare_include_warnings(doc: &SchemaDoc, old: &str, warnings: &mut Vec<String>) {
    for entity in &doc.entities {
        for value in &entity.values {
            if let Value::BracedBody(body) = &value.value {
                for (_, id) in body_include_directives(body) {
                    if id == old || id.contains('.') || id.starts_with('"') {
                        continue;
                    }
                    let candidates = resolve_include_candidates(entity, id, doc);
                    if candidates == vec![old.to_string()] {
                        warnings.push(format!(
                            "{}:{}: include \"{id}\" resolves to \"{old}\" via a bare short name; update it manually",
                            entity.source_location.file.display(),
                            value.source_location.line,
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    #[test]
    fn body_include_rewrite_targets_exact_directives() {
        let body = "include alpha\n\n```markdown\ninclude alpha\n```\ninclude beta\n";
        assert_eq!(
            rewrite_body_includes(body, "alpha", "gamma"),
            "include gamma\n\n```markdown\ninclude alpha\n```\ninclude beta\n"
        );
    }

    #[test]
    fn body_include_rewrite_leaves_unrelated_bodies_alone() {
        let body = "prose only, no directives";
        assert_eq!(rewrite_body_includes(body, "alpha", "gamma"), body);
    }

    #[test]
    fn simulate_rewrites_refs_and_declarations() {
        let source = r#"
schema "s" {
    type "Item" {
        field "name" text heading
        field "next" ref "Item" optional
    }
}

entity "Item" "a" {
    field "name" "A"
}

entity "Item" "b" {
    field "name" "B"
    field "next" [ "a" ]
}
"#;
        let file = std::path::Path::new("test.vdl");
        let tokens = crate::lexer::lex(source, file).expect("lex");
        let doc = parser::schema::parse_schema_document(&tokens, source, file).expect("parse");
        let renamed = simulate(&doc, "a", "z");
        assert_eq!(renamed.instance("z").expect("renamed").values.len(), 1);
        assert!(renamed.instance("a").is_none());
        let next = renamed
            .instance("b")
            .and_then(|b| b.value("next"))
            .expect("next value");
        match next {
            Value::Refs(ids) => assert_eq!(ids, &["z".to_string()]),
            _ => panic!("expected refs"),
        }
    }
}
