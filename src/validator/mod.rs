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

    if all_errors.is_empty() {
        Ok(())
    } else if all_errors.len() == 1 {
        Err(all_errors.into_iter().next().unwrap())
    } else {
        let count = all_errors.len();
        let messages = all_errors
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        Err(VdlError::ValidationErrors { count, messages })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{Entity, EntityType, EvidenceBlock, Module, Relationship, RelationshipType, Synthesis};
    use crate::test_helpers::{test_entity, test_evidence_block, test_location, test_relationship};

    fn entity(
        id: &str,
        entity_type: EntityType,
        version: &str,
        relationships: Vec<Relationship>,
        evidence: Option<EvidenceBlock>,
    ) -> Entity {
        let mut e = test_entity(id, entity_type, version);
        e.relationships = relationships;
        e.evidence = evidence;
        e
    }

    fn rel(rel_type: RelationshipType, target_id: &str) -> Relationship {
        test_relationship(rel_type, target_id)
    }

    fn valid_evidence() -> EvidenceBlock {
        test_evidence_block()
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
                        source_location: test_location(),
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

    #[test]
    fn multiple_validation_errors_are_collected() {
        // Concept without evidence AND with an invalid version triggers two
        // independent validation errors.
        let module = Module {
            entities: vec![entity(
                "e1",
                EntityType::Concept,
                "not-a-version",
                vec![],
                None,
            )],
        };
        let result = validate(&module);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VdlError::ValidationErrors { .. }),
            "expected multiple validation errors to be aggregated"
        );
    }
}
