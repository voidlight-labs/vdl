use crate::error::{VdlError, VdlResult};
use crate::graph::KnowledgeGraph;
use crate::parser::ast::{Entity, EvidenceBlock};
use std::path::Path;

/// Generate Markdown output for Nuxt Content.
///
/// Each entity becomes one `.md` file with YAML frontmatter:
/// ```yaml
/// ---
/// id: soul.law.i
/// type: law
/// version: "5.0"
/// title: "Autonomy Is Mandatory"
/// description: "..."
/// related: ["voidlight_constitution"]
/// ---
/// ```
///
/// The body renders evidence blocks as structured markdown sections.
pub fn generate(graph: &KnowledgeGraph, output_dir: &Path) -> VdlResult<()> {
    std::fs::create_dir_all(output_dir).map_err(|e| VdlError::Io {
        message: format!(
            "Failed to create output directory {}: {}",
            output_dir.display(),
            e
        ),
    })?;

    for (_, entity) in &graph.nodes {
        let content = generate_entity(entity)?;
        let filename = entity_id_to_filename(&entity.id);
        let filepath = output_dir.join(filename);

        std::fs::write(&filepath, content).map_err(|e| VdlError::Io {
            message: format!("Failed to write file {}: {}", filepath.display(), e),
        })?;
    }

    Ok(())
}

/// Convert an entity ID to a markdown filename.
///
/// Dots are replaced with dashes, and `.md` is appended.
fn entity_id_to_filename(id: &str) -> String {
    format!("{}.md", id.replace('.', "-").replace('_', "-"))
}

/// Generate the markdown content for a single entity.
fn generate_entity(entity: &Entity) -> VdlResult<String> {
    let mut output = String::new();

    // YAML frontmatter
    output.push_str("---\n");
    output.push_str(&format!("id: {}\n", entity.id));
    output.push_str(&format!("type: {}\n", entity.entity_type));
    output.push_str(&format!("version: \"{}\"\n", entity.version));
    output.push_str(&format!("title: \"{}\"\n", escape_yaml_string(&entity.title)));
    output.push_str(&format!(
        "description: \"{}\"\n",
        escape_yaml_string(&entity.description)
    ));

    // Related entities from relationships
    if !entity.relationships.is_empty() {
        output.push_str("related:\n");
        for rel in &entity.relationships {
            output.push_str(&format!("  - {}\n", rel.target_id));
        }
    }

    // Annotations mapped to frontmatter
    for annotation in &entity.annotations {
        match annotation.name.as_str() {
            "author" => {
                output.push_str(&format!("author: {}\n", annotation.value));
            }
            "created" => {
                output.push_str(&format!("created: \"{}\"\n", annotation.value));
            }
            "reviewed" => {
                output.push_str(&format!("reviewed: \"{}\"\n", annotation.value));
            }
            "status" => {
                output.push_str(&format!("status: {}\n", annotation.value));
            }
            "pillar" => {
                output.push_str(&format!("pillar: {}\n", annotation.value));
            }
            _ => {
                // Include unknown annotations as generic frontmatter
                output.push_str(&format!("{}: {}\n", annotation.name, annotation.value));
            }
        }
    }

    output.push_str("---\n\n");

    // Body: evidence sections
    if let Some(evidence) = &entity.evidence {
        let has_evidence = !evidence.revelations.is_empty()
            || !evidence.syntheses.is_empty()
            || !evidence.analogies.is_empty();

        if has_evidence {
            output.push_str("## Evidence\n\n");
            render_revelations(&mut output, evidence);
            render_syntheses(&mut output, evidence);
            render_analogies(&mut output, evidence);
        }
    }

    Ok(output)
}

/// Escape a string for use inside YAML double quotes.
fn escape_yaml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Render the Revelation subsection of an Evidence block.
fn render_revelations(output: &mut String, evidence: &EvidenceBlock) {
    if evidence.revelations.is_empty() {
        return;
    }

    output.push_str("### Revelation\n\n");
    for revelation in &evidence.revelations {
        output.push_str(&format!("**Source:** {}\n\n", revelation.source));
        output.push_str("**Text:**\n");
        for line in revelation.text.lines() {
            output.push_str(&format!("> {}\n", line));
        }
        output.push('\n');
    }
}

/// Render the Synthesis subsection of an Evidence block.
fn render_syntheses(output: &mut String, evidence: &EvidenceBlock) {
    if evidence.syntheses.is_empty() {
        return;
    }

    output.push_str("### Synthesis\n\n");
    for synthesis in &evidence.syntheses {
        output.push_str("**Sources:**\n");
        for source in &synthesis.sources {
            output.push_str(&format!("- {}\n", source));
        }
        output.push('\n');
        output.push_str("**Argument:**\n");
        output.push_str(&format!("{}\n\n", synthesis.argument));
    }
}

/// Render the Analogy subsection of an Evidence block.
fn render_analogies(output: &mut String, evidence: &EvidenceBlock) {
    if evidence.analogies.is_empty() {
        return;
    }

    output.push_str("### Analogy\n\n");
    for analogy in &evidence.analogies {
        output.push_str(&format!("**Domain:** {}\n\n", analogy.domain));
        output.push_str("**Mapping:**\n");
        output.push_str(&format!("{}\n\n", analogy.mapping));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceLocation;
    use crate::parser::ast::{
        Analogy, Annotation, Entity, EntityType, EvidenceBlock, Relationship, RelationshipType,
        Revelation, Synthesis,
    };
    use indexmap::IndexMap;
    use std::collections::HashMap;

    fn dummy_loc() -> SourceLocation {
        SourceLocation::new(std::path::PathBuf::from("test.vdl"), 1, 1)
    }

    fn make_entity(id: &str, entity_type: EntityType, title: &str) -> Entity {
        Entity {
            id: id.to_string(),
            entity_type,
            version: "1.0".to_string(),
            title: title.to_string(),
            description: "A test description.".to_string(),
            properties: HashMap::new(),
            relationships: Vec::new(),
            evidence: None,
            annotations: Vec::new(),
            source_location: dummy_loc(),
        }
    }

    #[test]
    fn test_entity_id_to_filename() {
        assert_eq!(entity_id_to_filename("soul.law.i"), "soul-law-i.md");
        assert_eq!(
            entity_id_to_filename("voidlight_constitution"),
            "voidlight-constitution.md"
        );
        assert_eq!(entity_id_to_filename("a.b.c.d"), "a-b-c-d.md");
        assert_eq!(entity_id_to_filename("simple"), "simple.md");
    }

    #[test]
    fn test_generate_entity_frontmatter() {
        let mut entity = make_entity("soul.law.i", EntityType::Law, "Autonomy Is Mandatory");
        entity.version = "5.0".to_string();
        entity.description = "Every node must retain sovereign control...".to_string();
        entity.annotations = vec![
            Annotation {
                name: "author".to_string(),
                value: "Khayren".to_string(),
                source_location: dummy_loc(),
            },
            Annotation {
                name: "created".to_string(),
                value: "2024-03-15".to_string(),
                source_location: dummy_loc(),
            },
            Annotation {
                name: "status".to_string(),
                value: "canonical".to_string(),
                source_location: dummy_loc(),
            },
            Annotation {
                name: "pillar".to_string(),
                value: "soul".to_string(),
                source_location: dummy_loc(),
            },
        ];
        entity.relationships = vec![Relationship {
            rel_type: RelationshipType::DerivesFrom,
            target_id: "voidlight_constitution".to_string(),
            source_location: dummy_loc(),
        }];

        let md = generate_entity(&entity).unwrap();

        assert!(md.contains("id: soul.law.i"));
        assert!(md.contains("type: law"));
        assert!(md.contains("version: \"5.0\""));
        assert!(md.contains("title: \"Autonomy Is Mandatory\""));
        assert!(md.contains("description: \"Every node must retain sovereign control...\""));
        assert!(md.contains("related:"));
        assert!(md.contains("  - voidlight_constitution"));
        assert!(md.contains("author: Khayren"));
        assert!(md.contains("created: \"2024-03-15\""));
        assert!(md.contains("status: canonical"));
        assert!(md.contains("pillar: soul"));
    }

    #[test]
    fn test_generate_entity_without_relationships_or_annotations() {
        let entity = make_entity("core.axiom.1", EntityType::Axiom, "Test Axiom");

        let md = generate_entity(&entity).unwrap();

        assert!(md.contains("id: core.axiom.1"));
        assert!(md.contains("type: axiom"));
        assert!(!md.contains("related:"));
        assert!(!md.contains("author:"));
        assert!(!md.contains("created:"));
    }

    #[test]
    fn test_evidence_revelation_rendering() {
        let mut entity = make_entity("test.1", EntityType::Law, "Test");
        entity.evidence = Some(EvidenceBlock {
            revelations: vec![Revelation {
                source: "Quran 2:30".to_string(),
                text: "And when your Lord said to the angels...".to_string(),
                translator: None,
                source_location: dummy_loc(),
            }],
            syntheses: vec![],
            analogies: vec![],
        });

        let md = generate_entity(&entity).unwrap();

        assert!(md.contains("## Evidence\n"));
        assert!(md.contains("### Revelation\n"));
        assert!(md.contains("**Source:** Quran 2:30"));
        assert!(md.contains("**Text:**"));
        assert!(md.contains("> And when your Lord said to the angels..."));
    }

    #[test]
    fn test_evidence_synthesis_rendering() {
        let mut entity = make_entity("test.2", EntityType::Law, "Test");
        entity.evidence = Some(EvidenceBlock {
            revelations: vec![],
            syntheses: vec![Synthesis {
                sources: vec!["Quran 51:56".to_string(), "Quran 13:28".to_string()],
                argument: "The Quranic command that humans were created...".to_string(),
                source_location: dummy_loc(),
            }],
            analogies: vec![],
        });

        let md = generate_entity(&entity).unwrap();

        assert!(md.contains("## Evidence\n"));
        assert!(md.contains("### Synthesis\n"));
        assert!(md.contains("**Sources:**"));
        assert!(md.contains("- Quran 51:56"));
        assert!(md.contains("- Quran 13:28"));
        assert!(md.contains("**Argument:**"));
        assert!(md.contains("The Quranic command that humans were created..."));
    }

    #[test]
    fn test_evidence_analogy_rendering() {
        let mut entity = make_entity("test.3", EntityType::Concept, "Test");
        entity.evidence = Some(EvidenceBlock {
            revelations: vec![],
            syntheses: vec![],
            analogies: vec![Analogy {
                domain: "Architecture".to_string(),
                mapping: "A building's foundation must be poured...".to_string(),
                source_location: dummy_loc(),
            }],
        });

        let md = generate_entity(&entity).unwrap();

        assert!(md.contains("## Evidence\n"));
        assert!(md.contains("### Analogy\n"));
        assert!(md.contains("**Domain:** Architecture"));
        assert!(md.contains("**Mapping:**"));
        assert!(md.contains("A building's foundation must be poured..."));
    }

    #[test]
    fn test_evidence_all_sections_together() {
        let mut entity = make_entity("test.4", EntityType::Principle, "Test");
        entity.evidence = Some(EvidenceBlock {
            revelations: vec![Revelation {
                source: "Source A".to_string(),
                text: "Text A".to_string(),
                translator: None,
                source_location: dummy_loc(),
            }],
            syntheses: vec![Synthesis {
                sources: vec!["Source B".to_string()],
                argument: "Argument B".to_string(),
                source_location: dummy_loc(),
            }],
            analogies: vec![Analogy {
                domain: "Domain C".to_string(),
                mapping: "Mapping C".to_string(),
                source_location: dummy_loc(),
            }],
        });

        let md = generate_entity(&entity).unwrap();

        assert!(md.contains("## Evidence\n"));
        assert!(md.contains("### Revelation\n"));
        assert!(md.contains("### Synthesis\n"));
        assert!(md.contains("### Analogy\n"));
    }

    #[test]
    fn test_no_evidence_section_when_empty() {
        let entity = make_entity("test.5", EntityType::Law, "Test");

        let md = generate_entity(&entity).unwrap();

        assert!(!md.contains("## Evidence"));
    }

    #[test]
    fn test_no_evidence_section_when_none() {
        let mut entity = make_entity("test.6", EntityType::Law, "Test");
        entity.evidence = Some(EvidenceBlock {
            revelations: vec![],
            syntheses: vec![],
            analogies: vec![],
        });

        let md = generate_entity(&entity).unwrap();

        assert!(!md.contains("## Evidence"));
    }

    #[test]
    fn test_yaml_escaping() {
        let mut entity = make_entity("test.7", EntityType::Law, "Title with \"quotes\"");
        entity.description = "Desc with \\ backslash".to_string();

        let md = generate_entity(&entity).unwrap();

        assert!(md.contains(r#"title: "Title with \"quotes\"""#));
        assert!(md.contains(r#"description: "Desc with \\ backslash""#));
    }

    #[test]
    fn test_generate_writes_files() {
        let mut graph = KnowledgeGraph::new();
        let entity1 = make_entity("soul.law.i", EntityType::Law, "Law One");
        let entity2 = make_entity("voidlight_constitution", EntityType::Framework, "Constitution");

        graph.nodes.insert(entity1.id.clone(), entity1);
        graph.nodes.insert(entity2.id.clone(), entity2);

        let tmpdir = tempfile::tempdir().unwrap();
        generate(&graph, tmpdir.path()).unwrap();

        let law_path = tmpdir.path().join("soul-law-i.md");
        let const_path = tmpdir.path().join("voidlight-constitution.md");

        assert!(law_path.exists());
        assert!(const_path.exists());

        let law_content = std::fs::read_to_string(law_path).unwrap();
        assert!(law_content.contains("id: soul.law.i"));
        assert!(law_content.contains("type: law"));

        let const_content = std::fs::read_to_string(const_path).unwrap();
        assert!(const_content.contains("id: voidlight_constitution"));
        assert!(const_content.contains("type: framework"));
    }

    #[test]
    fn test_generate_creates_output_directory() {
        let graph = KnowledgeGraph::new();
        let tmpdir = tempfile::tempdir().unwrap();
        let nested = tmpdir.path().join("nested").join("output");

        generate(&graph, &nested).unwrap();

        assert!(nested.exists());
    }

    #[test]
    fn test_generate_empty_graph() {
        let graph = KnowledgeGraph::new();
        let tmpdir = tempfile::tempdir().unwrap();

        // Should succeed without writing any files
        generate(&graph, tmpdir.path()).unwrap();

        let entries: Vec<_> = std::fs::read_dir(tmpdir.path()).unwrap().collect();
        assert!(entries.is_empty());
    }
}
