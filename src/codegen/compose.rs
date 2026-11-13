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
use crate::model::{body_include_directives, resolve_include_candidates, InlineItem, Instance, SchemaDoc, TypeDecl, Value};
use crate::validator::schema::validate_schema;

/// Compose the document rooted at `root_id`.
///
/// # Errors
///
/// Returns the schema validation error when the document is not valid, and
/// [`VdlError::Codegen`] when the root or a composed target cannot be resolved
/// or lacks a heading value, or when composition cycles or exceeds six levels.
pub fn compose(doc: &SchemaDoc, root_id: &str) -> VdlResult<String> {
    compose_with_numbering(doc, root_id, false)
}

pub(crate) fn compose_with_numbering(
    doc: &SchemaDoc,
    root_id: &str,
    number_headings: bool,
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
        number_headings.then_some(""),
    )?;

    let mut output = frontmatter;
    if !blocks.is_empty() {
        output.push('\n');
        output.push_str(&blocks.join("\n\n"));
        output.push('\n');
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
    ancestors.push(id);
    let mut child_index = 0;
    if let Some(body_field) = entity_type.body_field() {
        if let Some(value @ (Value::Body(_) | Value::BracedBody(_))) = entity.value(&body_field.name) {
            let body = match value { Value::Body(body) | Value::BracedBody(body) => body, _ => unreachable!() };
            if !body.trim().is_empty() {
                if body_field.compose_targets.is_empty() || !matches!(value, Value::BracedBody(_)) {
                    blocks.push(labelled_body(entity_type, body));
                } else {
                    render_body_includes(doc, entity, entity_type, body, ancestors, blocks, section_number, &mut child_index)?;
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
                        blocks.push(inline_body(doc, id)?);
                    }
                }
                Value::Inline(items) => {
                    for item in items {
                        blocks.push(match item {
                            InlineItem::Ref(id) => inline_body(doc, id)?,
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
            )?;
        }
    }
    ancestors.pop();
    Ok(())
}

fn render_body_includes<'doc>(
    doc: &'doc SchemaDoc, entity: &'doc Instance, entity_type: &'doc TypeDecl, body: &str,
    ancestors: &mut Vec<&'doc str>, blocks: &mut Vec<String>, section_number: Option<&str>, child_index: &mut usize,
) -> VdlResult<()> {
    if let Some(label) = &entity_type.label { blocks.push(format!("**{}**", label)); }
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
            render_entity(doc, target, target_type, ancestors, blocks, number.as_deref())?;
        } else { prose.push(line.to_owned()); }
    }
    if !prose.join("\n").trim().is_empty() { blocks.push(prose.join("\n")); }
    Ok(())
}

fn inline_body(doc: &SchemaDoc, id: &str) -> VdlResult<String> {
    let body = doc.instance(id).and_then(|target| {
        let target_type = doc.type_decl(&target.type_name)?;
        let field = target_type.body_field()?;
        match target.value(&field.name)? {
            Value::Body(body) | Value::BracedBody(body) => Some(labelled_body(target_type, body)),
            _ => None,
        }
    });
    body.ok_or_else(|| VdlError::Codegen {
        message: format!("inline entity '{}' has no body value", id),
    })
}

fn labelled_body(entity_type: &TypeDecl, body: &str) -> String {
    match &entity_type.label {
        Some(label) => format!("**{}**\n\n{}", label, body),
        None => body.to_owned(),
    }
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
fn yaml_double_quoted(value: &str) -> String {
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
