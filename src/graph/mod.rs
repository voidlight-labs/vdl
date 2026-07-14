pub mod node;
pub mod query;

pub use node::{Edge, KnowledgeGraph};
pub use query::{GraphError, GraphQuery};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceLocation;
    use crate::parser::ast::{
        Annotation, Entity, EntityType, EvidenceBlock, Module, Relationship, RelationshipType,
    };
    use std::collections::HashMap;

    fn dummy_loc() -> SourceLocation {
        SourceLocation::new("test.vdl", 1, 1)
    }

    fn make_entity(
        id: &str,
        entity_type: EntityType,
        rels: Vec<(&str, RelationshipType)>,
    ) -> Entity {
        Entity {
            id: id.to_string(),
            entity_type,
            version: "1.0.0".to_string(),
            title: format!("Test {}", id),
            description: format!("Description of {}", id),
            properties: HashMap::new(),
            relationships: rels
                .into_iter()
                .map(|(target, rel_type)| Relationship {
                    rel_type,
                    target_id: target.to_string(),
                    source_location: dummy_loc(),
                })
                .collect(),
            evidence: None,
            annotations: vec![],
            source_location: dummy_loc(),
        }
    }

    #[test]
    fn test_build_graph_from_module() {
        let a = make_entity("A", EntityType::Concept, vec![("B", RelationshipType::Requires)]);
        let b = make_entity("B", EntityType::Concept, vec![]);
        let module = Module {
            entities: vec![a.clone(), b.clone()],
        };
        let graph = KnowledgeGraph::from_module(&module).unwrap();

        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.nodes.contains_key("A"));
        assert!(graph.nodes.contains_key("B"));
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].from, "A");
        assert_eq!(graph.edges[0].to, "B");
        assert_eq!(graph.edges[0].rel_type, RelationshipType::Requires);
        assert!(graph.adjacency.contains_key("A"));
        assert_eq!(graph.adjacency["A"].len(), 1);
        assert_eq!(graph.adjacency["A"][0], ("B".to_string(), RelationshipType::Requires));
    }

    #[test]
    fn test_ancestors() {
        // Chain: A requires B, B derives_from C
        let a = make_entity("A", EntityType::Concept, vec![("B", RelationshipType::Requires)]);
        let b = make_entity("B", EntityType::Concept, vec![("C", RelationshipType::DerivesFrom)]);
        let c = make_entity("C", EntityType::Axiom, vec![]);
        let module = Module {
            entities: vec![a, b, c],
        };
        let graph = KnowledgeGraph::from_module(&module).unwrap();
        let query = GraphQuery::new(&graph);

        let ancestors = query.ancestors("A").unwrap();
        let ancestor_ids: Vec<_> = ancestors.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ancestor_ids.len(), 2);
        assert!(ancestor_ids.contains(&"B"));
        assert!(ancestor_ids.contains(&"C"));
    }

    #[test]
    fn test_descendants() {
        // Chain: A requires B, B derives_from C
        // From C's perspective, B and A are descendants.
        let a = make_entity("A", EntityType::Concept, vec![("B", RelationshipType::Requires)]);
        let b = make_entity("B", EntityType::Concept, vec![("C", RelationshipType::DerivesFrom)]);
        let c = make_entity("C", EntityType::Axiom, vec![]);
        let module = Module {
            entities: vec![a, b, c],
        };
        let graph = KnowledgeGraph::from_module(&module).unwrap();
        let query = GraphQuery::new(&graph);

        let descendants = query.descendants("C").unwrap();
        let descendant_ids: Vec<_> = descendants.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(descendant_ids.len(), 2);
        assert!(descendant_ids.contains(&"B"));
        assert!(descendant_ids.contains(&"A"));
    }

    #[test]
    fn test_related_with_specific_rel_type() {
        let a = make_entity(
            "A",
            EntityType::Concept,
            vec![
                ("B", RelationshipType::Requires),
                ("C", RelationshipType::References),
                ("D", RelationshipType::Requires),
            ],
        );
        let b = make_entity("B", EntityType::Concept, vec![]);
        let c = make_entity("C", EntityType::Concept, vec![]);
        let d = make_entity("D", EntityType::Concept, vec![]);
        let module = Module {
            entities: vec![a, b, c, d],
        };
        let graph = KnowledgeGraph::from_module(&module).unwrap();
        let query = GraphQuery::new(&graph);

        let required = query.related("A", RelationshipType::Requires).unwrap();
        let required_ids: Vec<_> = required.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(required_ids.len(), 2);
        assert!(required_ids.contains(&"B"));
        assert!(required_ids.contains(&"D"));

        let referenced = query.related("A", RelationshipType::References).unwrap();
        assert_eq!(referenced.len(), 1);
        assert_eq!(referenced[0].id, "C");

        let none = query.related("A", RelationshipType::Contradicts).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn test_entity_not_found_error() {
        let module = Module { entities: vec![] };
        let graph = KnowledgeGraph::from_module(&module).unwrap();
        let query = GraphQuery::new(&graph);

        assert!(
            matches!(query.ancestors("X"), Err(GraphError::EntityNotFound(ref id)) if id == "X")
        );
        assert!(
            matches!(query.descendants("X"), Err(GraphError::EntityNotFound(ref id)) if id == "X")
        );
        assert!(
            matches!(query.related("X", RelationshipType::Requires), Err(GraphError::EntityNotFound(ref id)) if id == "X")
        );
    }

    #[test]
    fn test_cycle_detection_requires() {
        // A requires B, B requires A  →  cycle
        let a = make_entity("A", EntityType::Concept, vec![("B", RelationshipType::Requires)]);
        let b = make_entity("B", EntityType::Concept, vec![("A", RelationshipType::Requires)]);
        let module = Module {
            entities: vec![a, b],
        };

        let err = KnowledgeGraph::from_module(&module).unwrap_err();
        assert!(matches!(err, GraphError::Cycle { .. }));
        if let GraphError::Cycle { rel_type, chain } = err {
            assert_eq!(rel_type, "requires");
            assert!(chain.contains("A"));
            assert!(chain.contains("B"));
        }
    }

    #[test]
    fn test_cycle_detection_derives_from() {
        // A derives_from B, B derives_from C, C derives_from A  →  cycle
        let a = make_entity("A", EntityType::Concept, vec![("B", RelationshipType::DerivesFrom)]);
        let b = make_entity("B", EntityType::Concept, vec![("C", RelationshipType::DerivesFrom)]);
        let c = make_entity("C", EntityType::Concept, vec![("A", RelationshipType::DerivesFrom)]);
        let module = Module {
            entities: vec![a, b, c],
        };

        let err = KnowledgeGraph::from_module(&module).unwrap_err();
        assert!(matches!(err, GraphError::Cycle { .. }));
        if let GraphError::Cycle { rel_type, chain } = err {
            assert_eq!(rel_type, "derives_from");
            assert!(chain.contains("A"));
            assert!(chain.contains("B"));
            assert!(chain.contains("C"));
        }
    }

    #[test]
    fn test_no_cycle_for_other_rel_types() {
        // A references B, B references A  →  NOT a cycle (only Requires / DerivesFrom matter)
        let a = make_entity("A", EntityType::Concept, vec![("B", RelationshipType::References)]);
        let b = make_entity("B", EntityType::Concept, vec![("A", RelationshipType::References)]);
        let module = Module {
            entities: vec![a, b],
        };

        let graph = KnowledgeGraph::from_module(&module).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 2);
    }
}
