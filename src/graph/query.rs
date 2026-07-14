use crate::parser::ast::{Entity, RelationshipType};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

/// Errors that can occur during graph operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum GraphError {
    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Cycle detected in {rel_type}: {chain}")]
    Cycle {
        rel_type: String,
        chain: String,
    },
}

/// Query interface for the knowledge graph.
///
/// Provides `ancestors`, `descendants`, and `related` queries.
pub struct GraphQuery<'a> {
    graph: &'a super::KnowledgeGraph,
    reverse_adj: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> GraphQuery<'a> {
    /// Create a new query bound to the given graph.
    ///
    /// Precomputes the reverse adjacency view used by `descendants`.
    pub fn new(graph: &'a super::KnowledgeGraph) -> Self {
        let mut reverse_adj: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
        for edge in &graph.edges {
            if edge.rel_type == RelationshipType::Requires
                || edge.rel_type == RelationshipType::DerivesFrom
            {
                reverse_adj.entry(&edge.to).or_default().push(&edge.from);
            }
        }
        Self {
            graph,
            reverse_adj,
        }
    }

    /// Find all ancestors of an entity by following outgoing `requires` and
    /// `derives_from` edges.
    ///
    /// An ancestor is any entity that the given entity directly or indirectly
    /// depends on through `requires` or `derives_from` relationships.
    ///
    /// Uses cycle-safe BFS.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::EntityNotFound`] if the entity does not exist.
    pub fn ancestors(&self, entity_id: &str) -> Result<Vec<&Entity>, GraphError> {
        self.graph
            .nodes
            .get(entity_id)
            .ok_or_else(|| GraphError::EntityNotFound(entity_id.to_string()))?;

        let mut visited = HashSet::new();
        visited.insert(entity_id.to_string());
        let mut queue = VecDeque::new();
        queue.push_back(entity_id.to_string());
        let mut result = Vec::new();

        while let Some(current_id) = queue.pop_front() {
            if let Some(neighbors) = self.graph.adjacency.get(&current_id) {
                for (to_id, rel_type) in neighbors {
                    if *rel_type == RelationshipType::Requires
                        || *rel_type == RelationshipType::DerivesFrom
                    {
                        if visited.insert(to_id.clone()) {
                            if let Some(entity) = self.graph.nodes.get(to_id) {
                                result.push(entity);
                                queue.push_back(to_id.clone());
                            }
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Find all descendants of an entity by following incoming `requires` and
    /// `derives_from` edges.
    ///
    /// A descendant is any entity that directly or indirectly depends on the
    /// given entity through `requires` or `derives_from` relationships.
    ///
    /// Uses cycle-safe BFS over the precomputed reverse adjacency view.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::EntityNotFound`] if the entity does not exist.
    pub fn descendants(&self, entity_id: &str) -> Result<Vec<&Entity>, GraphError> {
        self.graph
            .nodes
            .get(entity_id)
            .ok_or_else(|| GraphError::EntityNotFound(entity_id.to_string()))?;

        let mut visited = HashSet::new();
        visited.insert(entity_id.to_string());
        let mut queue = VecDeque::new();
        queue.push_back(entity_id.to_string());
        let mut result = Vec::new();

        while let Some(current_id) = queue.pop_front() {
            if let Some(parents) = self.reverse_adj.get(current_id.as_str()) {
                for &from_id in parents {
                    if visited.insert(from_id.to_string()) {
                        if let Some(entity) = self.graph.nodes.get(from_id) {
                            result.push(entity);
                            queue.push_back(from_id.to_string());
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Find all entities related to the given entity by a specific relationship type.
    ///
    /// Follows outgoing edges of the requested type.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::EntityNotFound`] if the entity does not exist.
    pub fn related(
        &self,
        entity_id: &str,
        rel_type: RelationshipType,
    ) -> Result<Vec<&Entity>, GraphError> {
        self.graph
            .nodes
            .get(entity_id)
            .ok_or_else(|| GraphError::EntityNotFound(entity_id.to_string()))?;

        let mut result = Vec::new();
        if let Some(neighbors) = self.graph.adjacency.get(entity_id) {
            for (to_id, edge_type) in neighbors {
                if *edge_type == rel_type {
                    if let Some(entity) = self.graph.nodes.get(to_id) {
                        result.push(entity);
                    }
                }
            }
        }
        Ok(result)
    }
}
