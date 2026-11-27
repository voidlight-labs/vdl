//! Render a schema document into one complete Markdown document.
//!
//! The renderer reads the authored schema for every decision:
//!
//! * frontmatter lists `id`, `type`, then the root type's `text` fields in
//!   declaration order, then non-composed `ref` collections as YAML lists;
//! * the root's `heading` field becomes `# ...`;
//! * the root's `body` field is emitted verbatim;
//! * each `compose`d collection is expanded in field declaration order and,
//!   inside a field, in author order; every referenced entity contributes
//!   a heading one level deeper than its parent and its body verbatim;
//! * nothing else is expanded, so an ordinary reference never inlines the
//!   target's body.
//!

use crate::error::{VdlError, VdlResult};
use crate::model::{body_include_directives, resolve_include_candidates, FieldKind, InlineItem, Instance, SchemaDoc, TypeDecl, Value};
use crate::validator::schema::validate_schema;

/// How `cite`-flagged reference fields render at their cite points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiteMode {
    /// Cite fields never render (default; matches the pre-flag behavior).
    None,
    /// Render the referenced entity IDs verbatim.
    Link,
    /// Render each target's heading-field text, falling back to its ID.
    Short,
}

/// Options controlling composition output.
#[derive(Debug, Clone)]
pub struct ComposeOptions {
    pub cites: CiteMode,
    pub number_headings: bool,
    /// Entity ID → Markdown link path for cited entities that have their own
    /// compiled output, already relative to the document being rendered.
    /// Set only by multi-target builds; `cite` fields render as links in
    /// `Link` mode when the target is present here.
    pub cite_links: Option<std::rc::Rc<std::collections::HashMap<String, String>>>,
}

impl Default for ComposeOptions {
    fn default() -> Self {
        Self { cites: CiteMode::None, number_headings: false, cite_links: None }
    }
}

/// Compose the document rooted at `root_id`.
///
/// # Errors
///
/// Returns the schema validation error when the document is not valid, and
/// [`VdlError::Codegen`] when the root or a composed target cannot be resolved
/// or lacks a heading value, or when composition cycles or exceeds six levels.
pub fn compose(doc: &SchemaDoc, root_id: &str) -> VdlResult<String> {
    compose_with_options(doc, root_id, &ComposeOptions::default())
}

#[cfg(test)]
pub(crate) fn compose_with_numbering(
    doc: &SchemaDoc,
    root_id: &str,
    number_headings: bool,
) -> VdlResult<String> {
    compose_with_options(
        doc,
        root_id,
        &ComposeOptions { cites: CiteMode::None, number_headings, cite_links: None },
    )
}

/// Compose the document rooted at `root_id` with explicit options.
///
/// # Errors
///
/// Same guarantees as [`compose`].
pub fn compose_with_options(
    doc: &SchemaDoc,
    root_id: &str,
    options: &ComposeOptions,
) -> VdlResult<String> {
    validate_schema(doc)?;

    let root = doc.instance(root_id).ok_or_else(|| VdlError::Codegen {
        message: format!(
            "root entity '{}' not found in schema document '{}'",
            root_id, doc.schema.name
        ),
    })?;
    let root_type = doc.type_decl(&root.type_name).ok_or_else(|| VdlError::Codegen {
        message: format!(
            "root entity '{}' has undeclared type '{}'",
            root.id, root.type_name
        ),
    })?;

    let frontmatter = render_frontmatter(root, root_type)?;

    let mut blocks = Vec::new();
    render_entity(
        doc,
        root,
        root_type,
        &mut Vec::new(),
        &mut blocks,
        options.number_headings.then_some(""),
        options,
    )?;

    let mut output = frontmatter;
    if !blocks.is_empty() {
        output.push('\n');
        output.push_str(&blocks.join("\n\n"));
        output.push('\n');
    }
    Ok(output)
}

/// One outline entry: heading level (1 = root) and its rendered title.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    pub level: usize,
    pub title: String,
}

/// Compose a table of contents for the document rooted at `root_id`: the
/// composed heading tree without bodies or preamble. Body `include`
/// directives contribute their targets; inline fields contribute nothing,
/// because inline entities render without headings. Numbering matches
/// [`compose_with_options`] when `number_headings` is set.
///
/// # Errors
///
/// Same guarantees as [`compose`].
pub fn compose_toc(doc: &SchemaDoc, root_id: &str, options: &ComposeOptions) -> VdlResult<String> {
    validate_schema(doc)?;

    let root = doc.instance(root_id).ok_or_else(|| VdlError::Codegen {
        message: format!(
            "root entity '{}' not found in schema document '{}'",
            root_id, doc.schema.name
        ),
    })?;
    let root_type = doc.type_decl(&root.type_name).ok_or_else(|| VdlError::Codegen {
        message: format!(
            "root entity '{}' has undeclared type '{}'",
            root.id, root.type_name
        ),
    })?;

    let mut entries = Vec::new();
    render_outline(
        doc,
        root,
        root_type,
        &mut Vec::new(),
        &mut entries,
        options.number_headings.then_some(""),
        options,
    )?;

    let mut output = String::new();
    for entry in &entries {
        output.push_str(&format!(
            "{}- {}\n",
            "  ".repeat(entry.level.saturating_sub(1)),
            entry.title
        ));
    }
    Ok(output)
}

fn render_entity<'doc>(
    doc: &'doc SchemaDoc,
    entity: &'doc Instance,
    entity_type: &'doc TypeDecl,
    ancestors: &mut Vec<&'doc str>,
    blocks: &mut Vec<String>,
    section_number: Option<&str>,
    options: &ComposeOptions,
) -> VdlResult<()> {
    let id = entity.id.as_str();
    if ancestors.contains(&id) {
        let chain = ancestors.iter().copied().chain([id]).collect::<Vec<_>>();
        return Err(VdlError::Codegen {
            message: format!("composition cycle: {}", chain.join(" -> ")),
        });
    }

    let level = ancestors.len() + 1;
    if level > 6 {
        return Err(VdlError::Codegen {
            message: format!("composition of '{}' exceeds Markdown's six heading levels", id),
        });
    }

    match heading_text(entity, entity_type) {
        Some(heading) => {
            let marker = "#".repeat(level);
            let heading = match section_number {
                Some(number) if level == 2 => format!("{} {}. {}", marker, number, heading),
                Some(number) if level > 2 => format!("{} {} {}", marker, number, heading),
                _ => format!("{} {}", marker, heading),
            };
            blocks.push(heading);
        }
        None if level > 1 => {
            return Err(VdlError::Codegen {
                message: format!(
                    "composed entity '{}' of type '{}' has no heading value",
                    id, entity_type.name
                ),
            });
        }
        None => {}
    }
    // Composed children surface their supplied non-heading text fields and
    // `cite`-flagged references between the heading and the body. The root's
    // text fields already live in the frontmatter, so it never emits them here.
    let is_root = ancestors.is_empty();
    ancestors.push(id);
    if !is_root {
        for line in entity_preamble(doc, entity, entity_type, options) {
            blocks.push(line);
        }
    }
    let mut child_index = 0;
    if let Some(body_field) = entity_type.body_field() {
        if let Some(value @ (Value::Body(_) | Value::BracedBody(_))) = entity.value(&body_field.name) {
            let body = match value { Value::Body(body) | Value::BracedBody(body) => body, _ => unreachable!() };
                if !body.trim().is_empty() {
                    if body_field.compose_targets.is_empty() || !matches!(value, Value::BracedBody(_)) {
                        blocks.push(labelled_body(entity_type, body));
                    } else {
                        render_body_includes(doc, entity, entity_type, body, ancestors, blocks, section_number, &mut child_index, options)?;
                    }
                }
        }
    }
    for field in entity_type.composed_fields() {
        let Some(value) = entity.value(&field.name) else {
            continue;
        };
        if field.inline {
            match value {
                Value::Refs(ids) => {
                    for id in ids {
                        blocks.push(inline_body(doc, id, options)?);
                    }
                }
                Value::Inline(items) => {
                    for item in items {
                        blocks.push(match item {
                            InlineItem::Ref(id) => inline_body(doc, id, options)?,
                            InlineItem::Prose(prose) => prose.clone(),
                        });
                    }
                }
                _ => {}
            }
            continue;
        }
        let Value::Refs(ids) = value else {
            continue;
        };
        for id in ids {
            let target = doc.instance(id).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "entity '{}' field '{}' references unresolved entity '{}'",
                    entity.id, field.name, id
                ),
            })?;
            let target_type = doc.type_decl(&target.type_name).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "entity '{}' has undeclared type '{}'",
                    target.id, target.type_name
                ),
            })?;
            child_index += 1;
            let child_number = section_number.map(|parent| {
                if parent.is_empty() {
                    child_index.to_string()
                } else {
                    format!("{}.{}", parent, child_index)
                }
            });
            render_entity(
                doc,
                target,
                target_type,
                ancestors,
                blocks,
                child_number.as_deref(),
                options,
            )?;
        }
    }
    ancestors.pop();
    Ok(())
}

fn render_body_includes<'doc>(
    doc: &'doc SchemaDoc, entity: &'doc Instance, entity_type: &'doc TypeDecl, body: &str,
    ancestors: &mut Vec<&'doc str>, blocks: &mut Vec<String>, section_number: Option<&str>, child_index: &mut usize,
    options: &ComposeOptions,
) -> VdlResult<()> {
    if let Some(label) = &entity_type.label { blocks.push(emit_label(label)); }
    let mut prose = Vec::new();
    let directives = body_include_directives(body).into_iter().map(|(line, id)| (line, id.to_owned())).collect::<std::collections::HashMap<_, _>>();
    for (line_index, line) in body.lines().enumerate() {
        if let Some(name) = directives.get(&line_index).map(String::as_str) {
            if !prose.join("\n").trim().is_empty() { blocks.push(prose.join("\n")); }
            prose.clear();
            let candidates = resolve_include_candidates(entity, name, doc);
            let target_id = match candidates.as_slice() {
                [id] => id.clone(),
                [] => return Err(VdlError::Codegen { message: format!("body include '{}' has no matching target", name) }),
                many => return Err(VdlError::Codegen { message: format!("body include '{}' is ambiguous: {}", name, many.join(", ")) }),
            };
            let target = doc.instance(&target_id).ok_or_else(|| VdlError::Codegen { message: format!("entity '{}' body includes unresolved entity '{}'", entity.id, target_id) })?;
            let target_type = doc.type_decl(&target.type_name).ok_or_else(|| VdlError::Codegen { message: format!("included entity '{}' has undeclared type '{}'", target.id, target.type_name) })?;
            let body_field = entity_type.body_field().expect("body compose is a body field");
            if !body_field.compose_targets.iter().any(|t| t == &target.type_name) {
                return Err(VdlError::Codegen { message: format!("entity '{}' body include '{}' has type '{}'; expected one of [{}]", entity.id, name, target.type_name, body_field.compose_targets.join(", ")) });
            }
            *child_index += 1;
            let number = section_number.map(|parent| if parent.is_empty() { child_index.to_string() } else { format!("{}.{}", parent, child_index) });
            render_entity(doc, target, target_type, ancestors, blocks, number.as_deref(), options)?;
        } else { prose.push(line.to_owned()); }
    }
    if !prose.join("\n").trim().is_empty() { blocks.push(prose.join("\n")); }
    Ok(())
}

/// The heading tree of [`render_entity`], with bodies, preamble, and inline
/// fields skipped. Keep the heading, numbering, and recursion in sync with
/// [`render_entity`].
fn render_outline<'doc>(
    doc: &'doc SchemaDoc,
    entity: &'doc Instance,
    entity_type: &'doc TypeDecl,
    ancestors: &mut Vec<&'doc str>,
    entries: &mut Vec<OutlineEntry>,
    section_number: Option<&str>,
    options: &ComposeOptions,
) -> VdlResult<()> {
    let id = entity.id.as_str();
    if ancestors.contains(&id) {
        let chain = ancestors.iter().copied().chain([id]).collect::<Vec<_>>();
        return Err(VdlError::Codegen {
            message: format!("composition cycle: {}", chain.join(" -> ")),
        });
    }
    let level = ancestors.len() + 1;
    if level > 6 {
        return Err(VdlError::Codegen {
            message: format!("composition of '{}' exceeds Markdown's six heading levels", id),
        });
    }
    if let Some(heading) = heading_text(entity, entity_type) {
        let title = match section_number {
            Some(number) if level == 2 => format!("{}. {}", number, heading),
            Some(number) if level > 2 => format!("{} {}", number, heading),
            _ => heading,
        };
        entries.push(OutlineEntry { level, title });
    }
    ancestors.push(id);
    let mut child_index = 0usize;
    if let Some(body_field) = entity_type.body_field() {
        if let Some(Value::BracedBody(body)) = entity.value(&body_field.name) {
            if !body_field.compose_targets.is_empty() {
                outline_body_includes(
                    doc,
                    entity,
                    entity_type,
                    body,
                    ancestors,
                    entries,
                    section_number,
                    &mut child_index,
                    options,
                )?;
            }
        }
    }
    for field in entity_type.composed_fields() {
        let Some(value) = entity.value(&field.name) else {
            continue;
        };
        if field.inline {
            // Inline entities render without headings, so they are absent
            // from the outline.
            continue;
        }
        let Value::Refs(ids) = value else {
            continue;
        };
        for id in ids {
            let target = doc.instance(id).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "entity '{}' field '{}' references unresolved entity '{}'",
                    entity.id, field.name, id
                ),
            })?;
            let target_type = doc.type_decl(&target.type_name).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "entity '{}' has undeclared type '{}'",
                    target.id, target.type_name
                ),
            })?;
            child_index += 1;
            let child_number = section_number.map(|parent| {
                if parent.is_empty() {
                    child_index.to_string()
                } else {
                    format!("{}.{}", parent, child_index)
                }
            });
            render_outline(
                doc,
                target,
                target_type,
                ancestors,
                entries,
                child_number.as_deref(),
                options,
            )?;
        }
    }
    ancestors.pop();
    Ok(())
}

/// The include-directive walk of [`render_body_includes`], for outlines.
fn outline_body_includes<'doc>(
    doc: &'doc SchemaDoc,
    entity: &'doc Instance,
    entity_type: &'doc TypeDecl,
    body: &str,
    ancestors: &mut Vec<&'doc str>,
    entries: &mut Vec<OutlineEntry>,
    section_number: Option<&str>,
    child_index: &mut usize,
    options: &ComposeOptions,
) -> VdlResult<()> {
    let directives = body_include_directives(body)
        .into_iter()
        .map(|(line, id)| (line, id.to_owned()))
        .collect::<std::collections::HashMap<_, _>>();
    for (line_index, _line) in body.lines().enumerate() {
        if let Some(name) = directives.get(&line_index).map(String::as_str) {
            let candidates = resolve_include_candidates(entity, name, doc);
            let target_id = match candidates.as_slice() {
                [id] => id.clone(),
                [] => return Err(VdlError::Codegen {
                    message: format!("body include '{}' has no matching target", name),
                }),
                many => return Err(VdlError::Codegen {
                    message: format!("body include '{}' is ambiguous: {}", name, many.join(", ")),
                }),
            };
            let target = doc.instance(&target_id).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "entity '{}' body includes unresolved entity '{}'",
                    entity.id, target_id
                ),
            })?;
            let target_type = doc.type_decl(&target.type_name).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "included entity '{}' has undeclared type '{}'",
                    target.id, target.type_name
                ),
            })?;
            let body_field = entity_type.body_field().expect("body compose is a body field");
            if !body_field.compose_targets.iter().any(|t| t == &target.type_name) {
                return Err(VdlError::Codegen {
                    message: format!(
                        "entity '{}' body include '{}' has type '{}'; expected one of [{}]",
                        entity.id,
                        name,
                        target.type_name,
                        body_field.compose_targets.join(", ")
                    ),
                });
            }
            *child_index += 1;
            let number = section_number.map(|parent| {
                if parent.is_empty() {
                    child_index.to_string()
                } else {
                    format!("{}.{}", parent, child_index)
                }
            });
            render_outline(
                doc,
                target,
                target_type,
                ancestors,
                entries,
                number.as_deref(),
                options,
            )?;
        }
    }
    Ok(())
}

fn inline_body(doc: &SchemaDoc, id: &str, options: &ComposeOptions) -> VdlResult<String> {
    let body = doc.instance(id).and_then(|target| {
        let target_type = doc.type_decl(&target.type_name)?;
        let field = target_type.body_field()?;
        match target.value(&field.name)? {
            Value::Body(body) | Value::BracedBody(body) => Some(render_inline_entity(doc, target, target_type, body, options)),
            _ => None,
        }
    });
    body.ok_or_else(|| VdlError::Codegen {
        message: format!("inline entity '{}' has no body value", id),
    })
}

/// Render one inline-referenced entity: label, then its supplied non-heading
/// text fields and `cite`-flagged reference lines, then the body.
fn render_inline_entity<'doc>(
    doc: &'doc SchemaDoc,
    entity: &'doc Instance,
    entity_type: &'doc TypeDecl,
    body: &str,
    options: &ComposeOptions,
) -> String {
    let mut out = String::new();
    if let Some(label) = &entity_type.label {
        out.push_str(&emit_label(label));
        out.push_str("\n\n");
    }
    let preamble = entity_preamble(doc, entity, entity_type, options);
    if !preamble.is_empty() {
        out.push_str(&preamble.join("\n\n"));
        out.push_str("\n\n");
    }
    out.push_str(body);
    out
}

/// Metadata lines rendered between the label and the body: each supplied
/// non-heading text field, then one `Cites:` line per `cite`-flagged ref field.
fn entity_preamble<'doc>(
    doc: &'doc SchemaDoc,
    entity: &'doc Instance,
    entity_type: &'doc TypeDecl,
    options: &ComposeOptions,
) -> Vec<String> {
    let mut lines = Vec::new();
    for field in &entity_type.fields {
        if field.heading || !matches!(field.kind, FieldKind::Text) {
            continue;
        }
        if let Some(Value::Text(value)) = entity.value(&field.name) {
            if !value.trim().is_empty() {
                lines.push(format!("**{}:** {}", field.name, value));
            }
        }
    }
    for field in &entity_type.fields {
        if !field.cite || options.cites == CiteMode::None {
            continue;
        }
        let Some(Value::Refs(ids)) = entity.value(&field.name) else { continue };
        if ids.is_empty() {
            continue;
        }
        let rendered = ids
            .iter()
            .map(|id| {
                cite_target(doc, id, options.cites, options.cite_links.as_deref())
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("**Cites:** {}", rendered));
    }
    lines
}

/// How one cited target renders: its heading-field text in short mode, its ID
/// otherwise. Unresolved or heading-less targets fall back to the ID. In link
/// mode, a target with its own compiled output renders as a Markdown link.
fn cite_target(
    doc: &SchemaDoc,
    id: &str,
    mode: CiteMode,
    links: Option<&std::collections::HashMap<String, String>>,
) -> String {
    match mode {
        CiteMode::Link => match links.and_then(|links| links.get(id)) {
            Some(path) => format!("[{id}]({path})"),
            None => id.to_owned(),
        },
        CiteMode::None => id.to_owned(),
        CiteMode::Short => {
            let Some(instance) = doc.instance(id) else { return id.to_owned() };
            let Some(type_decl) = doc.type_decl(&instance.type_name) else { return id.to_owned() };
            type_decl
                .heading_field()
                .and_then(|field| match instance.value(&field.name) {
                    Some(Value::Text(text)) if !text.trim().is_empty() => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| id.to_owned())
        }
    }
}

fn labelled_body(entity_type: &TypeDecl, body: &str) -> String {
    match &entity_type.label {
        Some(label) => format!("{}\n\n{}", emit_label(label), body),
        None => body.to_owned(),
    }
}

/// A schema type's `label` renders before its body. Plain labels are bold
/// text; a label that starts with `#` is authored Markdown (a heading) and is
/// emitted verbatim so it keeps its level and anchor.
fn emit_label(label: &str) -> String {
    if label.trim_start().starts_with('#') { label.to_owned() } else { format!("**{}**", label) }
}

fn render_frontmatter(root: &Instance, root_type: &TypeDecl) -> VdlResult<String> {
    check_reserved_keys(root_type)?;

    let mut out = String::from("---\n");
    // Values are always serialized as double-quoted strings, so `true`, `null`,
    // and `001` survive a YAML parser as strings.
    out.push_str(&format!("id: {}\n", yaml_double_quoted(&root.id)));
    out.push_str(&format!(
        "type: {}\n",
        yaml_double_quoted(&root.type_name)
    ));

    for field in root_type.text_fields() {
        if let Some(Value::Text(text)) = root.value(&field.name) {
            out.push_str(&format!(
                "{} {}\n",
                yaml_key(&field.name),
                yaml_double_quoted(text)
            ));
        }
    }

    for field in root_type.ref_fields().filter(|f| !f.compose) {
        let Some(Value::Refs(ids)) = root.value(&field.name) else {
            continue;
        };
        if ids.is_empty() {
            continue;
        }
        out.push_str(&format!("{}\n", yaml_key(&field.name)));
        for id in ids {
            out.push_str(&format!("  - {}\n", yaml_double_quoted(id)));
        }
    }

    out.push_str("---\n");
    Ok(out)
}

/// `id` and `type` belong to the compiler in the flat frontmatter format.
///
/// Only fields that actually reach the frontmatter are checked, so a `compose`
/// field may share a name with a reserved key without colliding.
fn check_reserved_keys(root_type: &TypeDecl) -> VdlResult<()> {
    let emitted = root_type
        .text_fields()
        .chain(root_type.ref_fields().filter(|f| !f.compose));

    for field in emitted {
        if field.name == "id" || field.name == "type" {
            return Err(VdlError::Codegen {
                message: format!(
                    "root field '{}' of type '{}' collides with the compiler-owned frontmatter key '{}'; rename the field",
                    field.name, root_type.name, field.name
                ),
            });
        }
    }

    Ok(())
}

/// The root type's `heading` field value, if it declares one.
fn heading_text(instance: &Instance, type_decl: &TypeDecl) -> Option<String> {
    let field = type_decl.heading_field()?;
    match instance.value(&field.name) {
        Some(Value::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

/// Serialize a string as a YAML double-quoted scalar.
///
/// Everything YAML would otherwise reinterpret (booleans, nulls, numbers,
/// leading zeros, control characters) stays a string because it is quoted and
/// escaped here.
pub(crate) fn yaml_double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0'..='\u{1f}' | '\u{7f}'..='\u{9f}' => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            '\u{2028}' | '\u{2029}' | '\u{feff}' | '\u{fffe}' | '\u{ffff}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Emit a string key and its value indicator. Long keys need YAML's explicit
/// key syntax because implicit keys are limited to 1024 characters.
fn yaml_key(name: &str) -> String {
    let key = yaml_double_quoted(name);
    if key.chars().count() > 1024 {
        format!("? {}\n:", key)
    } else {
        format!("{}:", key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const DOC: &str = r#"
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "chapters" ref "Chapter" compose
        field "see_also" ref "Chapter"
    }
    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}

entity "Chapter" "intro" {
    field "label" "Introduction"
    field "content" """
Prose for intro.

### Internal heading
"""
}

entity "Chapter" "rules" {
    field "label" "Rules"
    field "content" """Prose for rules."""
}

entity "Chapter" "appendix" {
    field "label" "Appendix"
    field "content" """Appendix body that must not be inlined."""
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "chapters" [ "intro", "rules" ]
    field "see_also" [ "appendix" ]
}

entity "Runbook" "ops-guide" {
    field "label" "Ops Guide"
    field "chapters" [ "rules", "intro" ]
    field "see_also" [ "appendix" ]
}
"#;

    fn doc(src: &str) -> SchemaDoc {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(src, file).expect("lexing should succeed");
        crate::parser::schema::parse_schema_document(&tokens, src, file)
            .expect("parsing should succeed")
    }

    fn render(root: &str) -> String {
        compose(&doc(DOC), root).expect("composition should succeed")
    }

    #[test]
    fn frontmatter_lists_id_type_then_text_fields() {
        let md = render("core-guide");
        let id_pos = md.find("id: \"core-guide\"").unwrap();
        let type_pos = md.find("type: \"Runbook\"").unwrap();
        let label_pos = md.find("\"label\": \"Core Guide\"").unwrap();
        assert!(id_pos < type_pos && type_pos < label_pos);
    }

    #[test]
    fn root_heading_uses_heading_field() {
        // The heading field here is `label`, not `title`.
        let md = render("core-guide");
        assert!(md.contains("\n# Core Guide\n"), "{}", md);
    }

    #[test]
    fn composed_section_heading_uses_target_heading_field() {
        let md = render("core-guide");
        assert!(md.contains("## Introduction"));
        assert!(md.contains("## Rules"));
    }

    fn compose_with(source: &str, options: &ComposeOptions) -> String {
        compose_with_options(&doc(source), "doc", options).expect("composition should succeed")
    }

    const CITE_DOC: &str = r#"
schema "cites" {
    type "Verse" label "REVELATION" {
        field "cites" ref "Ref" cite optional
        field "body" body
    }
    type "Ref" {
        field "title" text heading
        field "body" body
    }
    type "Note" label "AUDIT NOTE" {
        field "auditor" text optional
        field "date" text
        field "body" body
    }
    type "Doc" {
        field "title" text heading
        field "steps" ref ["Verse", "Note"] inline optional
    }
}
entity "Ref" "r1" {
    title "HR. Muslim no. 2653 (Kitab al-Qadar)"
    body {
        matn rujukan
    }
}
entity "Verse" "v1" {
    cites [r1]
    body {
        kutipan ayat
    }
}
entity "Note" "n1" {
    auditor "sanad-auditor"
    date "2026-11-14"
    body {
        isi catatan
    }
}
entity "Doc" "doc" {
    title "Dokumen"
    steps [v1, n1]
}
"#;

    const TOC_DOC: &str = r#"
schema "toc" {
    type "Doc" {
        field "label" text heading
        field "parts" ref "Part" compose
    }
    type "Part" {
        field "label" text heading
        field "content" body compose ["Part", "Step"] optional
        field "steps" ref "Step" compose optional
    }
    type "Step" {
        field "label" text heading
        field "content" body
    }
}

entity "Step" "step-one" {
    field "label" "Step One"
    field "content" """Do the thing."""
}

entity "Step" "step-two" {
    field "label" "Step Two"
    field "content" """Do it again."""
}

entity "Part" "part-alpha" {
    field "label" "Part Alpha"
    field "content" {
        Alpha prose.

        include step-one
    }
    field "steps" [ "step-two" ]
}

entity "Doc" "doc-root" {
    field "label" "Root Doc"
    field "parts" [ "part-alpha" ]
}
"#;

    #[test]
    fn compose_toc_lists_the_composed_heading_tree() {
        let md = compose_toc(&doc(TOC_DOC), "doc-root", &ComposeOptions::default())
            .expect("toc composes");
        let expected = "- Root Doc\n  - Part Alpha\n    - Step One\n    - Step Two\n";
        assert_eq!(md, expected);
    }

    #[test]
    fn compose_toc_numbers_like_the_composed_document() {
        let doc = doc(TOC_DOC);
        let md = compose_toc(
            &doc,
            "doc-root",
            &ComposeOptions { cites: CiteMode::None, number_headings: true, cite_links: None },
        )
        .expect("toc composes");
        let expected = "- Root Doc\n  - 1. Part Alpha\n    - 1.1 Step One\n    - 1.2 Step Two\n";
        assert_eq!(md, expected);
    }

    #[test]
    fn compose_toc_rejects_cycles_like_the_full_composition() {
        let source = r#"
schema "c" {
    type "Node" {
        field "label" text heading
        field "next" ref "Node" compose optional
    }
}

entity "Node" "a" {
    field "label" "A"
    field "next" [ "b" ]
}

entity "Node" "b" {
    field "label" "B"
    field "next" [ "a" ]
}
"#;
        let error = compose_toc(&doc(source), "a", &ComposeOptions::default()).unwrap_err();
        assert!(error.to_string().contains("composition cycle"), "{}", error);
    }

    #[test]
    fn cite_mode_none_renders_no_cite_lines() {
        let md = compose_with(CITE_DOC, &ComposeOptions::default());
        assert!(!md.contains("**Cites:**"), "{}", md);
        assert!(md.contains("kutipan ayat"), "{}", md);
    }

    #[test]
    fn cite_mode_link_renders_target_ids() {
        let md = compose_with(CITE_DOC, &ComposeOptions { cites: CiteMode::Link, number_headings: false, cite_links: None });
        assert!(md.contains("**Cites:** r1"), "{}", md);
    }

    #[test]
    fn cite_mode_link_renders_registered_outputs_as_links() {
        let mut links = std::collections::HashMap::new();
        links.insert("r1".to_string(), "refs/r1.md".to_string());
        let md = compose_with(
            CITE_DOC,
            &ComposeOptions {
                cites: CiteMode::Link,
                number_headings: false,
                cite_links: Some(std::rc::Rc::new(links)),
            },
        );
        assert!(md.contains("**Cites:** [r1](refs/r1.md)"), "{}", md);
    }

    #[test]
    fn cite_mode_link_without_a_registered_output_stays_a_plain_id() {
        let mut links = std::collections::HashMap::new();
        links.insert("other".to_string(), "other.md".to_string());
        let md = compose_with(
            CITE_DOC,
            &ComposeOptions {
                cites: CiteMode::Link,
                number_headings: false,
                cite_links: Some(std::rc::Rc::new(links)),
            },
        );
        assert!(md.contains("**Cites:** r1"), "{}", md);
    }

    #[test]
    fn cite_mode_short_renders_target_headings() {
        let md = compose_with(CITE_DOC, &ComposeOptions { cites: CiteMode::Short, number_headings: false, cite_links: None });
        assert!(
            md.contains("**Cites:** HR. Muslim no. 2653 (Kitab al-Qadar)"),
            "{}",
            md
        );
    }

    #[test]
    fn inline_entities_render_supplied_text_fields_as_metadata() {
        let md = compose_with(CITE_DOC, &ComposeOptions::default());
        assert!(md.contains("**auditor:** sanad-auditor"), "{}", md);
        assert!(md.contains("**date:** 2026-11-14"), "{}", md);
        // The cite-carrying Verse supplies no text fields, so nothing extra
        // appears beyond its label and cite line.
        assert!(!md.contains("**auditor:** sanad-auditor\n\n#### "), "{}", md);
    }

    #[test]
    fn cite_lines_render_between_label_and_body() {
        let md = compose_with(CITE_DOC, &ComposeOptions { cites: CiteMode::Link, number_headings: false, cite_links: None });
        let label_pos = md.find("**REVELATION**").unwrap();
        let cite_pos = md.find("**Cites:**").unwrap();
        let body_pos = md.find("kutipan ayat").unwrap();
        assert!(label_pos < cite_pos && cite_pos < body_pos, "{}", md);
    }

    #[test]
    fn heading_labels_render_verbatim_and_plain_labels_stay_bold() {
        let label = "#### REVELATION";
        let src = r#"
schema "labels" {
    type "Step" label "HEADING_PLACEHOLDER" {
        field "title" text heading
        field "body" body
    }
    type "Note" label "SYNTHESIS" {
        field "title" text heading
        field "body" body
    }
    type "Doc" {
        field "title" text heading
        field "steps" ref "Step" inline optional
        field "notes" ref "Note" inline optional
    }
}
entity "Step" "s1" {
    title "S1"
    body {
        isi langkah
    }
}
entity "Note" "n1" {
    title "N1"
    body {
        isi catatan
    }
}
entity "Doc" "doc" {
    title "Dokumen"
    steps [s1]
    notes [n1]
}
"#.replace("HEADING_PLACEHOLDER", label);
        let file = Path::new("labels.vdl");
        let tokens = crate::lexer::lex(&src, file).expect("lexing should succeed");
        let doc = crate::parser::schema::parse_schema_document(&tokens, &src, file)
            .expect("parsing should succeed");
        let md = compose(&doc, "doc").expect("composition should succeed");
        assert!(md.contains("\n#### REVELATION\n"), "{}", md);
        assert!(!md.contains("**#### REVELATION**"), "{}", md);
        assert!(md.contains("**SYNTHESIS**"), "{}", md);
    }

    #[test]
    fn nested_composition_renders_shared_children_at_their_depth() {
        let source = r#"
schema "nested" {
    type "Item" {
        field "label" text heading
        field "children" ref "Item" compose
        field "related" ref "Item"
    }
}
entity "Item" "root" {
    field "label" "Root"
    field "children" ["left", "right"]
    field "related" ["appendix"]
}
entity "Item" "left" {
    field "label" "Left"
    field "children" ["shared"]
    field "related" ["appendix"]
}
entity "Item" "right" {
    field "label" "Right"
    field "children" ["shared"]
    field "related" []
}
entity "Item" "shared" {
    field "label" "Shared"
    field "children" []
    field "related" []
}
entity "Item" "appendix" {
    field "label" "Appendix"
    field "children" []
    field "related" []
}
"#;
        let markdown = compose(&doc(source), "root").unwrap();
        let body = markdown.split("---\n").nth(2).unwrap();
        assert_eq!(
            body,
            "\n# Root\n\n## Left\n\n### Shared\n\n## Right\n\n### Shared\n"
        );
        assert!(markdown.contains("\"related\":\n  - \"appendix\""));
    }

    #[test]
    fn nested_composition_rejects_a_cycle() {
        let source = r#"
schema "nested" {
    type "Item" {
        field "label" text heading
        field "children" ref "Item" compose
    }

}
entity "Item" "root" { field "label" "Root" field "children" ["child"] }
entity "Item" "child" { field "label" "Child" field "children" ["root"] }
"#;
        let error = compose(&doc(source), "root").unwrap_err();
        assert!(error.to_string().contains("root -> child -> root"));
    }

    #[test]
    fn inline_composition_preserves_mixed_order_without_numbering_blocks() {
        let source = r#"
schema "example" {
    type "Page" {
        field "label" text heading
        field "blocks" ref ["Note", "Quote"] inline
        field "sections" ref "Section" compose
    }
    type "Note" { field "content" body }
    type "Quote" label "[QUOTE]" { field "label" text heading field "content" body }
    type "Section" { field "label" text heading }
}
Page root {
    label "Root"
    blocks [
        quote,
        prose {
            Authored prose.
        },
        note,
        quote
    ]
    sections [section]
}
Quote quote { label "Standalone title" content """> Quoted""" }
Note note { content """An intervening note.""" }
Section section { label "Section" }
"#;
        let doc = doc(source);
        let markdown = compose_with_numbering(&doc, "root", true).unwrap();
        assert!(markdown.ends_with(
            "# Root\n\n**[QUOTE]**\n\n> Quoted\n\nAuthored prose.\n\n\
             An intervening note.\n\n**[QUOTE]**\n\n> Quoted\n\n## 1. Section\n"
        ));
        assert!(compose(&doc, "quote")
            .unwrap()
            .ends_with("# Standalone title\n\n**[QUOTE]**\n\n> Quoted\n"));

        let invalid = source.replace("        note,", "        section,");
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(&invalid, file).unwrap();
        let invalid = crate::parser::schema::parse_schema_document(&tokens, &invalid, file)
            .unwrap();
        let error = compose(&invalid, "root").unwrap_err().to_string();
        assert!(error.contains("expects one of [Note, Quote]"));
    }

    #[test]
    fn root_body_rendered_after_title() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "content" body
    }
}
entity "A" "a" {
    field "label" "Root"
    field "content" """Root body text."""
}
"#;
        let md = compose(&doc(src), "a").unwrap();
        let heading = md.find("# Root").unwrap();
        let body = md.find("Root body text.").unwrap();
        assert!(heading < body);
    }

    #[test]
    fn no_heading_field_means_no_h1() {
        let src = r#"
schema "s" {
    type "A" {
        field "name" text
        field "content" body
    }
}
entity "A" "a" {
    field "name" "Name"
    field "content" """Body."""
}
"#;
        let md = compose(&doc(src), "a").unwrap();
        assert!(!md.contains("# "), "{}", md);
        assert!(md.contains("\"name\": \"Name\""));
    }

    #[test]
    fn sections_render_in_declared_order() {
        let md = render("core-guide");
        assert!(md.find("## Introduction").unwrap() < md.find("## Rules").unwrap());
    }

    #[test]
    fn reordering_references_changes_output() {
        let core = render("core-guide");
        let ops = render("ops-guide");
        assert!(core.find("## Introduction").unwrap() < core.find("## Rules").unwrap());
        assert!(ops.find("## Rules").unwrap() < ops.find("## Introduction").unwrap());
        assert_ne!(core, ops);
    }

    #[test]
    fn shared_section_renders_in_both_documents() {
        let core = render("core-guide");
        let ops = render("ops-guide");
        assert!(core.contains("### Internal heading"));
        assert!(ops.contains("### Internal heading"));
    }

    #[test]
    fn body_markdown_is_not_escaped() {
        let md = render("core-guide");
        assert!(md.contains("### Internal heading"));
        assert!(md.contains("Prose for intro."));
    }

    #[test]
    fn braced_body_includes_render_in_order_but_fenced_and_triple_quoted_text_stays_literal() {
        let source = r#"
package guide
schema "guide" {
    type "Page" { field "title" text heading field "body" body compose ["Artifact"] optional }
    type "Artifact" { field "title" text heading field "body" body optional }
}
Page root {
    title "Root"
    body {
        Before.

        include preface

        ```md
        include preface
        ```

        After.
    }
}
Artifact preface { title "Preface" body """Literal include absent.""" }
"#;
        let rendered = compose_with_numbering(&doc(source), "guide.root", true).unwrap();
        let before = rendered.find("Before.").unwrap();
        let included = rendered.find("## 1. Preface").unwrap();
        let literal = rendered.find("Literal include absent.").unwrap();
        let fenced = rendered.find("include preface").unwrap();
        let after = rendered.find("After.").unwrap();
        assert!(before < included && included < literal && literal < fenced && fenced < after);
        assert_eq!(rendered.matches("## 1. Preface").count(), 1);
    }

    #[test]
    fn body_include_cycles_are_rejected() {
        let source = r#"
package guide
schema "guide" {
    type "Page" { field "title" text heading field "body" body compose ["Page"] optional }
}
Page root {
    title "Root"
    body {
    include root
    }
}
"#;
        let error = compose(&doc(source), "guide.root").unwrap_err().to_string();
        assert!(error.contains("composition cycle"), "{error}");
    }

    #[test]
    fn body_include_missing_and_wrong_type_errors_point_to_directive() {
        let missing = r#"
package guide
schema "guide" { type "Page" { field "title" text heading field "body" body compose ["Artifact"] optional } type "Artifact" { field "title" text heading field "body" body } }
Page root {
    title "Root"
    body {
    include absent
    }
}
"#;
        let file = Path::new("include.vdl");
        let missing_doc = crate::parser::schema::parse_schema_document(&crate::lexer::lex(missing, file).unwrap(), missing, file).unwrap();
        let err = crate::validator::schema::validate_schema(&missing_doc).unwrap_err().to_string();
        assert!(err.contains("include.vdl:7:"), "{err}");
        assert!(err.contains("no matching local entity or import"), "{err}");

        let wrong = missing.replace("include absent", "include root");
        let doc = doc(&wrong);
        let err = crate::validator::schema::validate_schema(&doc).unwrap_err().to_string();
        assert!(err.contains("expected one of [Artifact]"), "{err}");
    }

    #[test]
    fn ordinary_refs_render_ids_without_bodies() {
        let md = render("core-guide");
        assert!(md.contains("  - \"appendix\""));
        assert!(!md.contains("Appendix body that must not be inlined."));
    }

    #[test]
    fn unknown_root_is_an_error() {
        match compose(&doc(DOC), "nope") {
            Err(VdlError::Codegen { message }) => assert!(message.contains("nope")),
            other => panic!("expected codegen error, got {:?}", other),
        }
    }

    #[test]
    fn unvalidated_document_is_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "B" compose }
    type "B" { field "label" text heading }
}
entity "A" "a" {
    field "label" "A"
    field "items" [ "missing" ]
}
"#;
        assert!(compose(&doc(src), "a").is_err());
    }

    #[test]
    fn yaml_special_characters_are_escaped() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading }
}
entity "A" "a" { field "label" "A \"quoted\" \\ thing" }
"#;
        let md = compose(&doc(src), "a").unwrap();
        assert!(md.contains(r#""label": "A \"quoted\" \\ thing""#), "{}", md);
    }

    #[test]
    fn rename_needs_no_code_change() {
        let src = r#"
schema "manuals" {
    type "Manual" {
        field "label" text heading
        field "chapters" ref "Chapter" compose
    }
    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}
entity "Chapter" "one" {
    field "label" "One"
    field "content" """One body."""
}
entity "Manual" "m" {
    field "label" "The Manual"
    field "chapters" [ "one" ]
}
"#;
        let md = compose(&doc(src), "m").unwrap();
        assert!(md.contains("# The Manual"));
        assert!(md.contains("## One"));
        assert!(md.contains("One body."));
        assert!(md.contains("type: \"Manual\""));
    }

    // -- YAML frontmatter safety -------------------------------------------

    #[test]
    fn reserved_id_key_collision_is_an_error() {
        let src = r#"
schema "s" {
    type "A" {
        field "id" text heading
    }
}
entity "A" "a" { field "id" "x" }
"#;
        match compose(&doc(src), "a") {
            Err(VdlError::Codegen { message }) => {
                assert!(message.contains("'id'"), "{}", message);
                assert!(message.contains("compiler-owned"), "{}", message);
            }
            other => panic!("expected codegen error, got {:?}", other),
        }
    }

    #[test]
    fn reserved_type_key_collision_is_an_error() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "type" ref "B"
    }
    type "B" {
        field "label" text heading
    }
}
entity "B" "b" { field "label" "B" }
entity "A" "a" {
    field "label" "A"
    field "type" [ "b" ]
}
"#;
        match compose(&doc(src), "a") {
            Err(VdlError::Codegen { message }) => assert!(message.contains("'type'"), "{}", message),
            other => panic!("expected codegen error, got {:?}", other),
        }
    }

    #[test]
    fn yaml_sensitive_values_stay_strings() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "notes" ref "B"
    }
    type "B" {
        field "label" text heading
    }
}
entity "B" "001" { field "label" "true" }
entity "A" "null" {
    field "label" "001"
    field "notes" [ "001" ]
}
"#;
        let md = compose(&doc(src), "null").unwrap();
        assert!(md.contains(r#"id: "null""#), "{}", md);
        assert!(md.contains(r#"type: "A""#), "{}", md);
        assert!(md.contains(r#""label": "001""#), "{}", md);
        assert!(md.contains(r#"  - "001""#), "{}", md);
    }

    #[test]
    fn yaml_sensitive_text_and_reference_keys_are_quoted() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "True" text
        field "NULL" text
        field "weird: key" text
        field "On" ref "A"
    }
}
entity "A" "a" {
    field "label" "A"
    field "True" "x"
    field "NULL" "null"
    field "weird: key" "y"
    field "On" [ "a" ]
}
"#;
        let md = compose(&doc(src), "a").unwrap();
        assert!(md.contains(r#""True": "x""#), "{}", md);
        assert!(md.contains(r#""NULL": "null""#), "{}", md);
        assert!(md.contains(r#""weird: key": "y""#), "{}", md);
        assert!(md.contains("\"On\":\n  - \"a\"\n"), "{}", md);
    }

    #[test]
    fn yaml_unicode_breaks_and_controls_are_escaped() {
        let src = "
schema \"s\" { type \"A\" { field \"key\u{85}suffix\" text } }
entity \"A\" \"a\" { field \"key\u{85}suffix\" \"a\u{85}b\u{9f}c\u{2028}d\u{2029}e\" }
";
        let md = compose(&doc(src), "a").unwrap();
        assert!(
            md.contains(r#""key\x85suffix": "a\x85b\x9fc\u2028d\u2029e""#),
            "{}",
            md
        );
    }

    #[test]
    fn yaml_long_keys_use_explicit_syntax() {
        let key = "a".repeat(1024);
        assert_eq!(yaml_key(&key), format!("? \"{}\"\n:", key));
    }

    #[test]
    fn control_characters_are_escaped() {
        let src = "schema \"s\" {\n    type \"A\" { field \"label\" text heading }\n}\nentity \"A\" \"a\" { field \"label\" \"line\\twith\\ttabs\" }\n";
        let md = compose(&doc(src), "a").unwrap();
        let slash_t = format!("{}{}", '\\', 't');
        assert!(
            md.contains(&format!(
                "\"label\": \"line{}with{}tabs\"",
                slash_t, slash_t
            )),
            "{}",
            md
        );
    }
}
