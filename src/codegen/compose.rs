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
//!   `## <its heading field>` and its body verbatim;
//! * nothing else is expanded, so an ordinary reference never inlines the
//!   target's body.
//!
//! Composition is one level deep.

use crate::error::{VdlError, VdlResult};
use crate::model::{FieldKind, Instance, SchemaDoc, TypeDecl, Value};
use crate::validator::schema::validate_schema;

/// Compose the document rooted at `root_id`.
///
/// # Errors
///
/// Returns the schema validation error when the document is not valid, and
/// [`VdlError::Codegen`] when the root or a composed target cannot be resolved
/// or lacks a heading value.
pub fn compose(doc: &SchemaDoc, root_id: &str) -> VdlResult<String> {
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

    let mut blocks: Vec<String> = Vec::new();

    if let Some(heading_value) = heading_text(root, root_type) {
        blocks.push(format!("# {}", heading_value));
    }
    if let Some(body) = body_text(root, root_type) {
        blocks.push(body);
    }

    for field in root_type.composed_fields() {
        let FieldKind::Refs { .. } = field.kind else {
            continue;
        };
        let Some(Value::Refs(ids)) = root.value(&field.name) else {
            continue;
        };

        for id in ids {
            let target = doc.instance(id).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "entity '{}' field '{}' references unresolved entity '{}'",
                    root.id, field.name, id
                ),
            })?;
            let target_type = doc.type_decl(&target.type_name).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "entity '{}' has undeclared type '{}'",
                    target.id, target.type_name
                ),
            })?;

            let heading = heading_text(target, target_type).ok_or_else(|| VdlError::Codegen {
                message: format!(
                    "composed entity '{}' of type '{}' has no heading value",
                    target.id, target_type.name
                ),
            })?;
            blocks.push(format!("## {}", heading));

            if let Some(body) = body_text(target, target_type) {
                blocks.push(body);
            }
        }
    }

    let mut output = frontmatter;
    if !blocks.is_empty() {
        output.push('\n');
        output.push_str(&blocks.join("\n\n"));
        output.push('\n');
    }
    Ok(output)
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
                "{}: {}\n",
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
        out.push_str(&format!("{}:\n", yaml_key(&field.name)));
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

/// The type's `body` field value, if it declares one and it is not empty.
fn body_text(instance: &Instance, type_decl: &TypeDecl) -> Option<String> {
    let field = type_decl.body_field()?;
    match instance.value(&field.name) {
        Some(Value::Body(body)) if !body.trim().is_empty() => Some(body.clone()),
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
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Serialize an author-defined key, quoting it unless it is unambiguously plain.
fn yaml_key(name: &str) -> String {
    if is_plain_yaml_key(name) {
        name.to_string()
    } else {
        yaml_double_quoted(name)
    }
}

fn is_plain_yaml_key(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return false;
    }
    // Words YAML would resolve to a non-string scalar.
    !matches!(
        name,
        "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n"
    )
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
        let label_pos = md.find("label: \"Core Guide\"").unwrap();
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
        assert!(md.contains("name: \"Name\""));
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
        assert!(md.contains(r#"label: "A \"quoted\" \\ thing""#), "{}", md);
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
        assert!(md.contains(r#"label: "001""#), "{}", md);
        assert!(md.contains(r#"  - "001""#), "{}", md);
    }

    #[test]
    fn keys_requiring_quotes_are_quoted() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "true" text
        field "weird: key" text
    }
}
entity "A" "a" {
    field "label" "A"
    field "true" "x"
    field "weird: key" "y"
}
"#;
        let md = compose(&doc(src), "a").unwrap();
        assert!(md.contains(r#""true": "x""#), "{}", md);
        assert!(md.contains(r#""weird: key": "y""#), "{}", md);
    }

    #[test]
    fn control_characters_are_escaped() {
        let src = "schema \"s\" {\n    type \"A\" { field \"label\" text heading }\n}\nentity \"A\" \"a\" { field \"label\" \"line\\twith\\ttabs\" }\n";
        let md = compose(&doc(src), "a").unwrap();
        let slash_t = format!("{}{}", '\\', 't');
        assert!(
            md.contains(&format!("label: \"line{}with{}tabs\"", slash_t, slash_t)),
            "{}",
            md
        );
    }
}
