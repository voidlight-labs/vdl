# Architecture Overview

```text
VDL sources → Lexer → Schema parser → Project merge / import resolution
           → Schema validation → Markdown composition → guarded build output
                              → Graph & search outputs (graph, search)
```

Schema projects load nested sources, resolve file-local imports, validate a
project-wide model, and render the Markdown targets declared in `vdl.toml`.
`validate`, `compose`, `graph`, and `search` also accept a standalone schema
document or a project without a manifest.

## Stage 1: Lexer (`src/lexer/`)

A hand-written byte-oriented scanner produces `Token` values with `Span`
information. It handles schema keywords, bare identifiers, quoted and raw
strings, braced Markdown bodies, delimiters, and line and block comments.

## Stage 2: Parser and project loader (`src/parser/`, `src/project/`)

The [chumsky](https://github.com/zesterer/chumsky) 0.9 parser turns a token
stream into a `SchemaDoc` or an individual `SchemaFragment`. The project loader
discovers sources recursively, merges fragments, resolves imports, and preserves
source locations. It requires exactly one schema per project.

## Stage 3: Validator (`src/validator/`)

`schema.rs` implements rules V1-V16 for author-defined models, including
type/field/ID integrity, references and composition targets, heading and body
requirements, and modifier placement. See [validation.md](validation.md).

## Stage 4: Composer and build (`src/codegen/compose.rs`, `src/project/build.rs`)

The composer renders one schema entity and its composed references as
Markdown, with citation and heading-numbering options. Project builds read a
`vdl.toml` manifest, validate output destinations, render all targets
(Markdown per root plus any `[[build.graph]]` artifacts from `src/graph/`),
and only then write outputs. Cycles encountered during composition are errors.

## Stage 5: Graph and search (`src/graph/`)

`graph::Graph::build` turns the validated document into nodes (entities in
loader order) and edges (reference-field targets in authored order), entirely
from the user-declared schema. Renderers serialize it: `graph::json` (pretty
JSON, no timestamp), `graph::dot` (Graphviz digraph with per-type colors
assigned by first appearance), and `graph::search` (a content index per entity
for substring search). The CLI `graph` command emits any of the three formats
and can narrow output to a one-hop neighborhood with `--root`; the `search`
command queries the index directly. `--out` reuses the guarded write used by
`compose`.

## Project Structure

```text
src/
├── main.rs            # CLI entry point
├── lib.rs             # public library API
├── error.rs           # VdlError and SourceLocation
├── lexer/             # source scanner and tokens
├── parser/            # schema document and fragment parser
├── model/             # schema types, fields, instances, values
├── project/           # discovery, manifest, guarded Markdown build
├── validator/         # schema validation rules and errors
├── codegen/compose.rs # schema model to Markdown
├── graph/             # graph build, JSON/DOT/search-index renderers
├── fsutil.rs          # file identity and path safety
└── cli/               # validate, compile, compose, graph, search
tests/
├── fixtures/schema/   # self-contained schema examples
├── fixtures/project/  # multi-file project with manifest
├── fixtures/invalid_cycle.vdl
├── cli_test.rs
├── graph_test.rs      # graph/search end-to-end over multi-file projects
├── project_test.rs
├── schema_test.rs
└── snapshot_test.rs   # schema Markdown output contract
```

## Direct dependencies

| Crate | Purpose |
|-------|---------|
| `chumsky` 0.9 | Parser combinators |
| `clap` 4.5 | CLI with derive macros |
| `serde` | Manifest deserialization |
| `serde_json` | Graph and search-index JSON output |
| `thiserror` | Error type definitions |
| `toml` 0.8 | `vdl.toml` parsing |
| `miette` 7.2 | Diagnostic reporting |
| `windows-sys` | Windows file identity and output safety |
| `tempfile` (dev) | Temporary directories in tests |

Direct dependencies removed with the legacy pipeline: `chrono`, `indexmap`,
`regex`, and the `insta` dev dependency. `serde_json` returned with the
schema-form graph and search-index output. Some removals may remain as
transitive dependencies of retained crates.
