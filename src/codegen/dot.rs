use crate::error::{VdlError, VdlResult};
use crate::graph::{entity_type_color, KnowledgeGraph};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Escape double quotes in a string for safe use inside DOT quoted strings.
fn escape_dot_label(label: &str) -> String {
    label.replace('"', "\\\"")
}

/// Generate a Graphviz DOT file for visualization.
///
/// Produces a `digraph` with:
/// - Nodes color-coded by entity type
/// - Edges labeled with relationship type
/// - A legend explaining the color scheme
pub fn generate(graph: &KnowledgeGraph, output_path: &Path) -> VdlResult<()> {
    let file = File::create(output_path).map_err(|e| VdlError::Codegen {
        message: format!(
            "failed to create DOT file at {}: {e}",
            output_path.display()
        ),
    })?;

    let mut w = BufWriter::new(file);

    writeln!(&mut w, "digraph VDL_Knowledge_Graph {{").map_err(io_to_codegen)?;
    writeln!(&mut w, "    rankdir=TB;").map_err(io_to_codegen)?;
    writeln!(&mut w, "    node [shape=box, style=filled, fontname=\"Helvetica\"];")
        .map_err(io_to_codegen)?;
    writeln!(&mut w, "    edge [fontname=\"Helvetica\", fontsize=10];").map_err(io_to_codegen)?;
    writeln!(&mut w).map_err(io_to_codegen)?;

    // Nodes
    for (id, entity) in &graph.nodes {
        let label = if entity.title.is_empty() {
            id.as_str()
        } else {
            entity.title.as_str()
        };
        let safe_label = escape_dot_label(label);
        let color = entity_type_color(entity.entity_type);
        writeln!(
            &mut w,
            "    \"{id}\" [label=\"{safe_label}\", fillcolor=\"{color}\"];"
        )
        .map_err(io_to_codegen)?;
    }

    if !graph.nodes.is_empty() && !graph.edges.is_empty() {
        writeln!(&mut w).map_err(io_to_codegen)?;
    }

    // Edges
    for edge in &graph.edges {
        let rel_label = edge.rel_type.to_string();
        let safe_rel_label = escape_dot_label(&rel_label);
        writeln!(
            &mut w,
            "    \"{from}\" -> \"{to}\" [label=\"{safe_rel_label}\"];",
            from = edge.from,
            to = edge.to
        )
        .map_err(io_to_codegen)?;
    }

    writeln!(&mut w, "}}").map_err(io_to_codegen)?;
    w.flush().map_err(io_to_codegen)?;

    Ok(())
}

fn io_to_codegen(e: std::io::Error) -> VdlError {
    VdlError::Codegen {
        message: format!("I/O error while writing DOT file: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Edge;
    use crate::parser::ast::{Entity, EntityType, RelationshipType};
    use crate::test_helpers::test_entity;

    fn make_entity(id: &str, entity_type: EntityType, title: &str) -> Entity {
        let mut e = test_entity(id, entity_type, "1.0");
        e.title = title.to_string();
        e.description = String::new();
        e
    }

    fn make_graph() -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::new();

        let law = make_entity("soul.law.i", EntityType::Law, "Autonomy Is Mandatory");
        let framework = make_entity(
            "voidlight_constitution",
            EntityType::Framework,
            "The Voidlight Constitution",
        );
        let axiom = make_entity("core.axiom.1", EntityType::Axiom, "");
        let principle = make_entity("core.principle.1", EntityType::Principle, "Discipline First");
        let concept = make_entity("core.concept.1", EntityType::Concept, "Concept A");
        let artifact = make_entity("core.artifact.1", EntityType::Artifact, "Artifact A");
        let pillar = make_entity("core.pillar.1", EntityType::Pillar, "Pillar A");

        graph.nodes.insert(law.id.clone(), law);
        graph.nodes.insert(framework.id.clone(), framework);
        graph.nodes.insert(axiom.id.clone(), axiom);
        graph.nodes.insert(principle.id.clone(), principle);
        graph.nodes.insert(concept.id.clone(), concept);
        graph.nodes.insert(artifact.id.clone(), artifact);
        graph.nodes.insert(pillar.id.clone(), pillar);

        graph.edges.push(Edge {
            from: "soul.law.i".to_string(),
            to: "voidlight_constitution".to_string(),
            rel_type: RelationshipType::DerivesFrom,
        });
        graph.edges.push(Edge {
            from: "core.axiom.1".to_string(),
            to: "core.principle.1".to_string(),
            rel_type: RelationshipType::Requires,
        });

        graph
    }

    #[test]
    fn test_generate_dot_valid_syntax() {
        let graph = make_graph();
        let tmp = tempfile::NamedTempFile::with_suffix(".dot").unwrap();
        let path = tmp.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();

        // Must be a valid digraph
        assert!(content.contains("digraph VDL_Knowledge_Graph {"));
        assert!(content.contains("}"));

        // Must contain node definitions with quoted IDs
        assert!(content.contains("\"soul.law.i\""));
        assert!(content.contains("\"voidlight_constitution\""));

        // Must contain edge arrows
        assert!(content.contains("\"soul.law.i\" -> \"voidlight_constitution\""));
        assert!(content.contains("\"core.axiom.1\" -> \"core.principle.1\""));
    }

    #[test]
    fn test_node_colors_match_entity_types() {
        let graph = make_graph();
        let tmp = tempfile::NamedTempFile::with_suffix(".dot").unwrap();
        let path = tmp.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();

        assert!(content.contains("fillcolor=\"#E8D5B7\"")); // axiom
        assert!(content.contains("fillcolor=\"#D4A373\"")); // framework
        assert!(content.contains("fillcolor=\"#C75B39\"")); // law
        assert!(content.contains("fillcolor=\"#6B8E23\"")); // principle
        assert!(content.contains("fillcolor=\"#5F9EA0\"")); // concept
        assert!(content.contains("fillcolor=\"#9370DB\"")); // artifact
        assert!(content.contains("fillcolor=\"#2F4F4F\"")); // pillar
    }

    #[test]
    fn test_node_labels_use_title_or_id() {
        let mut graph = KnowledgeGraph::new();
        let with_title = make_entity("id1", EntityType::Concept, "My Title");
        let no_title = make_entity("id2", EntityType::Axiom, "");

        graph.nodes.insert(with_title.id.clone(), with_title);
        graph.nodes.insert(no_title.id.clone(), no_title);

        let tmp = tempfile::NamedTempFile::with_suffix(".dot").unwrap();
        let path = tmp.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();

        assert!(content.contains("label=\"My Title\""));
        assert!(content.contains("label=\"id2\""));
    }

    #[test]
    fn test_edge_labels() {
        let graph = make_graph();
        let tmp = tempfile::NamedTempFile::with_suffix(".dot").unwrap();
        let path = tmp.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();

        assert!(content.contains("label=\"derives_from\""));
        assert!(content.contains("label=\"requires\""));
    }

    #[test]
    fn test_quote_escaping_in_labels() {
        let mut graph = KnowledgeGraph::new();
        let entity = make_entity("id1", EntityType::Law, r#"Say "hello" now"#);
        graph.nodes.insert(entity.id.clone(), entity);

        let tmp = tempfile::NamedTempFile::with_suffix(".dot").unwrap();
        let path = tmp.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();

        // The raw label should contain escaped quotes
        assert!(content.contains(r#"label="Say \"hello\" now""#));
    }

    #[test]
    fn test_empty_graph_produces_valid_dot() {
        let graph = KnowledgeGraph::new();
        let tmp = tempfile::NamedTempFile::with_suffix(".dot").unwrap();
        let path = tmp.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("digraph VDL_Knowledge_Graph {"));
        assert!(content.contains("}"));
    }

    #[test]
    fn test_default_color_for_unmapped_types() {
        let mut graph = KnowledgeGraph::new();
        let entity = make_entity("doc1", EntityType::Document, "Doc A");
        graph.nodes.insert(entity.id.clone(), entity);

        let tmp = tempfile::NamedTempFile::with_suffix(".dot").unwrap();
        let path = tmp.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("fillcolor=\"#999999\""));
    }
}
