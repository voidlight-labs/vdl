//! Graphviz DOT serialization of the project graph.

use super::Graph;
use std::collections::HashMap;

/// Fill colors cycled over declared types, assigned by first appearance.
/// The schema form has user-defined types, so there is no fixed color map.
const PALETTE: [&str; 8] = [
    "#4C72B0", "#DD8452", "#55A868", "#C44E52", "#8172B3", "#937860", "#DA8BC3", "#8C8C8C",
];

/// Serialize the graph as a Graphviz `digraph`.
///
/// Node names are entity IDs; labels prefer the heading value and fall back
/// to the ID. Colors are assigned per type in first-seen order, so equal
/// inputs produce equal bytes.
pub fn to_dot(graph: &Graph) -> String {
    let mut palette_index = 0usize;
    let mut type_colors: HashMap<&str, &'static str> = HashMap::new();
    for node in &graph.nodes {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            type_colors.entry(node.type_name.as_str())
        {
            let color = PALETTE[palette_index % PALETTE.len()];
            palette_index += 1;
            entry.insert(color);
        }
    }

    let mut dot = String::from(
        "digraph VDL_Graph {\n\
         \x20   rankdir=TB;\n\
         \x20   node [shape=box, style=\"rounded,filled\", fontname=\"Helvetica\"];\n\
         \x20   edge [fontname=\"Helvetica\", fontsize=10];\n",
    );
    dot.push('\n');
    for node in &graph.nodes {
        let color = type_colors[node.type_name.as_str()];
        let label = node.label.as_deref().unwrap_or(&node.id);
        dot.push_str(&format!(
            "    \"{}\" [label=\"{}\", fillcolor=\"{}\"];\n",
            escape(&node.id),
            escape(label),
            color
        ));
    }
    if !graph.edges.is_empty() {
        dot.push('\n');
    }
    for edge in &graph.edges {
        dot.push_str(&format!(
            "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
            escape(&edge.from),
            escape(&edge.to),
            escape(&edge.field)
        ));
    }
    dot.push_str("}\n");
    dot
}

/// Escape a string for a quoted DOT string literal.
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
        field "chapters" ref "Chapter"
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
}
"#;

    fn graph_of(source: &str) -> Graph {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(source, file).expect("lexing should succeed");
        let doc = crate::parser::schema::parse_schema_document(&tokens, source, file)
            .expect("parsing should succeed");
        validate_schema(&doc).expect("fixtures should validate");
        Graph::build(&doc)
    }

    #[test]
    fn dot_shape_is_stable_and_ordered() {
        let expected = r##"digraph VDL_Graph {
    rankdir=TB;
    node [shape=box, style="rounded,filled", fontname="Helvetica"];
    edge [fontname="Helvetica", fontsize=10];

    "intro" [label="Introduction", fillcolor="#4C72B0"];
    "core-guide" [label="Core Guide", fillcolor="#DD8452"];

    "core-guide" -> "intro" [label="chapters"];
}
"##;
        assert_eq!(to_dot(&graph_of(SOURCE)), expected);
    }

    #[test]
    fn labels_fall_back_to_the_id_when_no_heading_is_supplied() {
        let source = r#"
schema "items" {
    type "Item" {
        field "name" text heading optional
        field "next" ref "Item" optional
    }
}

entity "Item" "named" {
    field "name" "Named"
}

entity "Item" "plain" {
}
"#;
        let expected = r##"digraph VDL_Graph {
    rankdir=TB;
    node [shape=box, style="rounded,filled", fontname="Helvetica"];
    edge [fontname="Helvetica", fontsize=10];

    "named" [label="Named", fillcolor="#4C72B0"];
    "plain" [label="plain", fillcolor="#4C72B0"];
}
"##;
        assert_eq!(to_dot(&graph_of(source)), expected);
    }

    #[test]
    fn escaping_handles_backslashes_and_quotes() {
        assert_eq!(escape("plain"), "plain");
        assert_eq!(escape("a\"b"), "a\\\"b");
        assert_eq!(escape("a\\b"), "a\\\\b");
        assert_eq!(escape("say \"hi\" \\ ok"), "say \\\"hi\\\" \\\\ ok");
    }
}
