use crate::parser::ast::{EntityType, Module, RelationshipType};
use crate::validator::error::ValidationError;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static VERSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.\d+$").expect("version regex should be valid"));

/// Individual validation rule implementations.
///
/// Each function checks one aspect of the module and returns a list of errors.
/// The [`super::validate`] function orchestrates these in the correct order.

/// 1. Type constraint validation.
pub fn check_type_constraints(module: &Module) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Build a map of id -> entity_type for artifact validation
    let id_to_type: HashMap<&str, EntityType> = module
        .entities
        .iter()
        .map(|e| (e.id.as_str(), e.entity_type))
        .collect();

    for entity in &module.entities {
        match entity.entity_type {
            EntityType::Axiom => {
                let requires_count = entity
                    .relationships
                    .iter()
                    .filter(|r| r.rel_type == RelationshipType::Requires)
                    .count();
                if requires_count > 0 {
                    errors.push(ValidationError::TypeConstraint {
                        location: entity.source_location.clone(),
                        message: format!(
                            "axiom '{}' must have no requires relationships, found {}",
                            entity.id, requires_count
                        ),
                    });
                }
            }
            EntityType::Framework => {
                let has_based_on = entity
                    .relationships
                    .iter()
                    .any(|r| r.rel_type == RelationshipType::BasedOn);
                if !has_based_on {
                    errors.push(ValidationError::TypeConstraint {
                        location: entity.source_location.clone(),
                        message: format!(
                            "framework '{}' must have at least one based_on relationship",
                            entity.id
                        ),
                    });
                }
            }
            EntityType::Law => {
                let has_derives_from = entity
                    .relationships
                    .iter()
                    .any(|r| r.rel_type == RelationshipType::DerivesFrom);
                if !has_derives_from {
                    errors.push(ValidationError::TypeConstraint {
                        location: entity.source_location.clone(),
                        message: format!(
                            "law '{}' must have at least one derives_from relationship",
                            entity.id
                        ),
                    });
                }
            }
            EntityType::Principle => {
                let has_derives_from = entity
                    .relationships
                    .iter()
                    .any(|r| r.rel_type == RelationshipType::DerivesFrom);
                if !has_derives_from {
                    errors.push(ValidationError::TypeConstraint {
                        location: entity.source_location.clone(),
                        message: format!(
                            "principle '{}' must have at least one derives_from relationship",
                            entity.id
                        ),
                    });
                }
            }
            EntityType::Artifact => {
                let refs_law_or_principle = entity.relationships.iter().any(|r| {
                    id_to_type
                        .get(r.target_id.as_str())
                        .map(|&t| t == EntityType::Law || t == EntityType::Principle)
                        .unwrap_or(false)
                });
                if !refs_law_or_principle {
                    errors.push(ValidationError::TypeConstraint {
                        location: entity.source_location.clone(),
                        message: format!(
                            "artifact '{}' must reference at least one law or principle",
                            entity.id
                        ),
                    });
                }
            }
            _ => {} // No constraints for other types in v0.1
        }
    }

    errors
}

/// 2. Reference integrity validation.
pub fn check_reference_integrity(module: &Module) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let ids: HashSet<&str> = module.entities.iter().map(|e| e.id.as_str()).collect();

    for entity in &module.entities {
        for rel in &entity.relationships {
            if !ids.contains(rel.target_id.as_str()) {
                errors.push(ValidationError::ReferenceIntegrity {
                    location: rel.source_location.clone(),
                    message: format!(
                        "entity '{}' references unresolved target '{}'",
                        entity.id, rel.target_id
                    ),
                });
            }
        }
    }

    errors
}

/// 3. Evidence presence validation.
pub fn check_evidence_presence(module: &Module) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for entity in &module.entities {
        if entity.entity_type == EntityType::Artifact {
            continue;
        }

        let has_evidence = match &entity.evidence {
            Some(ev) => {
                !ev.revelations.is_empty()
                    || !ev.syntheses.is_empty()
                    || !ev.analogies.is_empty()
            }
            None => false,
        };

        if !has_evidence {
            errors.push(ValidationError::MissingEvidence {
                location: entity.source_location.clone(),
                message: format!(
                    "entity '{}' of type '{}' must have evidence with at least one entry",
                    entity.id, entity.entity_type
                ),
            });
        }
    }

    errors
}

/// 4. Evidence completeness validation.
pub fn check_evidence_completeness(module: &Module) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for entity in &module.entities {
        let evidence = match &entity.evidence {
            Some(e) => e,
            None => continue,
        };

        for revelation in &evidence.revelations {
            if revelation.source.trim().is_empty() {
                errors.push(ValidationError::IncompleteEvidence {
                    location: revelation.source_location.clone(),
                    message: format!(
                        "revelation in entity '{}' has empty source",
                        entity.id
                    ),
                });
            }
            if revelation.text.trim().is_empty() {
                errors.push(ValidationError::IncompleteEvidence {
                    location: revelation.source_location.clone(),
                    message: format!(
                        "revelation in entity '{}' has empty text",
                        entity.id
                    ),
                });
            }
        }

        for synthesis in &evidence.syntheses {
            if synthesis.sources.len() < 2 {
                errors.push(ValidationError::IncompleteEvidence {
                    location: synthesis.source_location.clone(),
                    message: format!(
                        "synthesis in entity '{}' must have at least 2 sources, found {}",
                        entity.id,
                        synthesis.sources.len()
                    ),
                });
            }
            if synthesis.argument.trim().is_empty() {
                errors.push(ValidationError::IncompleteEvidence {
                    location: synthesis.source_location.clone(),
                    message: format!(
                        "synthesis in entity '{}' has empty argument",
                        entity.id
                    ),
                });
            }
        }

        for analogy in &evidence.analogies {
            if analogy.domain.trim().is_empty() {
                errors.push(ValidationError::IncompleteEvidence {
                    location: analogy.source_location.clone(),
                    message: format!(
                        "analogy in entity '{}' has empty domain",
                        entity.id
                    ),
                });
            }
            if analogy.mapping.trim().is_empty() {
                errors.push(ValidationError::IncompleteEvidence {
                    location: analogy.source_location.clone(),
                    message: format!(
                        "analogy in entity '{}' has empty mapping",
                        entity.id
                    ),
                });
            }
        }
    }

    errors
}

/// 5. DAG validation (no cycles in requires / derives_from).
pub fn check_dag(module: &Module) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Build adjacency list from requires and derives_from edges only
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut all_ids = HashSet::new();

    for entity in &module.entities {
        all_ids.insert(entity.id.as_str());
        for rel in &entity.relationships {
            if rel.rel_type == RelationshipType::Requires
                || rel.rel_type == RelationshipType::DerivesFrom
            {
                adj.entry(entity.id.as_str())
                    .or_default()
                    .push(rel.target_id.as_str());
                all_ids.insert(rel.target_id.as_str());
            }
        }
    }

    // DFS with color marking: 0 = White, 1 = Gray, 2 = Black
    let mut color: HashMap<&str, u8> = HashMap::new();
    for &id in &all_ids {
        color.insert(id, 0);
    }

    for &start in &all_ids {
        if color[start] != 0 {
            continue;
        }

        let mut stack: Vec<(&str, usize)> = Vec::new();
        let mut path: Vec<&str> = Vec::new();

        stack.push((start, 0));
        path.push(start);
        *color.get_mut(start).unwrap() = 1;

        while let Some((node, next_idx)) = stack.pop() {
            let neighbors = adj.get(node).map(|v| v.as_slice()).unwrap_or(&[]);

            if next_idx < neighbors.len() {
                // Push current node back with incremented index
                stack.push((node, next_idx + 1));
                let neighbor = neighbors[next_idx];

                match color.get(neighbor).copied().unwrap_or(0) {
                    0 => {
                        // Unvisited - recurse
                        *color.get_mut(neighbor).unwrap() = 1;
                        stack.push((neighbor, 0));
                        path.push(neighbor);
                    }
                    1 => {
                        // Gray - cycle detected!
                        if let Some(cycle_start) = path.iter().position(|&n| n == neighbor) {
                            let mut cycle: Vec<&str> = path[cycle_start..].to_vec();
                            cycle.push(neighbor);
                            // Normalize: rotate so the lexicographically smallest node is first
                            let min_idx = cycle.iter().enumerate().min_by_key(|(_, &n)| n).map(|(i, _)| i).unwrap_or(0);
                            let normalized: Vec<&str> = cycle[min_idx..cycle.len()-1].iter().chain(&cycle[0..min_idx]).copied().collect();
                            let normalized_str = normalized.join(" → ");
                            let full_cycle = format!("{} → {}", normalized_str, normalized[0]);
                            errors.push(ValidationError::Cycle {
                                message: full_cycle,
                            });
                        }
                    }
                    _ => {} // Black - already processed
                }
            } else {
                // All neighbors processed - mark black and pop from path
                *color.get_mut(node).unwrap() = 2;
                path.pop();
            }
        }
    }

    errors
}

/// 6. Version format validation.
pub fn check_version_format(module: &Module) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for entity in &module.entities {
        if !VERSION_REGEX.is_match(&entity.version) {
            errors.push(ValidationError::InvalidVersion {
                location: entity.source_location.clone(),
                message: format!(
                    "entity '{}' has invalid version '{}', expected format 'MAJOR.MINOR' (e.g., '0.1')",
                    entity.id, entity.version
                ),
            });
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{Entity, EntityType, EvidenceBlock, Module, Relationship, RelationshipType, Synthesis};
    use crate::test_helpers::{test_entity, test_evidence_block, test_location, test_relationship};

    fn entity_with(
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
    fn valid_module_passes() {
        let module = Module {
            entities: vec![
                entity_with(
                    "axiom1",
                    EntityType::Axiom,
                    "1.0",
                    vec![],
                    Some(valid_evidence()),
                ),
                entity_with(
                    "framework1",
                    EntityType::Framework,
                    "1.0",
                    vec![rel(RelationshipType::BasedOn, "axiom1")],
                    Some(valid_evidence()),
                ),
                entity_with(
                    "law1",
                    EntityType::Law,
                    "1.0",
                    vec![rel(RelationshipType::DerivesFrom, "axiom1")],
                    Some(valid_evidence()),
                ),
                entity_with(
                    "principle1",
                    EntityType::Principle,
                    "1.0",
                    vec![rel(RelationshipType::DerivesFrom, "law1")],
                    Some(valid_evidence()),
                ),
                entity_with(
                    "artifact1",
                    EntityType::Artifact,
                    "1.0",
                    vec![rel(RelationshipType::References, "law1")],
                    None,
                ),
            ],
        };

        assert!(check_type_constraints(&module).is_empty());
        assert!(check_reference_integrity(&module).is_empty());
        assert!(check_evidence_presence(&module).is_empty());
        assert!(check_evidence_completeness(&module).is_empty());
        assert!(check_dag(&module).is_empty());
        assert!(check_version_format(&module).is_empty());
    }

    #[test]
    fn axiom_with_requires_fails() {
        let module = Module {
            entities: vec![entity_with(
                "axiom1",
                EntityType::Axiom,
                "1.0",
                vec![rel(RelationshipType::Requires, "something")],
                Some(valid_evidence()),
            )],
        };
        let errors = check_type_constraints(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::TypeConstraint { .. }));
    }

    #[test]
    fn framework_without_based_on_fails() {
        let module = Module {
            entities: vec![entity_with(
                "fw1",
                EntityType::Framework,
                "1.0",
                vec![],
                Some(valid_evidence()),
            )],
        };
        let errors = check_type_constraints(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::TypeConstraint { .. }));
    }

    #[test]
    fn law_without_derives_from_fails() {
        let module = Module {
            entities: vec![entity_with(
                "law1",
                EntityType::Law,
                "1.0",
                vec![],
                Some(valid_evidence()),
            )],
        };
        let errors = check_type_constraints(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::TypeConstraint { .. }));
    }

    #[test]
    fn missing_reference_fails() {
        let module = Module {
            entities: vec![entity_with(
                "e1",
                EntityType::Concept,
                "1.0",
                vec![rel(RelationshipType::References, "missing")],
                Some(valid_evidence()),
            )],
        };
        let errors = check_reference_integrity(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::ReferenceIntegrity { .. }));
    }

    #[test]
    fn non_artifact_without_evidence_fails() {
        let module = Module {
            entities: vec![entity_with(
                "concept1",
                EntityType::Concept,
                "1.0",
                vec![],
                None,
            )],
        };
        let errors = check_evidence_presence(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::MissingEvidence { .. }));
    }

    #[test]
    fn artifact_without_evidence_passes() {
        let module = Module {
            entities: vec![entity_with(
                "art1",
                EntityType::Artifact,
                "1.0",
                vec![rel(RelationshipType::References, "law1")],
                None,
            )],
        };
        let errors = check_evidence_presence(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn synthesis_with_one_source_fails() {
        let module = Module {
            entities: vec![entity_with(
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
        let errors = check_evidence_completeness(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::IncompleteEvidence { .. }));
    }

    #[test]
    fn circular_requires_fails() {
        let module = Module {
            entities: vec![
                entity_with(
                    "a",
                    EntityType::Concept,
                    "1.0",
                    vec![rel(RelationshipType::Requires, "b")],
                    Some(valid_evidence()),
                ),
                entity_with(
                    "b",
                    EntityType::Concept,
                    "1.0",
                    vec![rel(RelationshipType::Requires, "c")],
                    Some(valid_evidence()),
                ),
                entity_with(
                    "c",
                    EntityType::Concept,
                    "1.0",
                    vec![rel(RelationshipType::Requires, "a")],
                    Some(valid_evidence()),
                ),
            ],
        };
        let errors = check_dag(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::Cycle { .. }));
        if let ValidationError::Cycle { message } = &errors[0] {
            assert!(message.contains("a → b → c → a"));
        }
    }

    #[test]
    fn invalid_version_fails() {
        let module = Module {
            entities: vec![entity_with(
                "e1",
                EntityType::Concept,
                "abc",
                vec![],
                Some(valid_evidence()),
            )],
        };
        let errors = check_version_format(&module);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::InvalidVersion { .. }));
    }

    #[test]
    fn version_with_extra_dots_fails() {
        let module = Module {
            entities: vec![entity_with(
                "e1",
                EntityType::Concept,
                "1.2.3",
                vec![],
                Some(valid_evidence()),
            )],
        };
        let errors = check_version_format(&module);
        assert_eq!(errors.len(), 1);
    }
}
