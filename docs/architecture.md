# Architecture Overview

```
VDL Source (.vdl) → Lexer → Parser → Validator → Graph Builder → Code Generator
                                                          ↓
                                              JSON | Markdown | Search | DOT
```

Schema projects load nested sources, resolve file-local imports, validate a
project-wide model, and render the Markdown targets declared in `vdl.toml`.

## Stage 1: Lexer (`src/lexer/`)

Hand-written byte-oriented scanner. Tokenizes VDL source into a stream of
`Token` values with `Span` information (byte offset, line, column). Handles
all keywords, string literals (with escapes and multiline support),
annotations `@name("value")`, delimiters, and both line (`//`) and block
(`/* */`) comments.

## Stage 2: Parser (`src/parser/`)

Built on [chumsky](https://github.com/zesterer/chumsky) 0.9 — a parser
combinator library chosen for excellent error recovery and maintainability.
Transforms the token stream into an AST where every node carries a
`SourceLocation` for precise error reporting. `parser/schema.rs` implements
the schema document / fragment parser.

## Stage 3: Validator (`src/validator/`)

Two rule sets:

- **Legacy rules** (`rules.rs`) — six checks for the v0.1 fixed-type form:
  type constraints, reference integrity, evidence presence and
  completeness, DAG cycle detection, version format.
- **Schema rules** (`schema.rs`) — sixteen checks (V1-V16) for
  author-defined models: type/field/ID integrity, reference and composition
  targets, heading/body requirements, duplicate detection, and the `cite`
  modifier placement.

See [validation.md](validation.md) for the full tables.

## Stage 4: Graph Builder (`src/graph/`)

Constructs an in-memory directed knowledge graph:

- **Nodes**: `IndexMap<String, Entity>` for deterministic ordering
- **Edges**: Flat `Vec<Edge>` plus adjacency list for O(1) lookups
- **Queries**: `ancestors()`, `descendants()`, `related()` via BFS

## Stage 5: Code Generator (`src/codegen/`)

Output targets:

- **JSON** (`json.rs`) — `serde`-serialized graph with metadata
- **Markdown** (`markdown.rs`) — YAML frontmatter + evidence sections
- **Search Index** (`search.rs`) — flat array with `content_text` and `tags`
- **DOT** (`dot.rs`) — Graphviz with color-coded nodes per entity type
- **Compose** (`compose.rs`) — schema document → composed Markdown, with
  `ComposeOptions { cites, number_headings }` and heading-aware label
  emission

## Project Structure

```
vdl/
├── Cargo.toml
├── src/
│   ├── main.rs          # CLI binary entrypoint
│   ├── lib.rs           # Public library API
│   ├── error.rs         # VdlError, SourceLocation
│   ├── lexer/
│   │   ├── mod.rs       # lex() implementation
│   │   └── token.rs     # Token enum
│   ├── parser/
│   │   ├── mod.rs       # parse() and parse_document() with chumsky
│   │   ├── schema.rs    # schema document / fragment parser
│   │   └── ast.rs       # AST types (Entity, Relationship, Evidence, …)
│   ├── model/
│   │   └── mod.rs       # SchemaDoc, TypeDecl, Instance, Value
│   ├── project/
│   │   ├── mod.rs       # project module exports
│   │   ├── loader.rs    # recursive discovery + multi-file merge
│   │   ├── manifest.rs  # vdl.toml parsing and path checks
│   │   └── build.rs     # target validation + Markdown build
│   ├── validator/
│   │   ├── mod.rs       # validate() orchestration
│   │   ├── rules.rs     # 6 legacy rule implementations
│   │   ├── schema.rs    # 16 schema rules
│   │   └── error.rs     # ValidationError
│   ├── graph/
│   │   ├── mod.rs
│   │   ├── node.rs      # KnowledgeGraph, Edge
│   │   └── query.rs     # GraphQuery (ancestors, descendants, related)
│   ├── codegen/
│   │   ├── mod.rs
│   │   ├── compose.rs   # schema document → Markdown
│   │   ├── json.rs
│   │   ├── markdown.rs
│   │   ├── search.rs
│   │   └── dot.rs
│   └── cli/
│       ├── mod.rs       # CLI orchestration
│       └── commands.rs  # clap derive structs
├── tests/
│   ├── fixtures/
│   │   ├── seven_laws.vdl
│   │   ├── divine_alignment.vdl
│   │   ├── invalid_cycle.vdl
│   │   ├── schema/      # single-file schema fixtures
│   │   └── project/     # multi-file project fixture with vdl.toml
│   ├── snapshot/        # insta snapshot files
│   ├── cli_test.rs      # CLI integration tests via cargo run
│   ├── project_test.rs  # multi-file project and build-safety tests
│   ├── schema_test.rs   # schema document tests
│   └── snapshot_test.rs # insta snapshot tests
└── README.md
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `chumsky` 0.9 | Parser combinators |
| `clap` 4.5 | CLI with derive macros |
| `serde` + `serde_json` | JSON serialization |
| `thiserror` | Error type definitions |
| `toml` 0.8 | `vdl.toml` build manifest |
| `miette` 7.2 | Rich diagnostic reporting |
| `indexmap` | Deterministic HashMap |
| `regex` | Search query matching |
| `chrono` | Timestamps in JSON metadata |
| `insta` (dev) | Snapshot testing |
| `tempfile` (dev) | Temporary directories in tests |
