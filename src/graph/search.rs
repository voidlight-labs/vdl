//! Search index over entity content.
//!
//! Every entity becomes one searchable document whose content gathers its
//! ID, type name, heading label, text field values, and body. Queries are
//! case-insensitive substrings over that content, so equal inputs produce
//! equal index bytes and results follow declaration order.

use crate::model::{SchemaDoc, Value};
use serde::Serialize;

/// One entity prepared for search.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchDocument {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Everything searchable about the entity: ID, type name, heading, text
    /// fields, and body, each on its own line.
    pub content: String,
}

/// Build the search index in entity declaration order.
pub fn build_index(doc: &SchemaDoc) -> Vec<SearchDocument> {
    doc.entities
        .iter()
        .map(|entity| {
            let type_decl = doc.type_decl(&entity.type_name);
            let label = type_decl
                .and_then(|t| t.heading_field())
                .and_then(|f| entity.value(&f.name))
                .and_then(|value| match value {
                    Value::Text(text) => Some(text.clone()),
                    _ => None,
                });

            let mut parts: Vec<&str> = vec![&entity.id, &entity.type_name];
            if let Some(label) = &label {
                parts.push(label);
            }
            if let Some(type_decl) = type_decl {
                for field in &type_decl.fields {
                    // The heading already entered content as the label.
                    if field.heading {
                        continue;
                    }
                    if let Some(value) = entity.value(&field.name) {
                        match value {
                            Value::Text(text) | Value::Body(text) | Value::BracedBody(text) => {
                                parts.push(text)
                            }
                            Value::Refs(_) | Value::Inline(_) => {}
                        }
                    }
                }
            }
            let content = parts.join("\n");
            SearchDocument {
                id: entity.id.clone(),
                type_name: entity.type_name.clone(),
                label,
                content,
            }
        })
        .collect()
}

/// Case-insensitive substring match over the document's content.
pub fn matches(document: &SearchDocument, query: &str) -> bool {
    document.content.to_lowercase().contains(&query.to_lowercase())
}

/// Serialize documents as a pretty-printed JSON array.
pub fn to_json(documents: &[SearchDocument]) -> String {
    let mut json =
        serde_json::to_string_pretty(documents).expect("serializing plain data cannot fail");
    json.push('\n');
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::schema::validate_schema;
    use std::path::Path;

    const SOURCE: &str = r#"
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "summary" text
        field "chapters" ref "Chapter"
    }
    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}

entity "Chapter" "intro" {
    field "label" "Introduction"
    field "content" """Onboarding prose lives here."""
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "summary" "Start reading here."
    field "chapters" [ "intro" ]
}
"#;

    fn index() -> Vec<SearchDocument> {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(SOURCE, file).expect("lexing should succeed");
        let doc = crate::parser::schema::parse_schema_document(&tokens, SOURCE, file)
            .expect("parsing should succeed");
        validate_schema(&doc).expect("fixtures should validate");
        build_index(&doc)
    }

    #[test]
    fn content_gathers_id_type_heading_text_and_body() {
        let intro = &index()[0];
        assert_eq!(
            intro.content,
            "intro\nChapter\nIntroduction\nOnboarding prose lives here."
        );
    }

    #[test]
    fn reference_values_do_not_leak_into_content() {
        let core = &index()[1];
        assert!(!core.content.contains("intro"), "{}", core.content);
    }

    #[test]
    fn matching_is_case_insensitive_over_all_content() {
        let intro = &index()[0];
        assert!(matches(intro, "onboarding"));
        assert!(matches(intro, "CHAPTER"));
        assert!(matches(intro, "introduction"));
        assert!(!matches(intro, "missing-phrase"));
    }

    #[test]
    fn json_is_a_pretty_array_in_declaration_order() {
        let expected = r#"[
  {
    "id": "intro",
    "type": "Chapter",
    "label": "Introduction",
    "content": "intro\nChapter\nIntroduction\nOnboarding prose lives here."
  },
  {
    "id": "core-guide",
    "type": "Runbook",
    "label": "Core Guide",
    "content": "core-guide\nRunbook\nCore Guide\nStart reading here."
  }
]
"#;
        assert_eq!(to_json(&index()), expected);
    }
}
