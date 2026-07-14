use crate::error::{VdlError, VdlResult};
use crate::graph::KnowledgeGraph;
use crate::parser::ast::{Entity, EvidenceBlock};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// A single searchable document in the search index.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct SearchDocument {
    id: String,
    title: String,
    description: String,
    content_text: String,
    #[serde(rename = "type")]
    doc_type: String,
    tags: Vec<String>,
}

/// Generate the search index output.
///
/// Produces a flat JSON array of searchable documents:
/// ```json
/// [
///   { "id": "...", "title": "...", "description": "...",
///     "content_text": "...", "type": "...", "tags": [] }
/// ]
/// ```
pub fn generate(graph: &KnowledgeGraph, output_path: &Path) -> VdlResult<()> {
    let documents: Vec<SearchDocument> = graph
        .nodes
        .values()
        .map(entity_to_document)
        .collect();

    let json = serde_json::to_string_pretty(&documents)
        .map_err(|e| VdlError::Codegen {
            message: format!("Failed to serialize search index: {}", e),
        })?;

    let mut file = File::create(output_path).map_err(|e| VdlError::Codegen {
        message: format!(
            "Failed to create search index file at '{}': {}",
            output_path.display(),
            e
        ),
    })?;

    file.write_all(json.as_bytes()).map_err(|e| VdlError::Codegen {
        message: format!(
            "Failed to write search index file at '{}': {}",
            output_path.display(),
            e
        ),
    })?;

    Ok(())
}

/// Convert a single entity into a searchable document.
fn entity_to_document(entity: &Entity) -> SearchDocument {
    let content_text = build_content_text(entity);
    let tags = build_tags(entity);

    SearchDocument {
        id: entity.id.clone(),
        title: entity.title.clone(),
        description: entity.description.clone(),
        content_text,
        doc_type: entity.entity_type.to_string(),
        tags,
    }
}

/// Build the searchable content text from an entity.
///
/// Concatenates title, description, and all evidence text into a single string.
fn build_content_text(entity: &Entity) -> String {
    let mut parts = Vec::new();

    parts.push(entity.title.clone());
    parts.push(entity.description.clone());

    if let Some(evidence) = &entity.evidence {
        append_evidence_text(evidence, &mut parts);
    }

    parts.join(" ")
}

/// Append evidence text parts to the content builder.
fn append_evidence_text(evidence: &EvidenceBlock, parts: &mut Vec<String>) {
    for revelation in &evidence.revelations {
        parts.push(format!("Revelation: {}", revelation.source));
        parts.push(revelation.text.clone());
        if let Some(ref translator) = revelation.translator {
            parts.push(format!("Translator: {}", translator));
        }
    }

    for synthesis in &evidence.syntheses {
        let sources_text = synthesis.sources.join(", ");
        parts.push(format!("Synthesis ({}): {}", sources_text, synthesis.argument));
    }

    for analogy in &evidence.analogies {
        parts.push(format!(
            "Analogy ({}): {}",
            analogy.domain, analogy.mapping
        ));
    }
}

/// Build the tag list from an entity's annotations and type.
///
/// Collects values from `@author`, `@pillar`, and `@status` annotations,
/// plus the entity type itself. Tags are deduplicated while preserving order.
fn build_tags(entity: &Entity) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();

    for annotation in &entity.annotations {
        let tag_value = match annotation.name.as_str() {
            "author" | "pillar" | "status" => Some(annotation.value.clone()),
            _ => None,
        };

        if let Some(value) = tag_value {
            if seen.insert(value.clone()) {
                tags.push(value);
            }
        }
    }

    let type_tag = entity.entity_type.to_string();
    if seen.insert(type_tag.clone()) {
        tags.push(type_tag);
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceLocation;
    use crate::parser::ast::{
        Analogy, Annotation, Entity, EntityType, EvidenceBlock, Revelation, Synthesis,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn dummy_location() -> SourceLocation {
        SourceLocation::new(PathBuf::from("test.vdl"), 1, 1)
    }

    fn create_test_entity() -> Entity {
        Entity {
            id: "soul.law.i".to_string(),
            entity_type: EntityType::Law,
            version: "1.0".to_string(),
            title: "Autonomy Is Mandatory".to_string(),
            description: "Every node must retain sovereign control.".to_string(),
            properties: HashMap::new(),
            relationships: vec![],
            evidence: Some(EvidenceBlock {
                revelations: vec![Revelation {
                    source: "Quran 2:30".to_string(),
                    text: "And when your Lord said to the angels...".to_string(),
                    translator: Some("Sahih International".to_string()),
                    source_location: dummy_location(),
                }],
                syntheses: vec![Synthesis {
                    sources: vec!["Ibn Kathir".to_string(), "Al-Tabari".to_string()],
                    argument: "The verse establishes divine delegation.".to_string(),
                    source_location: dummy_location(),
                }],
                analogies: vec![Analogy {
                    domain: "Governance".to_string(),
                    mapping: "A steward is not the owner.".to_string(),
                    source_location: dummy_location(),
                }],
            }),
            annotations: vec![
                Annotation {
                    name: "author".to_string(),
                    value: "Khayren".to_string(),
                    source_location: dummy_location(),
                },
                Annotation {
                    name: "pillar".to_string(),
                    value: "soul".to_string(),
                    source_location: dummy_location(),
                },
                Annotation {
                    name: "status".to_string(),
                    value: "canonical".to_string(),
                    source_location: dummy_location(),
                },
            ],
            source_location: dummy_location(),
        }
    }

    #[test]
    fn test_generate_search_index_structure() {
        let mut graph = KnowledgeGraph::new();
        let entity = create_test_entity();
        graph.nodes.insert(entity.id.clone(), entity);

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("search_index.json");

        generate(&graph, &output_path).unwrap();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let docs: Vec<SearchDocument> = serde_json::from_str(&contents).unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id, "soul.law.i");
        assert_eq!(docs[0].title, "Autonomy Is Mandatory");
        assert_eq!(docs[0].description, "Every node must retain sovereign control.");
        assert_eq!(docs[0].doc_type, "law");
    }

    #[test]
    fn test_content_text_includes_evidence() {
        let mut graph = KnowledgeGraph::new();
        let entity = create_test_entity();
        graph.nodes.insert(entity.id.clone(), entity);

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("search_index.json");

        generate(&graph, &output_path).unwrap();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let docs: Vec<SearchDocument> = serde_json::from_str(&contents).unwrap();

        let content = &docs[0].content_text;
        assert!(content.contains("Autonomy Is Mandatory"));
        assert!(content.contains("Every node must retain sovereign control."));
        assert!(content.contains("Quran 2:30"));
        assert!(content.contains("And when your Lord said to the angels..."));
        assert!(content.contains("Sahih International"));
        assert!(content.contains("Ibn Kathir"));
        assert!(content.contains("Al-Tabari"));
        assert!(content.contains("The verse establishes divine delegation."));
        assert!(content.contains("Governance"));
        assert!(content.contains("A steward is not the owner."));
    }

    #[test]
    fn test_tags_include_annotations_and_entity_type() {
        let mut graph = KnowledgeGraph::new();
        let entity = create_test_entity();
        graph.nodes.insert(entity.id.clone(), entity);

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("search_index.json");

        generate(&graph, &output_path).unwrap();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let docs: Vec<SearchDocument> = serde_json::from_str(&contents).unwrap();

        let tags = &docs[0].tags;
        assert!(tags.contains(&"Khayren".to_string()));
        assert!(tags.contains(&"soul".to_string()));
        assert!(tags.contains(&"canonical".to_string()));
        assert!(tags.contains(&"law".to_string()));
    }

    #[test]
    fn test_tags_deduplicated() {
        let mut graph = KnowledgeGraph::new();
        let mut entity = create_test_entity();
        // Add a duplicate annotation to test deduplication
        entity.annotations.push(Annotation {
            name: "author".to_string(),
            value: "Khayren".to_string(),
            source_location: dummy_location(),
        });
        graph.nodes.insert(entity.id.clone(), entity);

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("search_index.json");

        generate(&graph, &output_path).unwrap();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let docs: Vec<SearchDocument> = serde_json::from_str(&contents).unwrap();

        let tags = &docs[0].tags;
        let khayren_count = tags.iter().filter(|t| t == &&"Khayren".to_string()).count();
        assert_eq!(khayren_count, 1, "Duplicate tag 'Khayren' should be deduplicated");
    }

    #[test]
    fn test_empty_evidence_handled() {
        let mut graph = KnowledgeGraph::new();
        let mut entity = create_test_entity();
        entity.evidence = None;
        graph.nodes.insert(entity.id.clone(), entity);

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("search_index.json");

        generate(&graph, &output_path).unwrap();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let docs: Vec<SearchDocument> = serde_json::from_str(&contents).unwrap();

        assert_eq!(docs[0].content_text, "Autonomy Is Mandatory Every node must retain sovereign control.");
    }

    #[test]
    fn test_pretty_printed_json() {
        let mut graph = KnowledgeGraph::new();
        let entity = create_test_entity();
        graph.nodes.insert(entity.id.clone(), entity);

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("search_index.json");

        generate(&graph, &output_path).unwrap();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        // Pretty-printed JSON should contain newlines and 2-space indentation
        assert!(contents.contains('\n'));
        assert!(contents.contains("  \"id\""));
    }

    #[test]
    fn test_multiple_entities() {
        let mut graph = KnowledgeGraph::new();

        let entity1 = create_test_entity();
        let mut entity2 = create_test_entity();
        entity2.id = "soul.law.ii".to_string();
        entity2.title = "Mutual Respect Is Required".to_string();
        entity2.entity_type = EntityType::Principle;

        graph.nodes.insert(entity1.id.clone(), entity1);
        graph.nodes.insert(entity2.id.clone(), entity2);

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("search_index.json");

        generate(&graph, &output_path).unwrap();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let docs: Vec<SearchDocument> = serde_json::from_str(&contents).unwrap();

        assert_eq!(docs.len(), 2);
        assert!(docs.iter().any(|d| d.id == "soul.law.i"));
        assert!(docs.iter().any(|d| d.id == "soul.law.ii"));
    }
}
