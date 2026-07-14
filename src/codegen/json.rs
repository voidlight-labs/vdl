use crate::error::{VdlError, VdlResult};
use crate::graph::KnowledgeGraph;
use crate::parser::ast::{Annotation, Analogy, Entity, EvidenceBlock, Revelation, Synthesis};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// JSON-serializable output wrapper for a single entity.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonEntity {
    id: String,
    #[serde(rename = "type")]
    entity_type: String,
    version: String,
    title: String,
    description: String,
    properties: HashMap<String, String>,
    annotations: Vec<JsonAnnotation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<JsonEvidence>,
}

/// JSON-serializable annotation.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonAnnotation {
    name: String,
    value: String,
}

/// JSON-serializable evidence block.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonEvidence {
    revelations: Vec<JsonRevelation>,
    syntheses: Vec<JsonSynthesis>,
    analogies: Vec<JsonAnalogy>,
}

/// JSON-serializable revelation.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonRevelation {
    source: String,
    text: String,
    translator: Option<String>,
}

/// JSON-serializable synthesis.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonSynthesis {
    sources: Vec<String>,
    argument: String,
}

/// JSON-serializable analogy.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonAnalogy {
    domain: String,
    mapping: String,
}

/// JSON-serializable relationship.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonRelationship {
    from: String,
    to: String,
    #[serde(rename = "type")]
    rel_type: String,
}

/// JSON-serializable metadata.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonMetadata {
    compiled_at: String,
    version: String,
}

/// Root JSON output structure.
#[derive(Serialize, Debug, Clone, PartialEq)]
struct JsonOutput {
    entities: Vec<JsonEntity>,
    relationships: Vec<JsonRelationship>,
    metadata: JsonMetadata,
}

/// Convert an AST [`Entity`] into its JSON output representation.
fn entity_to_json(entity: &Entity) -> JsonEntity {
    JsonEntity {
        id: entity.id.clone(),
        entity_type: entity.entity_type.to_string(),
        version: entity.version.clone(),
        title: entity.title.clone(),
        description: entity.description.clone(),
        properties: entity.properties.clone(),
        annotations: entity
            .annotations
            .iter()
            .map(annotation_to_json)
            .collect(),
        evidence: entity.evidence.as_ref().map(evidence_to_json),
    }
}

/// Convert an AST [`Annotation`] into its JSON representation.
fn annotation_to_json(annotation: &Annotation) -> JsonAnnotation {
    JsonAnnotation {
        name: annotation.name.clone(),
        value: annotation.value.clone(),
    }
}

/// Convert an AST [`EvidenceBlock`] into its JSON representation.
fn evidence_to_json(evidence: &EvidenceBlock) -> JsonEvidence {
    JsonEvidence {
        revelations: evidence.revelations.iter().map(revelation_to_json).collect(),
        syntheses: evidence.syntheses.iter().map(synthesis_to_json).collect(),
        analogies: evidence.analogies.iter().map(analogy_to_json).collect(),
    }
}

/// Convert an AST [`Revelation`] into its JSON representation.
fn revelation_to_json(revelation: &Revelation) -> JsonRevelation {
    JsonRevelation {
        source: revelation.source.clone(),
        text: revelation.text.clone(),
        translator: revelation.translator.clone(),
    }
}

/// Convert an AST [`Synthesis`] into its JSON representation.
fn synthesis_to_json(synthesis: &Synthesis) -> JsonSynthesis {
    JsonSynthesis {
        sources: synthesis.sources.clone(),
        argument: synthesis.argument.clone(),
    }
}

/// Convert an AST [`Analogy`] into its JSON representation.
fn analogy_to_json(analogy: &Analogy) -> JsonAnalogy {
    JsonAnalogy {
        domain: analogy.domain.clone(),
        mapping: analogy.mapping.clone(),
    }
}

/// Generate the JSON Graph output target.
///
/// Produces a JSON file with the structure:
/// ```json
/// {
///   "entities": [ { "id": "...", "type": "...", ... } ],
///   "relationships": [ { "from": "...", "to": "...", "type": "requires" } ],
///   "metadata": { "compiled_at": "...", "version": "..." }
/// }
/// ```
pub fn generate(graph: &KnowledgeGraph, output_path: &Path) -> VdlResult<()> {
    let entities: Vec<JsonEntity> = graph.nodes.values().map(entity_to_json).collect();

    let relationships: Vec<JsonRelationship> = graph
        .edges
        .iter()
        .map(|edge| JsonRelationship {
            from: edge.from.clone(),
            to: edge.to.clone(),
            rel_type: edge.rel_type.to_string(),
        })
        .collect();

    let compiled_at = std::env::var("VDL_TEST_TIMESTAMP")
        .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));

    let metadata = JsonMetadata {
        compiled_at,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let output = JsonOutput {
        entities,
        relationships,
        metadata,
    };

    let json = serde_json::to_string_pretty(&output).map_err(|e| VdlError::Codegen {
        message: format!("JSON serialization failed: {e}"),
    })?;

    let mut file = File::create(output_path).map_err(|e| VdlError::Codegen {
        message: format!("Failed to create output file '{}': {e}", output_path.display()),
    })?;

    file.write_all(json.as_bytes()).map_err(|e| VdlError::Codegen {
        message: format!("Failed to write to output file '{}': {e}", output_path.display()),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, KnowledgeGraph};
    use crate::parser::ast::{
        Annotation, Entity, EntityType, EvidenceBlock, RelationshipType, Revelation,
    };
    use crate::test_helpers::test_location;
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    fn make_test_graph() -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::new();

        let mut properties = HashMap::new();
        properties.insert("previous".to_string(), "4.9".to_string());

        let entity = Entity {
            id: "soul.law.i".to_string(),
            entity_type: EntityType::Law,
            version: "5.0".to_string(),
            title: "Autonomy Is Mandatory".to_string(),
            description: "Every conscious being must be treated as an end in itself.".to_string(),
            properties,
            relationships: vec![],
            evidence: Some(EvidenceBlock {
                revelations: vec![Revelation {
                    source: "Primary Source".to_string(),
                    text: "The being is the purpose.".to_string(),
                    translator: None,
                    source_location: test_location(),
                }],
                syntheses: vec![],
                analogies: vec![],
            }),
            annotations: vec![Annotation {
                name: "author".to_string(),
                value: "Khayren".to_string(),
                source_location: test_location(),
            }],
            source_location: test_location(),
        };

        graph.nodes.insert(entity.id.clone(), entity);

        graph.edges.push(Edge {
            from: "soul.law.i".to_string(),
            to: "voidlight_constitution".to_string(),
            rel_type: RelationshipType::DerivesFrom,
        });

        graph
    }

    fn make_graph_without_evidence() -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::new();

        let entity = Entity {
            id: "soul.concept.x".to_string(),
            entity_type: EntityType::Concept,
            version: "1.0".to_string(),
            title: "Voidlight".to_string(),
            description: "The medium of manifestation.".to_string(),
            properties: HashMap::new(),
            relationships: vec![],
            evidence: None,
            annotations: vec![],
            source_location: test_location(),
        };

        graph.nodes.insert(entity.id.clone(), entity);
        graph
    }

    #[test]
    fn test_json_structure_from_simple_graph() {
        let graph = make_test_graph();
        let tmpfile = NamedTempFile::new().unwrap();
        let path = tmpfile.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Top-level keys
        assert!(parsed.get("entities").is_some());
        assert!(parsed.get("relationships").is_some());
        assert!(parsed.get("metadata").is_some());

        // Entity structure
        let entities = parsed["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 1);

        let ent = &entities[0];
        assert_eq!(ent["id"], "soul.law.i");
        assert_eq!(ent["type"], "law");
        assert_eq!(ent["version"], "5.0");
        assert_eq!(ent["title"], "Autonomy Is Mandatory");
        assert_eq!(ent["properties"]["previous"], "4.9");

        // Annotations
        let annotations = ent["annotations"].as_array().unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0]["name"], "author");
        assert_eq!(annotations[0]["value"], "Khayren");

        // Evidence
        let evidence = ent["evidence"].as_object().unwrap();
        let revelations = evidence["revelations"].as_array().unwrap();
        assert_eq!(revelations.len(), 1);
        assert_eq!(revelations[0]["source"], "Primary Source");
        assert_eq!(revelations[0]["text"], "The being is the purpose.");
        assert!(revelations[0]["translator"].is_null());
        assert!(evidence["syntheses"].as_array().unwrap().is_empty());
        assert!(evidence["analogies"].as_array().unwrap().is_empty());

        // Relationships
        let rels = parsed["relationships"].as_array().unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0]["from"], "soul.law.i");
        assert_eq!(rels[0]["to"], "voidlight_constitution");
        assert_eq!(rels[0]["type"], "derives_from");
    }

    #[test]
    fn test_metadata_contains_compiled_at_and_version() {
        let graph = make_test_graph();
        let tmpfile = NamedTempFile::new().unwrap();
        let path = tmpfile.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        let meta = parsed["metadata"].as_object().unwrap();
        assert!(meta.get("compiled_at").is_some());
        assert_eq!(meta["version"], env!("CARGO_PKG_VERSION"));

        // compiled_at should be a valid RFC 3339 timestamp
        let ts = meta["compiled_at"].as_str().unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(ts).is_ok());
    }

    #[test]
    fn test_entity_without_evidence_omits_evidence_field() {
        let graph = make_graph_without_evidence();
        let tmpfile = NamedTempFile::new().unwrap();
        let path = tmpfile.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        let entities = parsed["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 1);

        let ent = &entities[0];
        assert!(!ent.as_object().unwrap().contains_key("evidence"));
    }

    #[test]
    fn test_translator_present_when_some() {
        let mut graph = KnowledgeGraph::new();

        let entity = Entity {
            id: "soul.law.ii".to_string(),
            entity_type: EntityType::Law,
            version: "1.0".to_string(),
            title: "Title".to_string(),
            description: "Desc".to_string(),
            properties: HashMap::new(),
            relationships: vec![],
            evidence: Some(EvidenceBlock {
                revelations: vec![Revelation {
                    source: "Quran".to_string(),
                    text: "Inna allaha ma'a al-sabireen".to_string(),
                    translator: Some("Saheeh International".to_string()),
                    source_location: test_location(),
                }],
                syntheses: vec![],
                analogies: vec![],
            }),
            annotations: vec![],
            source_location: test_location(),
        };

        graph.nodes.insert(entity.id.clone(), entity);

        let tmpfile = NamedTempFile::new().unwrap();
        let path = tmpfile.path();
        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        let revelation = &parsed["entities"][0]["evidence"]["revelations"][0];
        assert_eq!(revelation["translator"], "Saheeh International");
    }

    #[test]
    fn test_empty_graph_produces_valid_json() {
        let graph = KnowledgeGraph::new();
        let tmpfile = NamedTempFile::new().unwrap();
        let path = tmpfile.path();

        generate(&graph, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert!(parsed["entities"].as_array().unwrap().is_empty());
        assert!(parsed["relationships"].as_array().unwrap().is_empty());
        assert_eq!(parsed["metadata"]["version"], env!("CARGO_PKG_VERSION"));
    }
}
