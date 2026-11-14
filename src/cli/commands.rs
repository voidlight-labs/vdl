use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Voidlight Definition Language compiler.
///
/// Transforms declarative knowledge definitions into JSON Graph, Markdown,
/// Search Index, and Graphviz DOT output.
#[derive(Parser)]
#[command(name = "vdl")]
#[command(about = "Voidlight Definition Language compiler")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Validate VDL files and report errors with precise locations.
    ///
    /// A directory that contains schema documents is validated as one project:
    /// every `.vdl` file is merged, exactly one schema block is required, and
    /// IDs resolve across folders without a build manifest.
    Validate {
        /// Path to a .vdl file or directory containing .vdl files.
        path: String,
    },

    /// Compile VDL files through the full pipeline.
    ///
    /// A directory with `vdl.toml` is a schema project: every registered
    /// Markdown target is rendered, validated, and only then written. A
    /// directory without a manifest keeps the legacy graph pipeline.
    Compile {
        /// Path to a .vdl file, or a project directory containing vdl.toml.
        path: String,
    },

    /// Export a subgraph centered on an entity to DOT format.
    Graph {
        /// Entity ID to center the subgraph on.
        entity_id: String,
        /// Path to a .vdl file or directory containing .vdl files.
        #[arg(default_value = ".")]
        path: String,
    },

    /// Compare two versions of the same entity.
    Diff {
        /// Entity ID.
        id: String,
        /// First version to compare.
        v1: String,
        /// Second version to compare.
        v2: String,
        /// Path to a .vdl file or directory containing .vdl files.
        #[arg(default_value = ".")]
        path: String,
    },

    /// Search entity IDs and titles using a regex pattern.
    Search {
        /// Search query (regex).
        query: String,
        /// Path to a .vdl file or directory containing .vdl files.
        #[arg(default_value = ".")]
        path: String,
    },

    /// Compose a schema document or project into one Markdown file.
    ///
    /// The input may be one `.vdl` file or a directory of project sources; a
    /// directory is merged exactly like `validate` and needs no manifest.
    /// Composition renders the root entity, its heading and body, then every
    /// collection marked `compose` in declared order.
    Compose {
        /// Path to one .vdl schema document or a project directory.
        path: String,
        /// Entity ID of the composition root.
        #[arg(long)]
        root: String,
        /// Output file path. Prints to stdout when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
        /// How `cite`-flagged reference fields render: none, link, or short.
        #[arg(long)]
        cites: Option<String>,
    },
}
