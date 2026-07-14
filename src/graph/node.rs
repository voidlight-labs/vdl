use crate::graph::GraphError;
use crate::parser::ast::{Entity, Module, RelationshipType};
use indexmap::IndexMap;
use std::collections::HashSet;

/// An in-memory directed knowledge graph.
///
/// Nodes are entities keyed by their ID. Edges are typed relationships
/// extracted from the parsed and validated AST.
#[derive(Debug, Clone)]
pub struct KnowledgeGraph {
    /// All entities in the graph, keyed by ID.
    pub(crate) nodes: IndexMap<String, Entity>,
    /// All edges in the graph.
    pub(crate) edges: Vec<Edge>,
    /// Adjacency list: from_id -> [(to_id, relationship_type)].
    pub(crate) adjacency: IndexMap<String, Vec<(String, RelationshipType)>>,
}

impl KnowledgeGraph {
    /// Create a new empty knowledge graph.
    pub fn new() -> Self {
        Self {
            nodes: IndexMap::new(),
            edges: Vec::new(),
            adjacency: IndexMap::new(),
        }
    }

    /// Build a knowledge graph from a parsed VDL module.
    ///
    /// Inserts all entities into the node map, creates edges from each
    /// entity's relationships, builds the forward adjacency list, and
    /// validates that there are no cycles through `requires` or
    /// `derives_from` edges.
    pub fn from_module(module: &Module) -> Result<Self, GraphError> {
        let mut graph = Self::new();

        // 1. Insert all entities keyed by id
        for entity in &module.entities {
            graph.nodes.insert(entity.id.clone(), entity.clone());
        }

        // 2. Create edges and adjacency list from relationships
        for entity in &module.entities {
            let from_id = entity.id.clone();
            for rel in &entity.relationships {
                let edge = Edge {
                    from: from_id.clone(),
                    to: rel.target_id.clone(),
                    rel_type: rel.rel_type,
                };
                graph.edges.push(edge);
                graph
                    .adjacency
                    .entry(from_id.clone())
                    .or_default()
                    .push((rel.target_id.clone(), rel.rel_type));
            }
        }

        // 3. Validate no cycles in requires / derives_from edges
        graph.validate_cycles()?;

        Ok(graph)
    }

    /// BFS-based cyclic dependency detector.
    ///
    /// Checks all `Requires` and `DerivesFrom` edges for cycles.  If a
    /// cycle is found, returns [`GraphError::Cycle`] with the
    /// relationship type that closes the loop and a `->` joined chain
    /// of entity IDs.
    pub fn validate_cycles(&self) -> Result<(), GraphError> {
        let mut fully_explored = HashSet::new();

        for start_node in self.nodes.keys().cloned().collect::<Vec<_>>() {
            if fully_explored.contains(&start_node) {
                continue;
            }

            // Iterative DFS with explicit path tracking.
            // Each stack frame carries the current node and the path
            // (as a Vec) from the start node to it.
            let mut stack = vec![(start_node.clone(), vec![start_node.clone()])];

            while let Some((current, path)) = stack.pop() {
                if let Some(neighbors) = self.adjacency.get(&current) {
                    for (to_id, rel_type) in neighbors {
                        if *rel_type != RelationshipType::Requires
                            && *rel_type != RelationshipType::DerivesFrom
                        {
                            continue;
                        }

                        if let Some(pos) = path.iter().position(|id| id == to_id) {
                            let cycle_ids: Vec<_> =
                                path[pos..].iter().cloned().collect();
                            let chain = cycle_ids.join(" -> ") + " -> " + to_id;
                            return Err(GraphError::Cycle {
                                rel_type: rel_type.to_string(),
                                chain,
                            });
                        }

                        if !fully_explored.contains(to_id) {
                            let mut new_path = path.clone();
                            new_path.push(to_id.clone());
                            stack.push((to_id.clone(), new_path));
                        }
                    }
                }

                fully_explored.insert(current);
            }
        }

        Ok(())
    }
}

impl KnowledgeGraph {
    /// Return a reference to all entities in the graph, keyed by ID.
    pub fn nodes(&self) -> &IndexMap<String, Entity> {
        &self.nodes
    }

    /// Return a reference to all edges in the graph.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Return a reference to the forward adjacency list.
    pub fn adjacency(&self) -> &IndexMap<String, Vec<(String, RelationshipType)>> {
        &self.adjacency
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A directed edge in the knowledge graph.
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub rel_type: RelationshipType,
}
