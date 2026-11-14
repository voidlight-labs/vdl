//! Schema-driven validation for schema-authored VDL documents.
//!
//! All rules read the authored schema. Nothing here knows a domain: no type
//! name, field name, or composition target is compared against a literal.
//!
//! | Rule | Rejects |
//! |------|---------|
//! | V1  | instance of an undeclared type |
//! | V2  | duplicate entity ID |
//! | V3  | duplicate type name |
//! | V4  | duplicate field name inside one type |
//! | V5  | field supplied that its type does not declare |
//! | V6  | required field not supplied |
//! | V7  | value shape does not match the declared field kind |
//! | V8  | reference to an undeclared entity ID |
//! | V9  | reference whose target entity has a different type |
//! | V10 | `compose` on a non-`ref` field |
//! | V11 | `heading` on a non-`text` field, or more than one `heading` per type |
//! | V12 | composed target lacks the heading or body required by the composition mode |
//! | V13 | `ref` field whose declared target type is undeclared (empty list included) |
//! | V14 | the same field supplied twice in one instance |
//! | V16 | `cite` on a non-`ref` field |

use crate::error::{SourceLocation, VdlError, VdlResult};
use crate::model::{body_include_directives, resolve_include_candidates, FieldKind, SchemaDoc, TypeDecl, Value};
use crate::validator::error::ValidationError;
use std::collections::HashMap;

fn violation(location: &SourceLocation, message: impl Into<String>) -> ValidationError {
    ValidationError::Schema {
        location: location.clone(),
        message: message.into(),
    }
}

/// Collect every schema violation without short-circuiting.
pub fn collect_schema_errors(doc: &SchemaDoc) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    errors.extend(check_type_declarations(doc));
    errors.extend(check_composition_targets(doc));
    errors.extend(check_instances(doc));
    errors.extend(check_references(doc));
    errors.extend(check_body_includes(doc));
    errors
}

fn check_body_includes(doc: &SchemaDoc) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for instance in &doc.entities {
        let Some(type_decl) = doc.type_decl(&instance.type_name) else { continue };
        let Some(field) = type_decl.body_field().filter(|f| !f.compose_targets.is_empty()) else { continue };
        let Some(Value::BracedBody(body)) = instance.value(&field.name) else { continue };
        let value_location = instance.values.iter().find(|v| v.name == field.name).map(|v| &v.source_location).unwrap_or(&instance.source_location);
        for (line_offset, name) in body_include_directives(body) {
            let location = SourceLocation::new(value_location.file.clone(), value_location.line + line_offset + 1, value_location.column);
            let candidates = resolve_include_candidates(instance, name, doc);
            match candidates.as_slice() {
                [id] => match doc.instance(id) {
                    None => errors.push(violation(&location, format!("body include '{}' references missing entity '{}'", name, id))),
                    Some(target) if !field.compose_targets.contains(&target.type_name) => errors.push(violation(&location, format!("body include '{}' has type '{}'; expected one of [{}]", name, target.type_name, field.compose_targets.join(", ")))),
                    _ => {}
                },
                [] => errors.push(violation(&location, format!("body include '{}' has no matching local entity or import", name))),
                many => errors.push(violation(&location, format!("body include '{}' is ambiguous; candidates: {}", name, many.join(", ")))),
            }
        }
    }
    errors
}

/// Validate a schema document, reporting every violation at once.
///
/// # Errors
///
/// Returns [`VdlError::Validation`] for a single violation and
/// [`VdlError::ValidationErrors`] when several are found.
pub fn validate_schema(doc: &SchemaDoc) -> VdlResult<()> {
    let mut errors = collect_schema_errors(doc);
    if errors.is_empty() {
        return Ok(());
    }

    if errors.len() == 1 {
        return Err(errors.remove(0).into());
    }

    let count = errors.len();
    let messages = errors
        .into_iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    Err(VdlError::ValidationErrors { count, messages })
}

/// V3, V4, V10, V11, V13: rules that only need the schema block.
fn check_type_declarations(doc: &SchemaDoc) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut seen_types: HashMap<&str, ()> = HashMap::new();

    for type_decl in &doc.schema.types {
        if seen_types.insert(type_decl.name.as_str(), ()).is_some() {
            errors.push(violation(
                &type_decl.source_location,
                format!("duplicate type declaration '{}'", type_decl.name),
            ));
        }

        let mut seen_fields: HashMap<&str, ()> = HashMap::new();
        for field in &type_decl.fields {
            if seen_fields.insert(field.name.as_str(), ()).is_some() {
                errors.push(violation(
                    &field.source_location,
                    format!(
                        "type '{}' declares field '{}' more than once",
                        type_decl.name, field.name
                    ),
                ));
            }

            // V10
            if field.compose && field.kind.reference_targets().is_none()
                && !(matches!(field.kind, FieldKind::Body) && !field.compose_targets.is_empty()) {
                let modifier = if field.inline { "inline" } else { "compose" };
                errors.push(violation(
                    &field.source_location,
                    format!(
                        "field '{}' of type '{}' is marked {} but has kind '{}'; {} applies to ref fields only",
                        field.name, type_decl.name, modifier, field.kind, modifier
                    ),
                ));
            }
            if !field.compose_targets.is_empty() && !matches!(field.kind, FieldKind::Body) {
                errors.push(violation(&field.source_location, format!("field '{}' declares body composition targets but has kind '{}'", field.name, field.kind)));
            }

            // V11
            if field.heading && !matches!(field.kind, FieldKind::Text) {
                errors.push(violation(
                    &field.source_location,
                    format!(
                        "field '{}' of type '{}' is marked heading but has kind '{}'; heading applies to text fields only",
                        field.name, type_decl.name, field.kind
                    ),
                ));
            }

            // V16
            if field.cite && field.kind.reference_targets().is_none() {
                errors.push(violation(
                    &field.source_location,
                    format!(
                        "field '{}' of type '{}' is marked cite but has kind '{}'; cite applies to ref fields only",
                        field.name, type_decl.name, field.kind
                    ),
                ));
            }

            // V13: declared target type must exist even when no instance
            // supplies a value, so a schema typo cannot slip through.
            for target in field.kind.reference_targets().into_iter().flatten() {
                if doc.type_decl(target).is_none() {
                    errors.push(violation(
                        &field.source_location,
                        format!(
                            "field '{}' of type '{}' references undeclared type '{}'",
                            field.name, type_decl.name, target
                        ),
                    ));
                }
            }
            for target in &field.compose_targets {
                if doc.type_decl(target).is_none() {
                    errors.push(violation(&field.source_location, format!("body compose field '{}' of type '{}' references undeclared target type '{}'", field.name, type_decl.name, target)));
                }
            }
        }

        // V11: at most one heading field per type.
        let headings: Vec<&str> = type_decl
            .fields
            .iter()
            .filter(|f| f.heading)
            .map(|f| f.name.as_str())
            .collect();
        if headings.len() > 1 {
            errors.push(violation(
                &type_decl.source_location,
                format!(
                    "type '{}' declares {} heading fields ({}); at most one is allowed",
                    type_decl.name,
                    headings.len(),
                    headings.join(", ")
                ),
            ));
        }

        // V15: the renderer emits a single body, so an ambiguous declaration is
        // rejected even when the type has no instances.
        let bodies: Vec<&str> = type_decl
            .fields
            .iter()
            .filter(|f| matches!(f.kind, FieldKind::Body))
            .map(|f| f.name.as_str())
            .collect();
        if bodies.len() > 1 {
            errors.push(violation(
                &type_decl.source_location,
                format!(
                    "type '{}' declares {} body fields ({}); at most one is allowed",
                    type_decl.name,
                    bodies.len(),
                    bodies.join(", ")
                ),
            ));
        }
    }

    errors
}

/// V12: a composed target type must declare the field the renderer needs.
fn check_composition_targets(doc: &SchemaDoc) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for type_decl in &doc.schema.types {
        for field in type_decl.composed_fields() {
            for target in field.kind.reference_targets().into_iter().flatten() {
                let Some(target_decl) = doc.type_decl(target) else {
                    continue;
                };
                let (required, missing) = if field.inline {
                    ("body", target_decl.body_field().is_none())
                } else {
                    ("heading", target_decl.heading_field().is_none())
                };
                if missing {
                    errors.push(violation(
                        &field.source_location,
                        format!(
                            "field '{}' of type '{}' is composed but target type '{}' declares no {} field",
                            field.name, type_decl.name, target, required
                        ),
                    ));
                }
            }
        }
        for field in type_decl.fields.iter().filter(|f| !f.compose_targets.is_empty()) {
            for target in &field.compose_targets {
                let Some(target_decl) = doc.type_decl(target) else { continue };
                if target_decl.heading_field().is_none() {
                    errors.push(violation(&field.source_location, format!("body compose field '{}' target type '{}' declares no heading field", field.name, target)));
                }
            }
        }
    }

    errors
}

/// V1, V2, V5, V6, V7, V14: instance-level rules.
fn check_instances(doc: &SchemaDoc) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut seen_ids: HashMap<&str, &SourceLocation> = HashMap::new();

    for instance in &doc.entities {
        // V2: report the duplicate at its own location and name the first
        // declaration, so a project-wide duplicate names both files.
        if let Some(first) = seen_ids.insert(instance.id.as_str(), &instance.source_location) {
            errors.push(violation(
                &instance.source_location,
                format!(
                    "duplicate entity ID '{}'; first declared at {}",
                    instance.id, first
                ),
            ));
        }

        // V1
        let Some(type_decl) = doc.type_decl(&instance.type_name) else {
            errors.push(violation(
                &instance.source_location,
                format!(
                    "entity '{}' has undeclared type '{}'",
                    instance.id, instance.type_name
                ),
            ));
            // Field-level checks would only add noise for an unknown type.
            continue;
        };

        // V14
        let mut seen_values: HashMap<&str, usize> = HashMap::new();
        for value in &instance.values {
            *seen_values.entry(value.name.as_str()).or_insert(0) += 1;
        }
        for (name, count) in &seen_values {
            if *count > 1 {
                errors.push(violation(
                    &instance.source_location,
                    format!(
                        "entity '{}' supplies field '{}' {} times; each field may be set once",
                        instance.id, name, count
                    ),
                ));
            }
        }

        for value in &instance.values {
            // V5
            let Some(field_decl) = type_decl.field(&value.name) else {
                errors.push(violation(
                    &value.source_location,
                    format!(
                        "entity '{}' supplies field '{}' which type '{}' does not declare",
                        instance.id, value.name, type_decl.name
                    ),
                ));
                continue;
            };

            // V7
            let shape_ok = match (&field_decl.kind, &value.value) {
                (FieldKind::Text, Value::Text(_)) => true,
                (FieldKind::Body, Value::Body(_) | Value::BracedBody(_)) => true,
                (kind, Value::Refs(_)) => kind.reference_targets().is_some(),
                (kind, Value::Inline(_)) => field_decl.inline && kind.reference_targets().is_some(),
                _ => false,
            };
            if !shape_ok {
                errors.push(violation(
                    &value.source_location,
                    format!(
                        "entity '{}' field '{}' expects {} but was given {}",
                        instance.id,
                        value.name,
                        field_decl.kind,
                        describe_value(&value.value)
                    ),
                ));
            }
        }

        // V6
        for field_decl in &type_decl.fields {
            if !field_decl.optional && instance.value_count(&field_decl.name) == 0 {
                errors.push(violation(
                    &instance.source_location,
                    format!(
                        "entity '{}' is missing required field '{}' declared by type '{}'",
                        instance.id, field_decl.name, type_decl.name
                    ),
                ));
            }
        }
    }

    errors
}

/// V8, V9: reference resolution and target typing.
fn check_references(doc: &SchemaDoc) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for instance in &doc.entities {
        let Some(type_decl) = doc.type_decl(&instance.type_name) else {
            continue;
        };

        for value in &instance.values {
            let Some(field_decl) = type_decl.field(&value.name) else {
                continue;
            };
            let Some(targets) = field_decl.kind.reference_targets() else {
                continue;
            };
            for id in value.value.reference_ids() {
                match doc.instance(id) {
                    // V8
                    None => errors.push(violation(
                        &value.source_location,
                        format!(
                            "entity '{}' field '{}' references unresolved entity '{}'",
                            instance.id, value.name, id
                        ),
                    )),
                    // V9
                    Some(referenced) if !targets.contains(&referenced.type_name) => {
                        let expected = if let [target] = targets {
                            format!("type '{}'", target)
                        } else {
                            format!("one of [{}]", targets.join(", "))
                        };
                        errors.push(violation(
                            &value.source_location,
                            format!(
                                "entity '{}' field '{}' expects {} but '{}' is of type '{}'",
                                instance.id, value.name, expected, id, referenced.type_name
                            ),
                        ));
                    }
                    Some(referenced) if field_decl.inline => {
                        let body = doc
                            .type_decl(&referenced.type_name)
                            .and_then(TypeDecl::body_field);
                        if let Some(body) = body {
                                if !matches!(referenced.value(&body.name), Some(Value::Body(_) | Value::BracedBody(_))) {
                                errors.push(violation(
                                    &value.source_location,
                                    format!(
                                        "entity '{}' field '{}' inlines '{}' which has no body value",
                                        instance.id, value.name, id
                                    ),
                                ));
                            }
                        }
                    }
                    Some(_) => {}
                }
            }
        }
    }

    errors
}

fn describe_value(value: &Value) -> &'static str {
    match value {
        Value::Text(_) => "text",
        Value::Body(_) => "body",
        Value::BracedBody(_) => "braced body",
        Value::Refs(_) => "ref list",
        Value::Inline(_) => "inline list with prose",
    }
}

/// Convenience for callers that also need the declared type of an instance.
pub fn instance_type_decl<'a>(doc: &'a SchemaDoc, instance_id: &str) -> Option<&'a TypeDecl> {
    let instance = doc.instance(instance_id)?;
    doc.type_decl(&instance.type_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const BASE: &str = r#"
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
    field "content" """Body of intro."""
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "chapters" [ "intro" ]
    field "see_also" [ "intro" ]
}
"#;

    fn doc(src: &str) -> SchemaDoc {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(src, file).expect("lexing should succeed");
        crate::parser::schema::parse_schema_document(&tokens, src, file)
            .expect("parsing should succeed")
    }

    fn messages(src: &str) -> Vec<String> {
        collect_schema_errors(&doc(src))
            .into_iter()
            .map(|e| e.to_string())
            .collect()
    }

    fn assert_single(src: &str, needle: &str) {
        let msgs = messages(src);
        assert_eq!(msgs.len(), 1, "expected exactly one error, got {:?}", msgs);
        assert!(
            msgs[0].contains(needle),
            "expected message containing {:?}, got {:?}",
            needle,
            msgs[0]
        );
    }

    #[test]
    fn valid_document_passes() {
        assert!(messages(BASE).is_empty(), "{:?}", messages(BASE));
    }

    // -- V1 ---------------------------------------------------------------

    #[test]
    fn v1_unknown_type_rejected() {
        let src = r#"
schema "s" { type "A" { field "label" text heading } }
entity "B" "b" { field "label" "x" }
"#;
        assert_single(src, "undeclared type 'B'");
    }

    // -- V2 ---------------------------------------------------------------

    #[test]
    fn v2_duplicate_entity_id_rejected() {
        let src = r#"
schema "s" { type "A" { field "label" text heading } }
entity "A" "same" { field "label" "one" }
entity "A" "same" { field "label" "two" }
"#;
        assert_single(src, "duplicate entity ID 'same'");
    }

    #[test]
    fn v2_duplicate_entity_id_names_the_first_location() {
        let src = r#"
schema "s" { type "A" { field "label" text heading } }
entity "A" "same" { field "label" "one" }
entity "A" "same" { field "label" "two" }
"#;
        let msgs = messages(src);
        assert_eq!(msgs.len(), 1, "{:?}", msgs);
        assert!(
            msgs[0].contains("first declared at test.vdl:"),
            "{:?}",
            msgs[0]
        );
    }

    // -- V3 ---------------------------------------------------------------

    #[test]
    fn v3_duplicate_type_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading }
    type "A" { field "label" text heading }
}
"#;
        assert_single(src, "duplicate type declaration 'A'");
    }

    // -- V4 ---------------------------------------------------------------

    #[test]
    fn v4_duplicate_field_declaration_rejected() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "label" text
    }
}
"#;
        assert_single(src, "declares field 'label' more than once");
    }

    // -- V5 ---------------------------------------------------------------

    #[test]
    fn v5_unknown_field_rejected() {
        let src = r#"
schema "s" { type "A" { field "label" text heading } }
entity "A" "a" {
    field "label" "x"
    field "surprise" "y"
}
"#;
        assert_single(src, "supplies field 'surprise'");
    }

    // -- V6 ---------------------------------------------------------------

    #[test]
    fn v6_missing_field_rejected() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "note" text
    }
}
entity "A" "a" { field "label" "x" }
"#;
        assert_single(src, "missing required field 'note'");
    }

    // -- V7 ---------------------------------------------------------------

    #[test]
    fn v7_shape_mismatch_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading }
    type "B" { field "label" text heading }
}
entity "A" "a" { field "label" """body where text expected""" }
"#;
        assert_single(src, "expects text but was given body");
    }

    #[test]
    fn v7_ref_given_text_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "B" }
    type "B" { field "label" text heading }
}
entity "B" "b" { field "label" "b" }
entity "A" "a" {
    field "label" "a"
    field "items" "b"
}
"#;
        assert_single(src, "expects ref B but was given text");
    }

    // -- V8 ---------------------------------------------------------------

    #[test]
    fn v8_unresolved_reference_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "B" }
    type "B" { field "label" text heading }
}
entity "A" "a" {
    field "label" "a"
    field "items" [ "missing" ]
}
"#;
        assert_single(src, "unresolved entity 'missing'");
    }

    // -- V9 ---------------------------------------------------------------

    #[test]
    fn v9_wrong_target_type_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "B" }
    type "B" { field "label" text heading }
}
entity "A" "other" {
    field "label" "other"
    field "items" [ ]
}
entity "A" "a" {
    field "label" "a"
    field "items" [ "other" ]
}
"#;
        assert_single(src, "expects type 'B' but 'other' is of type 'A'");
    }

    // -- V10 --------------------------------------------------------------

    #[test]
    fn v10_compose_on_text_rejected() {
        let src = r#"
schema "s" { type "A" { field "label" text compose } }
"#;
        assert_single(src, "compose applies to ref fields only");
    }

    // -- V11 --------------------------------------------------------------

    #[test]
    fn v11_heading_on_ref_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "B" heading }
    type "B" { field "label" text heading }
}
"#;
        // Two violations follow from one mistake: heading on a ref field, and
        // two heading fields on the type. Both are reported.
        let msgs = messages(src);
        assert!(
            msgs.iter()
                .any(|m| m.contains("heading applies to text fields only")),
            "{:?}",
            msgs
        );
        assert!(
            msgs.iter().any(|m| m.contains("declares 2 heading fields")),
            "{:?}",
            msgs
        );
    }

    #[test]
    fn v11_two_heading_fields_rejected() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "alias" text heading
    }
}
"#;
        assert_single(src, "declares 2 heading fields");
    }

    // -- V16 --------------------------------------------------------------

    #[test]
    fn v16_cite_on_text_rejected() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "refs" text cite
    }
}
"#;
        assert_single(src, "cite applies to ref fields only");
    }

    #[test]
    fn v16_cite_on_ref_field_accepted() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "refs" ref "B" cite optional
        field "body" body
    }
    type "B" {
        field "label" text heading
        field "body" body
    }
}
entity "A" "a" {
    label "A"
    refs [b]
    body {
        isi
    }
}
entity "B" "b" {
    label "B"
    body {
        isi
    }
}
"#;
        assert!(messages(src).is_empty(), "{:?}", messages(src));
    }

    // -- V12 --------------------------------------------------------------

    #[test]
    fn v12_inline_target_must_declare_a_body() {
        let source = r#"
schema "example" {
    type "Page" { field "items" ref ["Entry", "Pointer"] inline }
    type "Entry" { field "content" body }
    type "Pointer" { field "label" text heading }
}
"#;
        assert_single(source, "target type 'Pointer' declares no body field");
    }

    #[test]
    fn v12_inline_target_must_supply_its_optional_body() {
        let source = r#"
schema "example" {
    type "Page" { field "items" ref "Entry" inline }
    type "Entry" { field "content" body optional }
}
Page page { items [entry] }
Entry entry {}
"#;
        assert_single(source, "inlines 'entry' which has no body value");
    }

    #[test]
    fn v12_composed_target_without_heading_rejected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "B" compose }
    type "B" { field "note" text }
}
"#;
        assert_single(src, "declares no heading field");
    }

    #[test]
    fn v12_not_triggered_for_non_composed_ref() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "B" }
    type "B" { field "note" text }
}
entity "B" "b" { field "note" "n" }
entity "A" "a" {
    field "label" "a"
    field "items" [ "b" ]
}
"#;
        assert!(messages(src).is_empty(), "{:?}", messages(src));
    }

    // -- V13 --------------------------------------------------------------

    #[test]
    fn v13_fires_for_empty_reference_list() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading field "items" ref "Typo" }
}
entity "A" "a" {
    field "label" "a"
    field "items" [ ]
}
"#;
        assert_single(src, "references undeclared type 'Typo'");
    }

    // -- V15 --------------------------------------------------------------

    #[test]
    fn v15_multiple_body_fields_rejected() {
        let src = r#"
schema "s" {
    type "A" {
        field "label" text heading
        field "first" body
        field "second" body
    }
}
"#;
        assert_single(src, "declares 2 body fields (first, second)");
    }

    // -- V14 --------------------------------------------------------------

    #[test]
    fn v14_duplicate_field_value_rejected() {
        let src = r#"
schema "s" { type "A" { field "label" text heading } }
entity "A" "a" {
    field "label" "one"
    field "label" "two"
}
"#;
        assert_single(src, "supplies field 'label' 2 times");
    }

    // -- Collection -------------------------------------------------------

    #[test]
    fn multiple_violations_are_collected() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading }
    type "A" { field "label" text heading }
}
entity "B" "b" { field "label" "x" }
"#;
        assert_eq!(messages(src).len(), 2);
    }

    #[test]
    fn validate_schema_reports_validation_errors_variant() {
        let src = r#"
schema "s" {
    type "A" { field "label" text heading }
    type "A" { field "label" text heading }
}
entity "B" "b" { field "label" "x" }
"#;
        match validate_schema(&doc(src)) {
            Err(VdlError::ValidationErrors { count, .. }) => assert_eq!(count, 2),
            other => panic!("expected ValidationErrors, got {:?}", other),
        }
    }
}
