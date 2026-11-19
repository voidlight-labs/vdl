//! Field-population and reference health report over a validated schema.
//!
//! The report answers two questions the compiler can check mechanically:
//! which optional fields are left unfilled, and which entities are never
//! referenced by any reference field. Nothing here knows a particular type or
//! field name; everything is read from the declared schema.

use crate::graph::Graph;
use crate::model::SchemaDoc;
use serde::Serialize;

/// Population of one declared field across the entities of its type.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldReport {
    pub field: String,
    /// The declared field kind, as written in the schema.
    pub kind: String,
    /// How many entities of this type supply the field.
    pub supplied: usize,
    /// How many entities of this type exist.
    pub total: usize,
    /// Whether the field may be omitted per its declaration.
    pub optional: bool,
}

/// Population report for one declared type.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TypeReport {
    pub type_name: String,
    pub entities: usize,
    pub fields: Vec<FieldReport>,
}

/// The full health report: population per type plus unreferenced entities.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub entities: usize,
    pub types: Vec<TypeReport>,
    /// Entities no reference field points at, in declaration order.
    pub orphans: Vec<String>,
}

/// Build the report from a validated document.
///
/// Types and entities follow declaration order; orphan detection uses the
/// project graph, so plain and inline references count alike.
pub fn build_report(doc: &SchemaDoc) -> Report {
    let mut types = Vec::with_capacity(doc.schema.types.len());
    for type_decl in &doc.schema.types {
        let members: Vec<&crate::model::Instance> = doc
            .entities
            .iter()
            .filter(|entity| entity.type_name == type_decl.name)
            .collect();
        let total = members.len();
        let fields = type_decl
            .fields
            .iter()
            .map(|field| FieldReport {
                field: field.name.clone(),
                kind: field.kind.to_string(),
                supplied: members
                    .iter()
                    .filter(|entity| entity.value_count(&field.name) > 0)
                    .count(),
                total,
                optional: field.optional,
            })
            .collect();
        types.push(TypeReport {
            type_name: type_decl.name.clone(),
            entities: total,
            fields,
        });
    }

    let graph = Graph::build(doc);
    let referenced: std::collections::HashSet<&str> = graph
        .edges
        .iter()
        .map(|edge| edge.to.as_str())
        .collect();
    let orphans = graph
        .nodes
        .iter()
        .filter(|node| !referenced.contains(node.id.as_str()))
        .map(|node| node.id.clone())
        .collect();

    Report {
        entities: doc.entities.len(),
        types,
        orphans,
    }
}

/// Render the report as an aligned text table.
pub fn to_text(report: &Report) -> String {
    let mut text = String::new();
    for type_report in &report.types {
        text.push_str(&format!(
            "{} ({} entities)\n",
            type_report.type_name, type_report.entities
        ));
        let width = type_report
            .fields
            .iter()
            .map(|field| field.field.len())
            .max()
            .unwrap_or(0);
        for field in &type_report.fields {
            let status = if field.optional { "optional" } else { "required" };
            text.push_str(&format!(
                "  {:width$}  {}/{}  {} [{}]\n",
                field.field,
                field.supplied,
                field.total,
                status,
                field.kind,
                width = width
            ));
        }
    }
    if report.orphans.is_empty() {
        text.push_str("No unreferenced entities.\n");
    } else {
        text.push_str(&format!(
            "Never referenced ({}):\n",
            report.orphans.len()
        ));
        for orphan in &report.orphans {
            text.push_str(&format!("  {orphan}\n"));
        }
    }
    text
}

/// Serialize the report as pretty-printed JSON.
pub fn to_json(report: &Report) -> String {
    let mut json = serde_json::to_string_pretty(report).expect("serializing plain data cannot fail");
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
        field "summary" text optional
        field "chapters" ref "Chapter" compose
        field "see_also" ref "Chapter" optional
    }
    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}

entity "Chapter" "intro" {
    field "label" "Introduction"
    field "content" """Body."""
}

entity "Chapter" "rules" {
    field "label" "Rules"
    field "content" """Body."""
}

entity "Chapter" "unused" {
    field "label" "Unused"
    field "content" """Body."""
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "summary" "Start here."
    field "chapters" [ "intro", "rules" ]
}

entity "Runbook" "ops-guide" {
    field "label" "Ops Guide"
    field "chapters" [ "rules" ]
}
"#;

    fn doc() -> SchemaDoc {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(SOURCE, file).expect("lexing should succeed");
        let doc = crate::parser::schema::parse_schema_document(&tokens, SOURCE, file)
            .expect("parsing should succeed");
        validate_schema(&doc).expect("fixture should validate");
        doc
    }

    #[test]
    fn population_counts_match_the_schema() {
        let report = build_report(&doc());
        assert_eq!(report.entities, 5);

        let runbook = &report.types[0];
        assert_eq!(runbook.type_name, "Runbook");
        assert_eq!(runbook.entities, 2);
        let label = &runbook.fields[0];
        assert_eq!((label.supplied, label.total, label.optional), (2, 2, false));
        let summary = &runbook.fields[1];
        assert_eq!((summary.supplied, summary.total, summary.optional), (1, 2, true));
        let see_also = &runbook.fields[3];
        assert_eq!(see_also.supplied, 0);
    }

    #[test]
    fn orphans_are_entities_no_reference_points_at() {
        let report = build_report(&doc());
        // The runbooks reference chapters but nothing references a runbook:
        // roots are in the list by design, next to the truly forgotten one.
        assert_eq!(report.orphans, vec!["unused", "core-guide", "ops-guide"]);
    }

    #[test]
    fn text_render_is_stable() {
        let expected = r#"Runbook (2 entities)
  label     2/2  required [text]
  summary   1/2  optional [text]
  chapters  2/2  required [ref Chapter]
  see_also  0/2  optional [ref Chapter]
Chapter (3 entities)
  label    3/3  required [text]
  content  3/3  required [body]
Never referenced (3):
  unused
  core-guide
  ops-guide
"#;
        assert_eq!(to_text(&build_report(&doc())), expected);
    }

    #[test]
    fn json_is_a_pretty_object() {
        let json = to_json(&build_report(&doc()));
        assert!(
            json.contains("\"orphans\": [\n    \"unused\",\n    \"core-guide\",\n    \"ops-guide\"\n  ]"),
            "{json}"
        );
        assert!(json.contains("\"kind\": \"ref Chapter\""), "{json}");
    }
}
