pub mod commands;

use crate::codegen::{dot, json, markdown, search};
use crate::error::{VdlError, VdlResult};
use crate::graph::{entity_type_color, KnowledgeGraph};
use crate::lexer;
use crate::parser;
use crate::parser::ast::Module;
use crate::validator;
use miette::{miette, Result};
use regex::Regex;
use std::collections::{BTreeSet, HashSet};
use std::fs;
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
    let path = Path::new(path);

    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("vdl") {
            Ok(vec![path.to_path_buf()])
        } else {
            Err(miette!("Path is not a .vdl file: {}", path.display()))
        }
    } else if path.is_dir() {
        let mut files = Vec::new();
        collect_vdl_files_recursive(path, &mut files)
            .map_err(|e| miette!("Failed to scan directory: {}", e))?;
        files.sort();
        Ok(files)
    } else {
        Err(miette!("Path does not exist: {}", path.display()))
    }
}

fn collect_vdl_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_vdl_files_recursive(&path, files)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("vdl") {
            files.push(path);
        }
    }
    Ok(())
}

/// Read a file to string, mapping IO errors into [`VdlError::Io`].
fn read_file_to_string(path: &Path) -> VdlResult<String> {
    fs::read_to_string(path).map_err(|e| VdlError::Io {
        message: format!("Failed to read {}: {}", path.display(), e),
    })
}

/// Returns `true` when `out` resolves to the same file as `input`.
///
/// Canonical paths cover relative segments, `..`, and symlinks. On Unix the
/// device/inode pair also catches hard links; on other platforms a hard link is
/// not detected because the standard library exposes no file identity there.
fn output_aliases_input(input: &Path, out: &Path) -> std::io::Result<bool> {
    let input_meta = fs::metadata(input)?;
    let input_canon = fs::canonicalize(input)?;

    // Existing target: compare identity directly (hard links included on Unix).
    if let Ok(out_meta) = fs::metadata(out) {
        if same_file(&input_meta, &out_meta) {
            return Ok(true);
        }
    }

    // Nonexistent target: canonicalize the parent so `sub/../x.vdl` still matches.
    let out_canon = match fs::canonicalize(out) {
        Ok(path) => path,
        Err(_) => {
            let parent = out
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let Some(name) = out.file_name() else {
                return Ok(false);
            };
            match fs::canonicalize(parent) {
                Ok(parent) => parent.join(name),
                // The target cannot be the input when its own directory does not
                // resolve; let the write fail with the ordinary IO message.
                Err(_) => return Ok(false),
            }
        }
    };

    Ok(out_canon == input_canon)
}

#[cfg(unix)]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(_a: &fs::Metadata, _b: &fs::Metadata) -> bool {
    false
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
        commands::Commands::Compose { path, root, out } => {
            cmd_compose(&path, &root, out.as_deref())
        }
    }
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

/// Validate VDL files and report errors with precise locations.
///
/// Each file is dispatched to its own pipeline: legacy entity documents are
/// merged and validated together, schema documents are validated on their own
/// because a schema document is single-file by contract.
fn cmd_validate(path: &str) -> Result<()> {
    let files = collect_vdl_files(path)?;
    let file_count = files.len();

    let mut legacy_entities = Vec::new();
    let mut schema_entity_count = 0usize;

    for file in &files {
        let source = wrap(read_file_to_string(file))?;
        let tokens = wrap(lexer::lex(&source, file))?;

        match wrap(parser::parse_document(&tokens, &source, file))? {
            parser::Document::Legacy(module) => legacy_entities.extend(module.entities),
            parser::Document::Schema(doc) => {
                wrap(validator::schema::validate_schema(&doc))?;
                schema_entity_count += doc.entities.len();
            }
        }
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
        file_count,
        legacy_entity_count + schema_entity_count
    );
    Ok(())
}

/// Compose one schema document into a complete Markdown file.
fn cmd_compose(path: &str, root: &str, out: Option<&Path>) -> Result<()> {
    let files = collect_vdl_files(path)?;
    let file = match files.as_slice() {
        [single] => single.clone(),
        _ => {
            return Err(miette!(
                "vdl compose takes exactly one .vdl schema document, found {}",
                files.len()
            ))
        }
    };

    if let Some(out_path) = out {
        if output_aliases_input(&file, out_path).map_err(|e| {
            miette!(
                "Failed to compare output path {} with input: {}",
                out_path.display(),
                e
            )
        })? {
            return Err(miette!(
                "refusing to write output over the input file '{}'",
                file.display()
            ));
        }
    }

    let source = wrap(read_file_to_string(&file))?;
    let tokens = wrap(lexer::lex(&source, &file))?;
    let doc = match wrap(parser::parse_document(&tokens, &source, &file))? {
        parser::Document::Schema(doc) => doc,
        parser::Document::Legacy(_) => {
            return Err(miette!(
                "composition requires a schema document; '{}' uses the legacy entity form",
                file.display()
            ))
        }
    };

    let markdown = wrap(crate::codegen::compose::compose(&doc, root))?;

    match out {
        Some(out_path) => {
            fs::write(out_path, markdown)
                .map_err(|e| miette!("Failed to write {}: {}", out_path.display(), e))?;
            println!("✓ Composed '{}' → {}", root, out_path.display());
        }
        None => print!("{}", markdown),
    }

    Ok(())
}

/// Compile VDL files through the full pipeline.
fn cmd_compile(path: &str) -> Result<()> {
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
