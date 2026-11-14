pub mod commands;

use crate::codegen::{dot, json, markdown, search};
use crate::error::{VdlError, VdlResult};
#[cfg(any(unix, windows))]
use crate::fsutil::same_file;
use crate::graph::{entity_type_color, KnowledgeGraph};
use crate::lexer;
use crate::parser;
use crate::parser::ast::Module;
use crate::project;
use crate::validator;
use miette::{miette, Result};
use regex::Regex;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Error conversion: VdlError → miette::Report
// ---------------------------------------------------------------------------

/// Convert a [`VdlError`] into a [`miette::Report`] for rich CLI diagnostics.
fn report_vdl_error(err: VdlError) -> miette::Report {
    match err {
        VdlError::Lexer {
            location,
            message,
        } => miette!(
            "[{}:{}:{}] Lexical error: {}",
            location.file.display(),
            location.line,
            location.column,
            message
        ),
        VdlError::Parser {
            location,
            message,
        } => miette!(
            "[{}:{}:{}] Parse error: {}",
            location.file.display(),
            location.line,
            location.column,
            message
        ),
        VdlError::Validation {
            location,
            message,
        } => miette!(
            "[{}:{}:{}] Validation error: {}",
            location.file.display(),
            location.line,
            location.column,
            message
        ),
        VdlError::ValidationErrors { count, messages } => {
            miette!("Validation failed with {} error(s):\n{}", count, messages)
        }
        VdlError::Graph { message } => miette!("Graph error: {}", message),
        VdlError::Codegen { message } => miette!("Codegen error: {}", message),
        VdlError::Io { message } => miette!("IO error: {}", message),
        VdlError::Other { message } => miette!("{}", message),
    }
}

/// Convenience wrapper: [`VdlResult<T>`] → [`miette::Result<T>`].
fn wrap<T>(result: VdlResult<T>) -> Result<T> {
    result.map_err(report_vdl_error)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect all `.vdl` files from a path.
///
/// * If `path` points to a single `.vdl` file → returns a one-element vec.
/// * If `path` is a directory → recursively finds all `.vdl` files, sorted
///   for deterministic output.
fn collect_vdl_files(path: &str) -> Result<Vec<PathBuf>> {
    wrap(project::discover_vdl_files(Path::new(path)))
}

/// Read a file to string, mapping IO errors into [`VdlError::Io`].
fn read_file_to_string(path: &Path) -> VdlResult<String> {
    fs::read_to_string(path).map_err(|e| VdlError::Io {
        message: format!("Failed to read {}: {}", path.display(), e),
    })
}

/// Parse and validate a set of `.vdl` files, returning the merged module.
fn parse_and_validate(files: &[PathBuf]) -> VdlResult<Module> {
    let mut all_entities = Vec::new();

    for file in files {
        let source = read_file_to_string(file)?;
        let tokens = lexer::lex(&source, file)?;
        let module = parser::parse(&tokens, &source, file)?;
        all_entities.extend(module.entities);
    }

    let module = Module {
        entities: all_entities,
    };

    validator::validate(&module)?;
    Ok(module)
}

/// Full compilation pipeline (stages 1–4): lex → parse → validate → graph.
fn compile_files(files: &[PathBuf]) -> VdlResult<KnowledgeGraph> {
    let module = parse_and_validate(files)?;
    KnowledgeGraph::from_module(&module).map_err(|e| VdlError::Graph {
        message: e.to_string(),
    })
}

// ---------------------------------------------------------------------------
// CLI entry point
// ---------------------------------------------------------------------------

/// Run the VDL CLI.
///
/// Parses command-line arguments and dispatches to the appropriate subcommand.
pub fn run() -> Result<()> {
    use clap::Parser;
    let args = commands::Cli::parse();

    match args.command {
        commands::Commands::Validate { path } => cmd_validate(&path),
        commands::Commands::Compile { path } => cmd_compile(&path),
        commands::Commands::Graph { entity_id, path } => cmd_graph(&entity_id, &path),
        commands::Commands::Diff { id, v1, v2, path } => cmd_diff(&id, &v1, &v2, &path),
        commands::Commands::Search { query, path } => cmd_search(&query, &path),
        commands::Commands::Compose { path, root, out, cites } => {
            cmd_compose(&path, &root, out.as_deref(), cites.as_deref())
        }
    }
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

/// Validate VDL files and report errors with precise locations.
///
/// A directory containing schema documents is validated as one project: all
/// files are merged, the single schema block is checked once, and references
/// resolve project-wide. Directories that contain only legacy documents keep
/// the original per-file behavior.
fn cmd_validate(path: &str) -> Result<()> {
    let files = collect_vdl_files(path)?;
    let file_count = files.len();

    if wrap(project::contains_schema_documents(Path::new(path)))? {
        let loaded = wrap(project::load_schema_project(Path::new(path)))?;
        println!(
            "✓ Validated {} files, {} entities. No errors.",
            file_count,
            loaded.doc.entities.len()
        );
        return Ok(());
    }

    let mut legacy_entities = Vec::new();

    for file in &files {
        let source = wrap(read_file_to_string(file))?;
        let tokens = wrap(lexer::lex(&source, file))?;
        let module = wrap(parser::parse(&tokens, &source, file))?;
        legacy_entities.extend(module.entities);
    }

    let legacy_entity_count = legacy_entities.len();
    if legacy_entity_count > 0 {
        let module = Module {
            entities: legacy_entities,
        };
        wrap(validator::validate(&module))?;
    }

    println!(
        "✓ Validated {} files, {} entities. No errors.",
        file_count, legacy_entity_count
    );
    Ok(())
}

/// Compose one root entity from a schema project into Markdown.
///
/// The input may be one `.vdl` file or a project directory. `--out` is only
/// opened after the whole project has loaded, and the open handle is compared
/// with every project source and the manifest before anything is truncated.
fn cmd_compose(path: &str, root: &str, out: Option<&Path>, cites: Option<&str>) -> Result<()> {
    let project = wrap(project::load_schema_project(Path::new(path)))?;
    let options = crate::codegen::compose::ComposeOptions {
        cites: match cites.map(str::trim) {
            None | Some("") | Some("none") => crate::codegen::compose::CiteMode::None,
            Some("link") => crate::codegen::compose::CiteMode::Link,
            Some("short") => crate::codegen::compose::CiteMode::Short,
            Some(other) => {
                return Err(miette!("--cites must be one of \"none\", \"link\", \"short\"; found \"{}\"", other))
            }
        },
        number_headings: false,
    };
    let markdown = wrap(crate::codegen::compose::compose_with_options(&project.doc, root, &options))?;

    match out {
        Some(out_path) => {
            let write_error = |e| miette!("Failed to write {}: {}", out_path.display(), e);
            // Compare the open files before truncating, so a path change after
            // the check cannot redirect the write to the input.
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(false);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
                };

                options.access_mode(FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES);
            }
            let mut output = options.open(out_path).map_err(write_error)?;

            for protected in project.protected_files() {
                #[cfg(any(unix, windows))]
                let aliases =
                    fs::File::open(protected).and_then(|input| same_file(&output, &input));
                #[cfg(not(any(unix, windows)))]
                let aliases = crate::fsutil::paths_alias(out_path, protected);

                let aliases = aliases.map_err(|e| {
                    miette!(
                        "Failed to compare output path {} with project source '{}': {}",
                        out_path.display(),
                        protected.display(),
                        e
                    )
                })?;
                if aliases {
                    if project.manifest_path.as_deref() == Some(protected) {
                        return Err(miette!(
                            "refusing to write output over the build manifest '{}'",
                            protected.display()
                        ));
                    }
                    return Err(miette!(
                        "refusing to write output over the input file '{}'",
                        protected.display()
                    ));
                }
            }

            output.set_len(0).map_err(write_error)?;
            output.write_all(markdown.as_bytes()).map_err(write_error)?;
            println!("✓ Composed '{}' → {}", root, out_path.display());
        }
        None => print!("{}", markdown),
    }

    Ok(())
}

/// Compile VDL files through the full pipeline.
///
/// A directory with a `vdl.toml` is a schema project: its manifest targets are
/// rendered to Markdown and nothing is written until every target passes.
/// Directories without a manifest keep the legacy graph pipeline.
fn cmd_compile(path: &str) -> Result<()> {
    let target = Path::new(path);

    if target.is_dir() && target.join(project::MANIFEST_FILE).is_file() {
        let report = wrap(project::build_project(target))?;
        for output in report.outputs {
            println!("  → {}", output.display());
        }
        println!("✓ Compilation complete.");
        return Ok(());
    }

    if target.is_dir() && wrap(project::contains_schema_documents(target))? {
        return Err(miette!("{}", project::missing_manifest_error(target)));
    }

    let files = collect_vdl_files(path)?;
    println!("Found {} VDL file(s) to compile.", files.len());

    let graph = wrap(compile_files(&files))?;
    println!("✓ Validation and graph construction complete.");

    let output_dir = Path::new("output");
    fs::create_dir_all(output_dir)
        .map_err(|e| miette!("Failed to create output directory: {}", e))?;
    fs::create_dir_all(output_dir.join("soul"))
        .map_err(|e| miette!("Failed to create output/soul directory: {}", e))?;

    println!("Generating output targets…");

    wrap(json::generate(&graph, &output_dir.join("graph.json")))?;
    println!("  → output/graph.json");

    wrap(markdown::generate(&graph, &output_dir.join("soul")))?;
    println!("  → output/soul/");

    wrap(search::generate(&graph, &output_dir.join("search.json")))?;
    println!("  → output/search.json");

    wrap(dot::generate(&graph, &output_dir.join("graph.dot")))?;
    println!("  → output/graph.dot");

    println!("✓ Compilation complete.");
    Ok(())
}

/// Export a subgraph centered on an entity to DOT format.
fn cmd_graph(entity_id: &str, path: &str) -> Result<()> {
    let files = collect_vdl_files(path)?;
    if files.is_empty() {
        return Err(miette!("No .vdl files found in '{}'.", path));
    }

    let graph = wrap(compile_files(&files))?;

    if !graph.nodes.contains_key(entity_id) {
        return Err(miette!("Entity not found: {}", entity_id));
    }

    // Build subgraph: entity + direct neighbours (1 hop each direction)
    let mut subgraph = HashSet::new();
    subgraph.insert(entity_id.to_string());

    // Outgoing neighbours
    if let Some(outgoing) = graph.adjacency.get(entity_id) {
        for (to_id, _) in outgoing {
            subgraph.insert(to_id.clone());
        }
    }

    // Incoming neighbours
    for (from_id, outgoing) in &graph.adjacency {
        for (to_id, _) in outgoing {
            if to_id == entity_id {
                subgraph.insert(from_id.clone());
            }
        }
    }

    // Emit DOT to stdout
    println!("digraph subgraph {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box, style=\"rounded,filled\"];");

    for id in &subgraph {
        if let Some(node) = graph.nodes.get(id) {
            let colour = entity_type_color(node.entity_type);
            let label = format!("{}\\n({})", node.id, node.entity_type);
            println!(
                "  \"{}\" [label=\"{}\", fillcolor=\"{}\"];",
                node.id, label, colour
            );
        }
    }

    for (from_id, outgoing) in &graph.adjacency {
        if !subgraph.contains(from_id) {
            continue;
        }
        for (to_id, rel_type) in outgoing {
            if subgraph.contains(to_id) {
                println!(
                    "  \"{}\" -> \"{}\" [label=\"{}\"];",
                    from_id, to_id, rel_type
                );
            }
        }
    }

    println!("}}");
    Ok(())
}

/// Compare two versions of the same entity.
///
/// v0.1 simplified: v1 and v2 are ignored; we search for entities whose
/// ID contains `id` as a substring and diff the first two matches.
fn cmd_diff(id: &str, _v1: &str, _v2: &str, path: &str) -> Result<()> {
    let files = collect_vdl_files(path)?;
    if files.is_empty() {
        return Err(miette!("No .vdl files found in '{}'.", path));
    }

    let graph = wrap(compile_files(&files))?;

    let matches: Vec<_> = graph.nodes.values().filter(|e| e.id.contains(id)).collect();

    if matches.len() != 2 {
        println!(
            "Need exactly 2 entities matching '{}' for diff. Found {}.",
            id,
            matches.len()
        );
        return Ok(());
    }

    let a = matches[0];
    let b = matches[1];

    println!("Diff: {} vs {}", a.id, b.id);
    println!("{:-<50}", "");

    // -- properties ----------------------------------------------------
    let a_props: std::collections::HashMap<_, _> = a
        .properties
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let b_props: std::collections::HashMap<_, _> = b
        .properties
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let all_keys: BTreeSet<_> = a_props.keys().chain(b_props.keys()).copied().collect();
    for key in &all_keys {
        match (a_props.get(key), b_props.get(key)) {
            (Some(av), Some(bv)) if *av != *bv => {
                println!("  PROPERTY {}: '{}' | '{}'", key, av, bv);
            }
            (Some(av), None) => {
                println!("  PROPERTY {}: '{}' | <missing>", key, av);
            }
            (None, Some(bv)) => {
                println!("  PROPERTY {}: <missing> | '{}'", key, bv);
            }
            _ => {}
        }
    }

    // -- relationships -------------------------------------------------
    let a_rels: BTreeSet<_> = a
        .relationships
        .iter()
        .map(|r| (r.target_id.as_str(), r.rel_type.to_string()))
        .collect();
    let b_rels: BTreeSet<_> = b
        .relationships
        .iter()
        .map(|r| (r.target_id.as_str(), r.rel_type.to_string()))
        .collect();

    let only_in_a: Vec<_> = a_rels.difference(&b_rels).collect();
    let only_in_b: Vec<_> = b_rels.difference(&a_rels).collect();

    for (target, rel) in &only_in_a {
        println!("  REL only in {}: {} → {}", a.id, rel, target);
    }
    for (target, rel) in &only_in_b {
        println!("  REL only in {}: {} → {}", b.id, rel, target);
    }

    // -- basic fields --------------------------------------------------
    if a.title != b.title {
        println!("  TITLE: '{}' | '{}'", a.title, b.title);
    }
    if a.description != b.description {
        println!(
            "  DESCRIPTION: '{}' | '{}'",
            a.description, b.description
        );
    }
    if a.version != b.version {
        println!("  VERSION: '{}' | '{}'", a.version, b.version);
    }
    if a.entity_type != b.entity_type {
        println!("  TYPE: {} | {}", a.entity_type, b.entity_type);
    }

    if all_keys.is_empty() && only_in_a.is_empty() && only_in_b.is_empty() {
        println!("  (no differences found)");
    }

    Ok(())
}

/// Search entity IDs and titles using a regex pattern.
fn cmd_search(query: &str, path: &str) -> Result<()> {
    let files = collect_vdl_files(path)?;
    if files.is_empty() {
        return Err(miette!("No .vdl files found in '{}'.", path));
    }

    let graph = wrap(compile_files(&files))?;

    let re = Regex::new(&format!("(?i){}", regex::escape(query)))
        .map_err(|e| miette!("Invalid regex pattern: {}", e))?;

    let mut found = false;
    for entity in graph.nodes.values() {
        if re.is_match(&entity.id) || re.is_match(&entity.title) {
            println!("{} | {} | {}", entity.id, entity.entity_type, entity.title);
            found = true;
        }
    }

    if !found {
        println!("No entities matching '{}'.", query);
    }

    Ok(())
}
