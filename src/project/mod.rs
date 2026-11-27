//! Multi-file schema projects: discovery, manifest, and Markdown build.
//!
//! A project is any directory of `.vdl` files with exactly one schema block.
//! Entity IDs are project-global; a file's name and folder never matter.
//! `vdl.toml` turns the project into a build by listing roots and output
//! paths. See [`manifest`] for the manifest contract and [`build`] for the
//! guard rails around writing output.

mod build;
mod loader;
mod manifest;

pub use build::{build_project, build_project_with_options, BuildReport};
pub use loader::{discover_vdl_files, load_schema_project, SchemaProject};
pub(crate) use loader::resolve_import_references;
pub use manifest::{
    load_manifest, missing_manifest_error, Manifest, MarkdownTarget, MANIFEST_FILE,
};
