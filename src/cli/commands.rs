use clap::{Parser, Subcommand};

/// Voidlight Definition Language compiler.
///
/// Transforms declarative knowledge definitions into JSON Graph, Markdown,
/// Search Index, and Graphviz DOT output.
#[derive(Parser)]
#[command(name = "vdl")]
#[command(about = "Voidlight Definition Language compiler")]
#[command(version = "0.1.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Validate VDL files and report errors with precise locations.
    Validate {
        /// Path to a .vdl file or directory containing .vdl files.
        path: String,
    },

    /// Compile VDL files through the full pipeline.
    ///
    /// Validates, builds the knowledge graph, and generates all output targets.
    Compile {
        /// Path to a .vdl file or directory containing .vdl files.
        path: String,
    },

    /// Export a subgraph centered on an entity to DOT format.
    Graph {
        /// Entity ID to center the subgraph on.
        entity_id: String,
    },

    /// Compare two versions of the same entity.
    Diff {
        /// Entity ID.
        id: String,
        /// First version to compare.
        v1: String,
        /// Second version to compare.
        v2: String,
    },

    /// Search entity IDs and titles using a regex pattern.
    Search {
        /// Search query (regex).
        query: String,
    },
}
