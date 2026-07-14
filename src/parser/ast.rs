use crate::error::SourceLocation;
use std::collections::HashMap;

/// A VDL module containing all parsed entities from one or more source files.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Module {
    pub entities: Vec<Entity>,
}

/// A single VDL entity declaration.
///
/// Represents any discrete unit of knowledge in the Voidlight system,
/// from axioms to creative artifacts.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: String,
    pub entity_type: EntityType,
    pub version: String,
    pub title: String,
    pub description: String,
    /// Custom properties beyond the standard fields. In v0.1, values are strings only.
    pub properties: HashMap<String, String>,
    pub relationships: Vec<Relationship>,
    pub evidence: Option<EvidenceBlock>,
    pub annotations: Vec<Annotation>,
    pub source_location: SourceLocation,
}

/// Classification of a VDL entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Axiom,
    Framework,
    Law,
    Principle,
    Concept,
    Artifact,
    /// Complex types — parseable and validate basic rules in v0.1.
    Pillar,
    Document,
    Project,
    Release,
    Persona,
    Collection,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::Axiom => write!(f, "axiom"),
            EntityType::Framework => write!(f, "framework"),
            EntityType::Law => write!(f, "law"),
            EntityType::Principle => write!(f, "principle"),
            EntityType::Concept => write!(f, "concept"),
            EntityType::Artifact => write!(f, "artifact"),
            EntityType::Pillar => write!(f, "pillar"),
            EntityType::Document => write!(f, "document"),
            EntityType::Project => write!(f, "project"),
            EntityType::Release => write!(f, "release"),
            EntityType::Persona => write!(f, "persona"),
            EntityType::Collection => write!(f, "collection"),
        }
    }
}

/// A typed, directional relationship between two entities.
#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    pub rel_type: RelationshipType,
    pub target_id: String,
    pub source_location: SourceLocation,
}

/// Types of semantic relationships in VDL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationshipType {
    Requires,
    Enables,
    References,
    BasedOn,
    DerivesFrom,
    Implements,
    InspiredBy,
    EvolvedFrom,
    Contradicts,
}

impl std::fmt::Display for RelationshipType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationshipType::Requires => write!(f, "requires"),
            RelationshipType::Enables => write!(f, "enables"),
            RelationshipType::References => write!(f, "references"),
            RelationshipType::BasedOn => write!(f, "based_on"),
            RelationshipType::DerivesFrom => write!(f, "derives_from"),
            RelationshipType::Implements => write!(f, "implements"),
            RelationshipType::InspiredBy => write!(f, "inspired_by"),
            RelationshipType::EvolvedFrom => write!(f, "evolved_from"),
            RelationshipType::Contradicts => write!(f, "contradicts"),
        }
    }
}

/// A block of evidence supporting an entity's claims.
///
/// Every non-artifact entity must have at least one evidence entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvidenceBlock {
    pub revelations: Vec<Revelation>,
    pub syntheses: Vec<Synthesis>,
    pub analogies: Vec<Analogy>,
}

/// Direct textual evidence from scripture or primary sources.
#[derive(Debug, Clone, PartialEq)]
pub struct Revelation {
    pub source: String,
    pub text: String,
    pub translator: Option<String>,
    pub source_location: SourceLocation,
}

/// Scholarly interpretation combining multiple sources.
#[derive(Debug, Clone, PartialEq)]
pub struct Synthesis {
    pub sources: Vec<String>,
    pub argument: String,
    pub source_location: SourceLocation,
}

/// Cross-domain metaphor to bridge unfamiliar concepts.
#[derive(Debug, Clone, PartialEq)]
pub struct Analogy {
    pub domain: String,
    pub mapping: String,
    pub source_location: SourceLocation,
}

/// A metadata annotation attached to an entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub name: String,
    pub value: String,
    pub source_location: SourceLocation,
}
