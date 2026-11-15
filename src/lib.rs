//! Voidlight Definition Language (VDL) Compiler
//!
//! A standalone Rust compiler for schema-authored VDL documents: each document
//! declares its own model in a `schema` block, instances are validated against
//! it, and projects compose into Markdown.
//!
//! # Architecture
//!
//! 1. **Lexer** — Tokenizes VDL source into semantic tokens
//! 2. **Parser** — Parses schema documents into a [`model::SchemaDoc`]
//! 3. **Validator** — Checks the schema and every instance against it
//! 4. **Composer** — Renders a root entity and its composed collections to
//!    Markdown (`codegen::compose`), guarded by the project build
//!    (`project::build_project`) for `vdl.toml` targets.
//!
//! # Usage
//!
//! ```no_run
//! use std::path::Path;
//! use vdl::{lexer, parser, validator::schema::validate_schema};
//!
//! let source = r#"schema "example" { type "Page" { field "name" text } }
//! Page "intro" { name "Introduction" }"#;
//! let path = Path::new("input.vdl");
//! let tokens = lexer::lex(source, path)?;
//! let doc = parser::parse_document(&tokens, source, path)?;
//! validate_schema(&doc)?;
//! # Ok::<(), vdl::VdlError>(())
//! ```

pub mod codegen;
pub mod error;
pub mod lexer;
pub mod model;
pub mod parser;
pub mod project;
pub mod validator;

mod fsutil;

// CLI is primarily for the binary, but exposed for library consumers who want it.
pub mod cli;

pub use error::{SourceLocation, VdlError, VdlResult};
pub use model::{
    FieldDecl, FieldKind, FieldValue, ImportDecl, InlineItem, Instance, Schema, SchemaDoc, TypeDecl,
    Value,
};
pub use parser::parse_document;
