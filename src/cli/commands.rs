use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Voidlight Definition Language compiler.
///
/// Validates schema documents and composes schema projects into Markdown.
#[derive(Parser)]
#[command(name = "vdl")]
#[command(about = "Voidlight Definition Language compiler")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

// The graph output format lives with the graph domain and is re-exported so
// `commands::GraphFormat` keeps working for CLI consumers.
pub use crate::graph::GraphFormat;

#[derive(Subcommand)]
pub enum Commands {
    /// Validate VDL files and report errors with precise locations.
    ///
    /// A file or directory is loaded as a schema project: every `.vdl` file is
    /// merged, exactly one schema block is required, and IDs resolve across
    /// folders without a build manifest. Documents in the removed legacy
    /// (v0.1 fixed-type) form are rejected with a migration hint.
    Validate {
        /// Path to a .vdl file or directory containing .vdl files.
        path: String,
    },

    /// Compile a schema project through its `vdl.toml` manifest.
    ///
    /// Every registered Markdown target is rendered, validated, and only then
    /// written. A directory without a `vdl.toml` is an error, not a fallback.
    Compile {
        /// Project directory containing vdl.toml.
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

    /// Emit the project graph as JSON, Graphviz DOT, or a search index.
    ///
    /// The input may be one `.vdl` file or a directory of project sources,
    /// loaded exactly like `validate`. Nodes are entities in declaration
    /// order; edges are reference-field targets in authored order. No type or
    /// field name is known to the compiler: the graph is read entirely from
    /// the schema.
    Graph {
        /// Path to one .vdl schema document or a project directory.
        path: String,
        /// Output format: json, dot, or index.
        #[arg(long, value_enum, default_value_t = GraphFormat::Json)]
        format: GraphFormat,
        /// Restrict output to one entity and its one-hop neighborhood.
        #[arg(long)]
        root: Option<String>,
        /// Output file path. Prints to stdout when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Search entity content for a case-insensitive substring.
    ///
    /// Matches entity IDs, type names, heading labels, text fields, and body
    /// content; prints one line per matching entity in declaration order.
    Search {
        /// Case-insensitive substring to look for.
        query: String,
        /// Path to one .vdl schema document or a project directory.
        path: String,
    },
}
