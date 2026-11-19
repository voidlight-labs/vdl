pub mod commands;

use crate::error::{VdlError, VdlResult};
#[cfg(any(unix, windows))]
use crate::fsutil::same_file;
use crate::project;
use miette::{miette, Result};
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
        VdlError::Codegen { message } => miette!("Codegen error: {}", message),
        VdlError::Graph { message } => miette!("Graph error: {}", message),
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
        commands::Commands::Compose { path, root, out, cites } => {
            cmd_compose(&path, &root, out.as_deref(), cites.as_deref())
        }
        commands::Commands::Graph { path, format, root, out } => {
            cmd_graph(&path, format, root.as_deref(), out.as_deref())
        }
        commands::Commands::Search { query, path } => cmd_search(&query, &path),
    }
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

/// Validate VDL files and report errors with precise locations.
///
/// The path is loaded as one schema project: all files are merged, the single
/// schema block is checked once, and references resolve project-wide. Legacy
/// (v0.1 fixed-type) documents are rejected by the loader with a migration
/// hint.
fn cmd_validate(path: &str) -> Result<()> {
    let files = collect_vdl_files(path)?;
    let file_count = files.len();

    let loaded = wrap(project::load_schema_project(Path::new(path)))?;
    println!(
        "✓ Validated {} files, {} entities. No errors.",
        file_count,
        loaded.doc.entities.len()
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
            write_protected(&project, out_path, &markdown)?;
            println!("✓ Composed '{}' → {}", root, out_path.display());
        }
        None => print!("{}", markdown),
    }

    Ok(())
}

/// Write `contents` to `out_path`, refusing to truncate any project source or
/// the manifest.
///
/// The output is opened without truncation and compared with every protected
/// file before anything is written, so a path change after the check cannot
/// redirect the write to the input.
fn write_protected(project: &project::SchemaProject, out_path: &Path, contents: &str) -> Result<()> {
    let write_error = |e| miette!("Failed to write {}: {}", out_path.display(), e);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES};

        options.access_mode(FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES);
    }
    let mut output = options.open(out_path).map_err(write_error)?;

    for protected in project.protected_files() {
        #[cfg(any(unix, windows))]
        let aliases = fs::File::open(protected).and_then(|input| same_file(&output, &input));
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
    output.write_all(contents.as_bytes()).map_err(write_error)?;
    Ok(())
}

/// Compile a schema project through its `vdl.toml` manifest.
///
/// A directory without a manifest is an error, not a fallback: there is no
/// legacy graph pipeline anymore.
fn cmd_compile(path: &str) -> Result<()> {
    let target = Path::new(path);

    if !target.is_dir() {
        return Err(miette!(
            "`vdl compile` requires a project directory with a '{}' manifest; '{}' is not a directory.",
            project::MANIFEST_FILE,
            path
        ));
    }

    if !target.join(project::MANIFEST_FILE).is_file() {
        return Err(miette!("{}", project::missing_manifest_error(target)));
    }

    let report = wrap(project::build_project(target))?;
    for output in report.outputs {
        println!("  → {}", output.display());
    }
    println!("✓ Compilation complete.");
    Ok(())
}

/// Emit the project graph as JSON, DOT, or a search index.
///
/// The input is loaded exactly like `validate`; `--root` narrows the output
/// to one entity and its one-hop neighborhood before rendering.
fn cmd_graph(
    path: &str,
    format: commands::GraphFormat,
    root: Option<&str>,
    out: Option<&Path>,
) -> Result<()> {
    use commands::GraphFormat;
    use std::collections::HashSet;

    let project = wrap(project::load_schema_project(Path::new(path)))?;
    let graph = crate::graph::Graph::build(&project.doc);
    let graph = match root {
        Some(root) => wrap(graph.one_hop(root))?,
        None => graph,
    };

    let output = match format {
        GraphFormat::Json => crate::graph::json::to_json(&graph),
        GraphFormat::Dot => crate::graph::dot::to_dot(&graph),
        GraphFormat::Index => {
            let documents = crate::graph::search::build_index(&project.doc);
            let kept: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
            let kept: Vec<_> = documents
                .iter()
                .filter(|d| kept.contains(d.id.as_str()))
                .cloned()
                .collect();
            crate::graph::search::to_json(&kept)
        }
    };

    match out {
        Some(out_path) => {
            write_protected(&project, out_path, &output)?;
            println!("✓ Graph written to {}", out_path.display());
        }
        None => print!("{}", output),
    }

    Ok(())
}

/// Search entity content for a case-insensitive substring.
fn cmd_search(query: &str, path: &str) -> Result<()> {
    let project = wrap(project::load_schema_project(Path::new(path)))?;
    let documents = crate::graph::search::build_index(&project.doc);

    let mut matched = false;
    for document in &documents {
        if crate::graph::search::matches(document, query) {
            matched = true;
            println!(
                "{} | {} | {}",
                document.id,
                document.type_name,
                document.label.as_deref().unwrap_or("-")
            );
        }
    }
    if !matched {
        println!("No entities matching '{}'.", query);
    }
    Ok(())
}
