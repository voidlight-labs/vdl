pub mod error;
pub mod rules;

use crate::error::{VdlError, VdlResult};
use crate::parser::ast::Module;

/// Validate a parsed VDL module against all compiler rules.
///
/// Checks performed (in order):
/// 1. Type constraints (axiom empty requires, framework has based_on, etc.)
/// 2. Reference integrity (all relationship targets resolve)
/// 3. Evidence presence (non-artifact entities must have evidence)
/// 4. Evidence completeness (required sub-fields present)
/// 5. DAG validation (no cycles in requires / derives_from)
/// 6. Version format (must match `^\d+\.\d+`$)
///
/// # Errors
///
/// Returns [`VdlError::Validation`] for any rule violation.
/// Where possible, all violations are collected and reported together.
pub fn validate(module: &Module) -> VdlResult<()> {
    let mut all_errors: Vec<VdlError> = Vec::new();

    all_errors.extend(rules::check_type_constraints(module).into_iter().map(VdlError::from));
    all_errors.extend(rules::check_reference_integrity(module).into_iter().map(VdlError::from));
    all_errors.extend(rules::check_evidence_presence(module).into_iter().map(VdlError::from));
    all_errors.extend(rules::check_evidence_completeness(module).into_iter().map(VdlError::from));
    all_errors.extend(rules::check_dag(module).into_iter().map(VdlError::from));
    all_errors.extend(rules::check_version_format(module).into_iter().map(VdlError::from));

    if let Some(first_error) = all_errors.into_iter().next() {
        Err(first_error)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{
        Entity, EntityType, EvidenceBlock, Module, Relationship, RelationshipType,
        Revelation, Synthesis,
    };
    use crate::error::SourceLocation;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn loc() -> SourceLocation {
        SourceLocation::new(PathBuf::from("test.vdl"), 1, 1)
    }

    fn entity(
        id: &str,
        entity_type: EntityType,
        version: &str,
        relationships: Vec<Relationship>,
        evidence: Option<EvidenceBlock>,
    ) -> Entity {
        Entity {
            id: id.to_string(),
            entity_type,
            version: version.to_string(),
            title: id.to_string(),
            description: format!("Description of {}", id),
            properties: HashMap::new(),
            relationships,
            evidence,
            annotations: Vec::new(),
            source_location: loc(),
        }
    }

    fn rel(rel_type: RelationshipType, target_id: &str) -> Relationship {
        Relationship {
            rel_type,
            target_id: target_id.to_string(),
            source_location: loc(),
        }
    }

    fn valid_evidence() -> EvidenceBlock {
        EvidenceBlock {
            revelations: vec![Revelation {
                source: "Source".to_string(),
                text: "Text".to_string(),
                translator: None,
                source_location: loc(),
            }],
            syntheses: Vec::new(),
            analogies: Vec::new(),
        }
    }

    #[test]
    fn valid_module_passes_validation() {
        let module = Module {
            entities: vec![
                entity("axiom1", EntityType::Axiom, "1.0", vec![], Some(valid_evidence())),
                entity(
                    "framework1",
                    EntityType::Framework,
                    "1.0",
                    vec![rel(RelationshipType::BasedOn, "axiom1")],
                    Some(valid_evidence()),
                ),
                entity(
                    "law1",
                    EntityType::Law,
                    "1.0",
                    vec![rel(RelationshipType::DerivesFrom, "axiom1")],
                    Some(valid_evidence()),
                ),
                entity(
                    "principle1",
                    EntityType::Principle,
                    "1.0",
                    vec![rel(RelationshipType::DerivesFrom, "law1")],
                    Some(valid_evidence()),
                ),
                entity(
                    "artifact1",
                    EntityType::Artifact,
                    "1.0",
                    vec![rel(RelationshipType::References, "law1")],
                    None,
                ),
            ],
        };
        assert!(validate(&module).is_ok());
    }

    #[test]
    fn axiom_with_requires_fails() {
        let module = Module {
            entities: vec![entity(
                "axiom1",
                EntityType::Axiom,
                "1.0",
                vec![rel(RelationshipType::Requires, "other")],
                Some(valid_evidence()),
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn framework_without_based_on_fails() {
        let module = Module {
            entities: vec![entity(
                "fw1",
                EntityType::Framework,
                "1.0",
                vec![],
                Some(valid_evidence()),
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn law_without_derives_from_fails() {
        let module = Module {
            entities: vec![entity(
                "law1",
                EntityType::Law,
                "1.0",
                vec![],
                Some(valid_evidence()),
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn missing_reference_fails() {
        let module = Module {
            entities: vec![entity(
                "e1",
                EntityType::Concept,
                "1.0",
                vec![rel(RelationshipType::References, "missing")],
                Some(valid_evidence()),
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn non_artifact_without_evidence_fails() {
        let module = Module {
            entities: vec![entity(
                "concept1",
                EntityType::Concept,
                "1.0",
                vec![],
                None,
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn synthesis_with_one_source_fails() {
        let module = Module {
            entities: vec![entity(
                "e1",
                EntityType::Concept,
                "1.0",
                vec![],
                Some(EvidenceBlock {
                    revelations: Vec::new(),
                    syntheses: vec![Synthesis {
                        sources: vec!["one".to_string()],
                        argument: "arg".to_string(),
                        source_location: loc(),
                    }],
                    analogies: Vec::new(),
                }),
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn circular_requires_chain_fails() {
        let module = Module {
            entities: vec![
                entity(
                    "a",
                    EntityType::Concept,
                    "1.0",
                    vec![rel(RelationshipType::Requires, "b")],
                    Some(valid_evidence()),
                ),
                entity(
                    "b",
                    EntityType::Concept,
                    "1.0",
                    vec![rel(RelationshipType::Requires, "c")],
                    Some(valid_evidence()),
                ),
                entity(
                    "c",
                    EntityType::Concept,
                    "1.0",
                    vec![rel(RelationshipType::Requires, "a")],
                    Some(valid_evidence()),
                ),
            ],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_version_fails() {
        let module = Module {
            entities: vec![entity(
                "e1",
                EntityType::Concept,
                "abc",
                vec![],
                Some(valid_evidence()),
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
    }
}
