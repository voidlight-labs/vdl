//! Voidlight Definition Language (VDL) Compiler
//!
//! A standalone Rust compiler that transforms declarative knowledge definitions
//! into multiple output formats including JSON Graph, Markdown, Search Index,
//! and Graphviz DOT.
//!
//! # Architecture
//!
//! The compiler follows a 5-stage pipeline:
//!
//! 1. **Lexer** — Tokenizes VDL source into semantic tokens
//! 2. **Parser** — Builds an Abstract Syntax Tree (AST)
//! 3. **Validator** — Checks type constraints, reference integrity, evidence rules
//! 4. **Graph Builder** — Constructs a directed acyclic knowledge graph
//! 5. **Code Generator** — Produces JSON, Markdown, Search Index, and DOT output
//!
//! # Usage
//!
//! ```no_run
//! use std::path::Path;
//!
//! // Validate a VDL file
//! // vdl::validate(Path::new("input.vdl"));
//!
//! // Compile to all targets
//! // vdl::compile(Path::new("input.vdl"), Path::new("output/"));
//! ```

pub mod codegen;
pub mod error;
pub mod graph;
pub mod lexer;
pub mod parser;
pub mod validator;

// CLI is primarily for the binary, but exposed for library consumers who want it.
pub mod cli;

pub use error::{SourceLocation, VdlError, VdlResult};
pub use parser::ast::{
    Annotation, Analogy, Entity, EntityType, EvidenceBlock, Module, Relationship, RelationshipType,
    Revelation, Synthesis,
};
