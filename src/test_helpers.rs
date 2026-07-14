//! Shared helpers for unit and integration tests.

use crate::error::SourceLocation;
use crate::parser::ast::{
    Entity, EntityType, EvidenceBlock, Relationship, RelationshipType, Revelation,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// Return a default test source location.
pub fn test_location() -> SourceLocation {
    SourceLocation::new(PathBuf::from("test.vdl"), 1, 1)
}

/// Build a minimal test entity with the given id, type, and version.
pub fn test_entity(id: &str, entity_type: EntityType, version: &str) -> Entity {
    Entity {
        id: id.to_string(),
        entity_type,
        version: version.to_string(),
        title: id.to_string(),
        description: format!("Description of {}", id),
        properties: HashMap::new(),
        relationships: Vec::new(),
        evidence: None,
        annotations: Vec::new(),
        source_location: test_location(),
    }
}

/// Build a test relationship pointing to the given target.
pub fn test_relationship(rel_type: RelationshipType, target_id: &str) -> Relationship {
    Relationship {
        rel_type,
        target_id: target_id.to_string(),
        source_location: test_location(),
    }
}

/// Build a test evidence block containing a single revelation.
pub fn test_evidence_block() -> EvidenceBlock {
    EvidenceBlock {
        revelations: vec![Revelation {
            source: "Source".to_string(),
            text: "Text".to_string(),
            translator: None,
            source_location: test_location(),
        }],
        syntheses: Vec::new(),
        analogies: Vec::new(),
    }
}
