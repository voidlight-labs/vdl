//! Graph construction over a validated schema document.
//!
//! Entities become nodes; every reference-field target becomes a directed
//! edge labeled with the field name. Nothing here knows any particular type
//! or field name: the model is read entirely from the document's schema.
//!
//! Renderers live next door: [`json`] and [`dot`] serialize the graph, and
//! [`search`] builds and queries a content index over the same document.

pub mod dot;
pub mod json;
pub mod search;

use crate::error::{VdlError, VdlResult};
use crate::model::{SchemaDoc, Value};
use std::collections::HashSet;

/// Output format for serialized graph artifacts, shared by the CLI and the
/// build manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum GraphFormat {
    /// JSON node and edge list.
    #[default]
    Json,
    /// Graphviz DOT digraph.
    Dot,
    /// JSON search index over entity content.
    Index,
}

impl GraphFormat {
    /// Parse a format name as written on the CLI or in `vdl.toml`.
    pub fn parse(raw: &str) -> Option<GraphFormat> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "json" => Some(GraphFormat::Json),
            "dot" => Some(GraphFormat::Dot),
            "index" => Some(GraphFormat::Index),
            _ => None,
        }
    }
}

/// One entity in the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The entity's stable ID, unique across the project after validation.
    pub id: String,
    /// The entity's declared type name.
    pub type_name: String,
    /// The type's heading field value, when declared and supplied.
    pub label: Option<String>,
}

/// A directed reference from one entity to another.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// The reference field the edge came through.
    pub field: String,
}

/// The project graph: entities and their reference relationships.
#[derive(Debug, Clone, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    /// Build the graph from a validated schema document.
    ///
    /// Nodes follow entity declaration order. Edges follow entity order, then
    /// field declaration order, then the authored reference order inside each
    /// value (plain reference lists and inline lists alike).
    pub fn build(doc: &SchemaDoc) -> Graph {
        let mut nodes = Vec::with_capacity(doc.entities.len());
        let mut edges = Vec::new();
        for entity in &doc.entities {
            let type_decl = doc.type_decl(&entity.type_name);
            let label = type_decl
                .and_then(|t| t.heading_field())
                .and_then(|f| entity.value(&f.name))
                .and_then(|value| match value {
                    Value::Text(text) => Some(text.clone()),
                    _ => None,
                });
            nodes.push(Node {
                id: entity.id.clone(),
                type_name: entity.type_name.clone(),
                label,
            });

            if let Some(type_decl) = type_decl {
                for field in type_decl.ref_fields() {
                    if let Some(value) = entity.value(&field.name) {
                        for target in value.reference_ids() {
                            edges.push(Edge {
                                from: entity.id.clone(),
                                to: target.to_string(),
                                field: field.name.clone(),
                            });
                        }
                    }
                }
            }
        }
        Graph { nodes, edges }
    }

    /// Narrow the graph to `root` and its one-hop neighborhood: the entities
    /// the root references and the entities referencing the root. Only edges
    /// whose both endpoints survive are kept, in original order.
    ///
    /// # Errors
    ///
    /// [`VdlError::Graph`] when `root` is not a declared entity ID.
    pub fn one_hop(&self, root: &str) -> VdlResult<Graph> {
        let kept = self.one_hop_ids(root)?;
        let nodes = self
            .nodes
            .iter()
            .filter(|n| kept.contains(n.id.as_str()))
            .cloned()
            .collect();
        let edges = self
            .edges
            .iter()
            .filter(|e| kept.contains(e.from.as_str()) && kept.contains(e.to.as_str()))
            .cloned()
            .collect();
        Ok(Graph { nodes, edges })
    }

    /// The one-hop neighborhood IDs of `root`, root included. All IDs are
    /// borrowed from this graph, never from `root`.
    fn one_hop_ids(&self, root: &str) -> VdlResult<HashSet<&str>> {
        let root_id = self
            .nodes
            .iter()
            .find(|n| n.id == root)
            .map(|n| n.id.as_str())
            .ok_or_else(|| VdlError::Graph {
                message: format!("entity not found: \"{}\"", root),
            })?;
        let mut kept: HashSet<&str> = HashSet::new();
        kept.insert(root_id);
        for edge in &self.edges {
            if edge.from == root_id {
                kept.insert(edge.to.as_str());
            }
            if edge.to == root_id {
                kept.insert(edge.from.as_str());
            }
        }
        Ok(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::schema::validate_schema;
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

entity "Chapter" "rules" {
    field "label" "Rules"
    field "content" """Body of rules."""
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "chapters" [ "intro", "rules" ]
    field "see_also" [ "intro" ]
}
"#;

    const INLINE_FORM: &str = r#"
schema "flows" {
    type "Flow" {
        field "label" text heading
        field "steps" ref "Step" inline
    }
    type "Step" {
        field "label" text heading
        field "content" body
    }
}

entity "Step" "first" {
    field "label" "First"
    field "content" """Step body."""
}

entity "Flow" "main-flow" {
    field "label" "Main Flow"
    field "steps" [
        "first",
        prose {
            A short note between references.
        },
    ]
}
"#;

    fn doc(src: &str) -> SchemaDoc {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(src, file).expect("lexing should succeed");
        let doc = crate::parser::schema::parse_schema_document(&tokens, src, file)
            .expect("parsing should succeed");
        validate_schema(&doc).expect("fixtures should validate");
        doc
    }

    #[test]
    fn nodes_follow_declaration_order_with_heading_labels() {
        let graph = Graph::build(&doc(BASE));
        assert_eq!(
            graph
                .nodes
                .iter()
                .map(|n| (n.id.as_str(), n.label.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                ("intro", Some("Introduction")),
                ("rules", Some("Rules")),
                ("core-guide", Some("Core Guide")),
            ]
        );
    }

    #[test]
    fn edges_follow_entity_field_and_authored_order() {
        let graph = Graph::build(&doc(BASE));
        assert_eq!(
            graph
                .edges
                .iter()
                .map(|e| (e.from.as_str(), e.to.as_str(), e.field.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("core-guide", "intro", "chapters"),
                ("core-guide", "rules", "chapters"),
                ("core-guide", "intro", "see_also"),
            ]
        );
    }

    #[test]
    fn inline_references_become_edges() {
        let graph = Graph::build(&doc(INLINE_FORM));
        assert_eq!(
            graph
                .edges
                .iter()
                .map(|e| (e.from.as_str(), e.to.as_str(), e.field.as_str()))
                .collect::<Vec<_>>(),
            vec![("main-flow", "first", "steps")]
        );
    }

    #[test]
    fn one_hop_keeps_both_directions_and_inner_edges_only() {
        let graph = Graph::build(&doc(BASE)).one_hop("intro").expect("root exists");
        assert_eq!(
            graph.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["intro", "core-guide"]
        );
        // core-guide -> intro survives; core-guide -> rules drops (rules is
        // outside the neighborhood).
        assert_eq!(
            graph
                .edges
                .iter()
                .map(|e| (e.from.as_str(), e.to.as_str(), e.field.as_str()))
                .collect::<Vec<_>>(),
            vec![("core-guide", "intro", "chapters"), ("core-guide", "intro", "see_also")]
        );
    }

    #[test]
    fn one_hop_rejects_unknown_root() {
        let error = Graph::build(&doc(BASE)).one_hop("nope").unwrap_err();
        assert!(
            error.to_string().contains("entity not found: \"nope\""),
            "{}",
            error
        );
    }

    #[test]
    fn building_is_deterministic() {
        let source = doc(BASE);
        assert_eq!(Graph::build(&source), Graph::build(&source));
    }
}
