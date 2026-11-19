//! JSON serialization of the project graph.

use super::{Edge, Graph, Node};
use serde::Serialize;

#[derive(Serialize)]
struct JsonGraph<'a> {
    generator: Generator,
    nodes: Vec<JsonNode<'a>>,
    edges: Vec<JsonEdge<'a>>,
}

#[derive(Serialize)]
struct Generator {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct JsonNode<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    type_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
}

#[derive(Serialize)]
struct JsonEdge<'a> {
    from: &'a str,
    to: &'a str,
    field: &'a str,
}

/// Serialize the graph as pretty-printed JSON.
///
/// The output carries no timestamp, so equal inputs produce equal bytes.
pub fn to_json(graph: &Graph) -> String {
    let output = JsonGraph {
        generator: Generator {
            name: "vdl",
            version: env!("CARGO_PKG_VERSION"),
        },
        nodes: graph
            .nodes
            .iter()
            .map(
                |Node {
                     id, type_name, label,
                 }| JsonNode {
                    id,
                    type_name,
                    label: label.as_deref(),
                },
            )
            .collect(),
        edges: graph
            .edges
            .iter()
            .map(|Edge { from, to, field }| JsonEdge { from, to, field })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&output)
        .expect("serializing plain data cannot fail");
    json.push('\n');
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SOURCE: &str = r#"
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "chapters" ref "Chapter"
    }
    type "Chapter" {
        field "label" text heading
        field "note" text
    }
}

entity "Chapter" "intro" {
    field "label" "Introduction"
    field "note" "Plain note."
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "chapters" [ "intro" ]
}
"#;

    fn graph() -> Graph {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(SOURCE, file).expect("lexing should succeed");
        let doc = crate::parser::schema::parse_schema_document(&tokens, SOURCE, file)
            .expect("parsing should succeed");
        Graph::build(&doc)
    }

    #[test]
    fn json_shape_is_stable_and_ordered() {
        let expected = r#"{
  "generator": {
    "name": "vdl",
    "version": "0.1.0"
  },
  "nodes": [
    {
      "id": "intro",
      "type": "Chapter",
      "label": "Introduction"
    },
    {
      "id": "core-guide",
      "type": "Runbook",
      "label": "Core Guide"
    }
  ],
  "edges": [
    {
      "from": "core-guide",
      "to": "intro",
      "field": "chapters"
    }
  ]
}
"#;
        assert_eq!(to_json(&graph()), expected);
    }

    #[test]
    fn nodes_without_headings_omit_the_label() {
        let source = r#"
schema "items" {
    type "Item" {
        field "name" text heading optional
        field "note" text
    }
}

entity "Item" "labelled" {
    field "name" "Named"
    field "note" "Has a label."
}

entity "Item" "plain" {
    field "note" "No label."
}
"#;
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(source, file).expect("lexing should succeed");
        let doc = crate::parser::schema::parse_schema_document(&tokens, source, file)
            .expect("parsing should succeed");
        crate::validator::schema::validate_schema(&doc).expect("fixture should validate");
        let json = to_json(&Graph::build(&doc));
        assert!(
            json.contains("{\n      \"id\": \"plain\",\n      \"type\": \"Item\"\n    }"),
            "label must be omitted for heading-less nodes: {json}"
        );
        assert!(json.contains("\"label\": \"Named\""), "{json}");
    }
}
