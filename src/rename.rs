//! Reference rewrite: rename an entity ID across a project's sources.
//!
//! The plan locates every source occurrence of one entity ID — its
//! declaration, every reference-field value (quoted, dotted, or bare), and
//! braced-body `include` directives — as byte-span edits. The rewritten
//! model is validated in memory before anything is written, so a rename that
//! would break the project fails without touching a single file.
//!
//! Reference and declaration spellings are preserved. Files named after the
//! renamed entity, and folders with its local name, move with the entity.

use crate::error::{VdlError, VdlResult};
use crate::lexer::{self, token::Token};
use crate::model::{
    body_include_directives, resolve_include_candidates, resolve_short_name, SchemaDoc, Value,
};
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
    pub path_renames: Vec<(PathBuf, PathBuf)>,
    pub order_shifts: Vec<String>,
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

    pub fn source_spelling(&self, edit: &SourceEdit) -> String {
        self.contents
            .get(&edit.file)
            .and_then(|source| source.get(edit.start..edit.end))
            .unwrap_or("")
            .to_string()
    }

    /// Write every planned edit back to its file.
    pub fn apply(&self) -> VdlResult<()> {
        for (file, _) in self.per_file() {
            let content = self
                .contents
                .get(&file)
                .ok_or_else(|| VdlError::Io {
                    message: format!(
                        "planned file '{}' is missing from the rename plan",
                        file.display()
                    ),
                })?
                .clone();
            let mut edits: Vec<&SourceEdit> =
                self.edits.iter().filter(|edit| edit.file == file).collect();
            edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));

            let mut result = content;
            for edit in edits {
                result.replace_range(edit.start..edit.end, &edit.replacement);
            }
            fs::write(&file, result).map_err(|e| VdlError::Io {
                message: format!("Failed to write {}: {}", file.display(), e),
            })?;
        }
        let mut directories: Vec<_> = self
            .path_renames
            .iter()
            .filter(|(from, _)| from.is_dir())
            .collect();
        directories.sort_by_key(|(from, _)| from.components().count());
        let files: Vec<_> = self
            .path_renames
            .iter()
            .filter(|(from, _)| from.is_file())
            .collect();
        let mut moved_directories: Vec<(PathBuf, PathBuf)> = Vec::new();
        for (from, to) in &directories {
            let mut current = from.clone();
            if let Some((old_parent, new_parent)) = moved_directories
                .iter()
                .filter(|(old_parent, _)| current.starts_with(old_parent))
                .max_by_key(|(old_parent, _)| old_parent.components().count())
            {
                if let Ok(relative) = current.strip_prefix(old_parent) {
                    current = new_parent.join(relative);
                }
            }
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| VdlError::Io {
                    message: format!("Failed to create {}: {}", parent.display(), e),
                })?;
            }
            if current != **to {
                fs::rename(&current, to).map_err(|e| VdlError::Io {
                    message: format!(
                        "Failed to rename {} to {}: {}",
                        current.display(),
                        to.display(),
                        e
                    ),
                })?;
            }
            moved_directories.push(((*from).clone(), (*to).clone()));
        }
        for (from, to) in files {
            let current = directories
                .iter()
                .filter(|(directory, _)| from.starts_with(directory))
                .max_by_key(|(directory, _)| directory.components().count())
                .and_then(|(directory, target)| {
                    from.strip_prefix(directory)
                        .ok()
                        .map(|relative| target.join(relative))
                })
                .unwrap_or_else(|| from.clone());
            if current != *to {
                fs::rename(&current, to).map_err(|e| VdlError::Io {
                    message: format!(
                        "Failed to rename {} to {}: {}",
                        current.display(),
                        to.display(),
                        e
                    ),
                })?;
            }
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
    if new.is_empty()
        || new
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
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
    if project
        .doc
        .entities
        .iter()
        .any(|entity| entity.id.eq_ignore_ascii_case(new) && entity.id != old)
    {
        return Err(VdlError::Other {
            message: format!("entity \"{new}\" already exists"),
        });
    }

    let target = project.doc.instance(old).expect("checked above");
    ensure_declaration_remains_local(
        target.package.as_deref(),
        old,
        new,
        &target.source_location.file,
    )?;
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
    let path_renames = plan_path_renames(project, old, new, &target.source_location.file)?;
    let order_shifts = predict_order_shifts(project, &path_renames);

    for source in &project.sources {
        let content = fs::read_to_string(source).map_err(|e| VdlError::Io {
            message: format!("Failed to read {}: {}", source.display(), e),
        })?;
        contents.insert(source.clone(), content.clone());
        collect_include_edits(source, &content, old, new, &project.doc, &mut edits);
    }

    ensure_existing_resolutions(&project.doc, &simulated, old, new, &contents)?;
    ensure_include_resolutions(&project.doc, &simulated, old, new, &contents)?;

    for entity in &project.doc.entities {
        let file = &entity.source_location.file;
        if entity.id == old {
            edits.push(SourceEdit {
                file: file.clone(),
                start: entity.id_span.start,
                end: entity.id_span.end,
                replacement: declaration_spelling(entity.package.as_deref(), new),
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
                let original = contents
                    .get(file)
                    .and_then(|source| source.get(span.start..span.end))
                    .unwrap_or("");
                let replacement = match bare {
                    false if original.starts_with('"') => format!("\"{new}\""),
                    false => new.to_string(),
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

    Ok(RenamePlan {
        old: old.to_string(),
        new: new.to_string(),
        edits,
        warnings,
        path_renames,
        order_shifts,
        contents,
    })
}

/// The model after the rename, used to decide whether the project survives.
fn simulate(doc: &SchemaDoc, old: &str, new: &str) -> SchemaDoc {
    let original = doc.clone();
    let mut doc = doc.clone();
    for entity in &mut doc.entities {
        let entity_context = entity.clone();
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
                    *text = rewrite_resolvable_body_includes(
                        text,
                        &entity_context,
                        &original,
                        old,
                        new,
                    );
                }
                _ => {}
            }
        }
    }
    doc
}

/// Rewrite exact `include <old>` lines inside a braced body.
fn rewrite_resolvable_body_includes(
    body: &str,
    entity: &crate::model::Instance,
    doc: &SchemaDoc,
    old: &str,
    new: &str,
) -> String {
    let directives = body_include_directives(body);
    if directives.is_empty() {
        return body.to_string();
    }
    let mut lines: Vec<String> = body.lines().map(str::to_owned).collect();
    for (line_index, id) in directives {
        if resolve_include_candidates(entity, id, doc) != [old.to_string()] {
            continue;
        }
        let replacement = include_replacement(id, new);
        if let Some(line) = lines.get_mut(line_index) {
            if let Some(start) = include_argument_offset(line)
                .and_then(|offset| line[offset..].find(id).map(|index| index + offset))
            {
                line.replace_range(start..start + id.len(), &replacement);
            }
        }
    }
    let mut result = lines.join("\n");
    if body.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
fn rewrite_body_includes(body: &str, old: &str, new: &str) -> String {
    let mut lines: Vec<String> = body.lines().map(str::to_owned).collect();
    for (line_index, id) in body_include_directives(body) {
        let quoted = id.starts_with('"') && id.ends_with('"');
        if id == old || (quoted && id.trim_matches('"') == old) {
            let replacement = if quoted {
                format!("\"{new}\"")
            } else {
                new.to_string()
            };
            if let Some(line) = lines.get_mut(line_index) {
                if let Some(start) = include_argument_offset(line)
                    .and_then(|offset| line[offset..].find(id).map(|found| found + offset))
                {
                    line.replace_range(start..start + id.len(), &replacement);
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

fn include_argument_offset(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("include ")?;
    Some(leading + "include ".len() + (rest.len() - rest.trim_start().len()))
}

fn raw_line_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut offsets = vec![0];
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                index += if bytes.get(index + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                offsets.push(index);
            }
            b'\n' => {
                index += 1;
                offsets.push(index);
            }
            _ => index += 1,
        }
    }
    offsets
}

fn collect_validation_errors(doc: &SchemaDoc) -> Vec<String> {
    match validate_schema(doc) {
        Ok(()) => Vec::new(),
        Err(error) => vec![error.to_string()],
    }
}

fn local_id(id: &str) -> String {
    id.rsplit('.').next().unwrap_or(id).to_string()
}

fn declaration_spelling(package: Option<&str>, id: &str) -> String {
    if package.is_some() {
        local_id(id)
    } else {
        id.to_string()
    }
}

fn ensure_declaration_remains_local(
    package: Option<&str>,
    old: &str,
    new: &str,
    file: &Path,
) -> VdlResult<()> {
    let local = declaration_spelling(package, new);
    let representable = match package {
        Some(package) => new.strip_prefix(&format!("{package}.")) == Some(local.as_str()),
        None => true,
    };
    if !representable {
        return Err(VdlError::Other {
            message: format!("cannot rename \"{old}\" to \"{new}\": a package-local declaration in {:?} would resolve to a different ID", package),
        });
    }
    let tokens = lexer::lex(&local, file).map_err(|_| VdlError::Other {
        message: format!("new local declaration name \"{local}\" is not a valid bare name"),
    })?;
    let valid_segments = local.split('.').all(|segment| {
        !segment.is_empty()
            && !segment.starts_with('-')
            && !segment.ends_with('-')
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    });
    if valid_segments && tokens.len() == 1 && tokens[0].0.word() == Some(local.as_str()) {
        Ok(())
    } else {
        Err(VdlError::Other {
            message: format!("new local declaration name \"{local}\" is not a valid bare name"),
        })
    }
}

fn ensure_existing_resolutions(
    original: &SchemaDoc,
    simulated: &SchemaDoc,
    old: &str,
    new: &str,
    contents: &HashMap<PathBuf, String>,
) -> VdlResult<()> {
    for import in &original.imports {
        if !import.wildcard && import.target == old {
            return Err(VdlError::Other {
                message: format!(
                    "{}: explicit import of \"{old}\" must be updated manually before renaming",
                    import.source_location
                ),
            });
        }
    }
    for entity in &original.entities {
        for value in &entity.values {
            for (index, target) in value.value.reference_ids().enumerate() {
                if !value.bare_ref_indices.contains(&index) {
                    continue;
                }
                let renamed_target = target == old;
                let target = if renamed_target { new } else { target };
                let Some(span) = value.ref_spans.get(index) else {
                    return Err(VdlError::Other {
                        message: format!("{}: bare reference span is unavailable; cannot verify resolution preservation", value.source_location),
                    });
                };
                let original_spelling = contents
                    .get(&value.source_location.file)
                    .and_then(|source| source.get(span.start..span.end))
                    .ok_or_else(|| VdlError::Other {
                        message: format!(
                            "{}: bare reference source span is invalid",
                            value.source_location
                        ),
                    })?;
                let replacement_spelling = if renamed_target {
                    local_id(new)
                } else {
                    original_spelling.to_string()
                };
                let resolved = resolve_short_name(
                    &entity.imports,
                    entity.package.as_deref(),
                    &replacement_spelling,
                    |id| simulated.instance(id).is_some(),
                );
                if resolved != [target.to_string()] {
                    return Err(VdlError::Other {
                        message: format!("{}: preserving bare reference \"{replacement_spelling}\" would change its target from \"{target}\" to {:?}", value.source_location, resolved),
                    });
                }
            }
        }
    }
    Ok(())
}

fn ensure_include_resolutions(
    original: &SchemaDoc,
    simulated: &SchemaDoc,
    old: &str,
    new: &str,
    contents: &HashMap<PathBuf, String>,
) -> VdlResult<()> {
    for (file, source) in contents {
        let Ok(tokens) = lexer::lex(source, file) else {
            continue;
        };
        for (token, span) in tokens {
            let Token::BracedBody(raw_body) = token else {
                continue;
            };
            let body = crate::parser::schema::dedent(&raw_body);
            let Some(owner) = original
                .entities
                .iter()
                .filter(|entity| {
                    entity.source_location.file == *file && entity.source_location.line <= span.line
                })
                .max_by_key(|entity| entity.source_location.line)
            else {
                continue;
            };
            for (_, spelling) in body_include_directives(&body) {
                let before = resolve_include_candidates(owner, spelling, original);
                if before.len() > 1 && before.iter().any(|candidate| candidate == old) {
                    return Err(VdlError::Other { message: format!("{}: ambiguous include \"{spelling}\" mentions the renamed entity; update it manually", owner.source_location) });
                }
                let rename_target = before == [old.to_string()];
                let replacement = if rename_target {
                    include_replacement(spelling, new)
                } else {
                    spelling.to_string()
                };
                let after = resolve_include_candidates(owner, &replacement, simulated);
                let expected = before
                    .iter()
                    .map(|candidate| {
                        if candidate == old {
                            new.to_string()
                        } else {
                            candidate.clone()
                        }
                    })
                    .collect::<Vec<_>>();
                if after != expected {
                    return Err(VdlError::Other { message: format!("{}: preserving include \"{replacement}\" would change its target from {:?} to {:?}", owner.source_location, expected, after) });
                }
            }
        }
    }
    Ok(())
}

fn include_replacement(spelling: &str, new: &str) -> String {
    if spelling.starts_with('"') && spelling.ends_with('"') {
        format!("\"{new}\"")
    } else if spelling.contains('.') {
        new.to_string()
    } else {
        local_id(new)
    }
}

fn plan_path_renames(
    project: &SchemaProject,
    old: &str,
    new: &str,
    entity_file: &Path,
) -> VdlResult<Vec<(PathBuf, PathBuf)>> {
    let mut renames = Vec::new();
    let old_local = local_id(old);
    let new_local = local_id(new);
    let mut folders: Vec<(PathBuf, PathBuf)> = Vec::new();
    let relative_parent = entity_file
        .strip_prefix(&project.root)
        .unwrap_or(entity_file)
        .parent()
        .unwrap_or(Path::new("."));
    let mut current = project.root.clone();
    for component in relative_parent.components() {
        current.push(component.as_os_str());
        if current != project.root
            && component.as_os_str() == old_local.as_str()
            && current.is_dir()
        {
            let target_parent = map_path_through_folders(&current, &folders);
            folders.push((current.clone(), target_parent.with_file_name(&new_local)));
        }
    }
    if entity_file
        .file_name()
        .is_some_and(|name| name.to_string_lossy() == format!("{old_local}.vdl"))
    {
        let sibling = entity_file
            .parent()
            .unwrap_or(&project.root)
            .join(&old_local);
        if sibling != project.root
            && sibling.is_dir()
            && !folders.iter().any(|(from, _)| from == &sibling)
        {
            let mapped_sibling = map_path_through_folders(&sibling, &folders);
            folders.push((sibling, mapped_sibling.with_file_name(&new_local)));
        }
    }
    for source in &project.sources {
        let in_target_folder = folders.iter().any(|(folder, _)| source.starts_with(folder));
        let is_entity_file = source == entity_file
            && source
                .file_name()
                .is_some_and(|name| name.to_string_lossy() == format!("{old_local}.vdl"));
        if !in_target_folder && !is_entity_file {
            continue;
        }
        let mut target = folders
            .iter()
            .filter(|(folder, _)| source.starts_with(folder))
            .max_by_key(|(folder, _)| folder.components().count())
            .and_then(|(folder, target)| {
                source
                    .strip_prefix(folder)
                    .ok()
                    .map(|relative| target.join(relative))
            })
            .unwrap_or_else(|| source.clone());
        if is_entity_file {
            target.set_file_name(format!("{new_local}.vdl"));
        }
        if target != *source {
            renames.push((source.clone(), target));
        }
    }
    renames.extend(folders);
    renames.sort();
    renames.dedup();
    let sources: Vec<_> = project.sources.iter().collect();
    for (from, to) in &renames {
        if let Some(existing) = sources.iter().find(|source| {
            source.as_path() != from
                && source
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&to.to_string_lossy())
        }) {
            return Err(VdlError::Other {
                message: format!(
                    "rename path collision: '{}' conflicts with '{}' (case-insensitive)",
                    to.display(),
                    existing.display()
                ),
            });
        }
        if to.exists() && !renames.iter().any(|(candidate, _)| candidate == to) {
            return Err(VdlError::Other {
                message: format!("rename destination already exists: '{}'", to.display()),
            });
        }
    }
    let mut targets = Vec::new();
    for (_, target) in &renames {
        if targets.iter().any(|path: &PathBuf| {
            path.to_string_lossy()
                .eq_ignore_ascii_case(&target.to_string_lossy())
        }) {
            return Err(VdlError::Other {
                message: format!(
                    "multiple sources map to '{}' (case-insensitive)",
                    target.display()
                ),
            });
        }
        targets.push(target.clone());
    }
    let planned_sources: Vec<_> = renames.iter().map(|(from, _)| from.clone()).collect();
    for (_, target) in &renames {
        let Some(file_name) = target.file_name() else {
            continue;
        };
        let Some(mut parent) = target.parent().map(Path::to_path_buf) else {
            continue;
        };
        if let Some((from, _)) = renames
            .iter()
            .filter(|(from, to)| from.is_dir() && parent.starts_with(to))
            .max_by_key(|(_, to)| to.components().count())
        {
            if let Some((_, to)) = renames.iter().find(|(candidate, _)| candidate == from) {
                if let Ok(relative) = parent.strip_prefix(to) {
                    parent = from.join(relative);
                }
            }
        }
        if !parent.is_dir() {
            continue;
        }
        let entries = fs::read_dir(&parent).map_err(|error| VdlError::Io {
            message: format!(
                "Failed to inspect rename destination parent {}: {}",
                parent.display(),
                error
            ),
        })?;
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&file_name.to_string_lossy())
            {
                let existing = entry.path();
                if !planned_sources.contains(&existing) {
                    return Err(VdlError::Other {
                        message: format!(
                            "rename destination '{}' collides case-insensitively with '{}'",
                            target.display(),
                            existing.display()
                        ),
                    });
                }
            }
        }
    }
    Ok(renames)
}

fn map_path_through_folders(path: &Path, folders: &[(PathBuf, PathBuf)]) -> PathBuf {
    folders
        .iter()
        .filter(|(folder, _)| path.starts_with(folder))
        .max_by_key(|(folder, _)| folder.components().count())
        .and_then(|(folder, target)| {
            path.strip_prefix(folder)
                .ok()
                .map(|relative| target.join(relative))
        })
        .unwrap_or_else(|| path.to_path_buf())
}

fn predict_order_shifts(project: &SchemaProject, renames: &[(PathBuf, PathBuf)]) -> Vec<String> {
    let mapping: HashMap<_, _> = renames.iter().cloned().collect();
    let mut old_order: Vec<_> = project.doc.entities.iter().enumerate().collect();
    let mut new_order = old_order.clone();
    new_order.sort_by(|(left_index, left), (right_index, right)| {
        let left_path = mapping
            .get(&left.source_location.file)
            .unwrap_or(&left.source_location.file);
        let right_path = mapping
            .get(&right.source_location.file)
            .unwrap_or(&right.source_location.file);
        left_path.cmp(right_path).then(left_index.cmp(right_index))
    });
    let mut new_positions = HashMap::new();
    for (position, (index, _)) in new_order.iter().enumerate() {
        new_positions.insert(*index, position + 1);
    }
    old_order
        .drain(..)
        .enumerate()
        .filter_map(|(old_position, (index, entity))| {
            let new_position = new_positions[&index];
            (new_position != old_position + 1)
                .then(|| format!("{}: {} -> {}", entity.id, old_position + 1, new_position))
        })
        .collect()
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
    doc: &SchemaDoc,
    edits: &mut Vec<SourceEdit>,
) {
    let Ok(tokens) = lexer::lex(content, file) else {
        return;
    };
    for (token, span) in &tokens {
        let Token::BracedBody(raw_body) = token else {
            continue;
        };
        let model_body = crate::parser::schema::dedent(raw_body);
        let region = content
            .get(span.start..span.end)
            .expect("token span in bounds");
        let owner = doc
            .entities
            .iter()
            .filter(|entity| {
                entity.source_location.file == file && entity.source_location.line <= span.line
            })
            .max_by_key(|entity| entity.source_location.line);
        let line_offsets = raw_line_offsets(region);
        for (body_line_index, id) in body_include_directives(&model_body) {
            if !owner.is_some_and(|entity| {
                resolve_include_candidates(entity, id, doc) == [old.to_string()]
            }) {
                continue;
            }
            let replacement = include_replacement(id, new);
            let line_index = body_line_index + 1; // raw span starts with the opening brace line
            let Some(line_offset) = line_offsets.get(line_index) else {
                continue;
            };
            let line_start = span.start + line_offset;
            let line_end = content[line_start..span.end]
                .split(['\n', '\r'])
                .next()
                .unwrap_or("");
            let argument_offset = include_argument_offset(line_end).unwrap_or(line_end.len());
            let Some(offset_in_line) = line_end[argument_offset..]
                .find(id)
                .map(|offset| offset + argument_offset)
            else {
                continue;
            };
            let start = line_start + offset_in_line;
            edits.push(SourceEdit {
                file: file.to_path_buf(),
                start,
                end: start + id.len(),
                replacement,
                kind: EditKind::Include,
            });
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
