# VDL Compiler v0.1

> **Voidlight Definition Language** — A standalone Rust compiler that transforms declarative knowledge definitions into JSON Graph, Markdown, Search Index, and Graphviz DOT output.

## Overview

VDL is a domain-specific language for describing, structuring, and interconnecting knowledge systems. The compiler follows a strict 5-stage pipeline:

```
VDL Source (.vdl) → Lexer → Parser → Validator → Graph Builder → Code Generator
                                                          ↓
                                              JSON | Markdown | Search | DOT
```

## Build Instructions

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2021 or later)
- `cargo` package manager

### Building

```bash
# Clone the repository
git clone https://github.com/voidlight-labs/vdl
cd vdl

# Build in release mode
cargo build --release

# The binary will be at target/release/vdl
./target/release/vdl --help
```

### Running Tests

```bash
# Run all tests including snapshot tests
cargo test

# Run only unit tests
cargo test --lib

# Update snapshots (if output format changes)
cargo insta review
```

## Grammar Reference (v0.1)

### Entity Declaration

```vdll
<type> "<identifier>" {
    version "<major.minor>"
    title "<human-readable name>"
    description "<brief summary>"

    // Relationships (optional, type-dependent)
    requires [ "<id.a>", "<id.b>" ]
    enables [ "<id.c>" ]
    references [ "<id.d>" ]
    based_on [ "<id.e>" ]
    derives_from [ "<id.f>" ]
    implements [ "<id.g>" ]
    inspired_by [ "<id.h>" ]
    evolved_from [ "<id.i>" ]
    contradicts [ "<id.j>" ]

    // Evidence block (mandatory for non-artifact)
    evidence {
        revelation {
            source "<Quran X:Y or Hadith reference>"
            text "<exact quotation>"
            translator "<optional>"
        }
        synthesis {
            sources [ "<Source A>", "<Source B>" ]
            argument "<scholarly reasoning>"
        }
        analogy {
            domain "<source domain>"
            mapping "<cross-domain explanation>"
        }
    }
}
```

### Entity Types

| Type | Constraint |
|------|-----------|
| `axiom` | `requires` must be empty |
| `framework` | Must have `based_on` |
| `law` | Must have `derives_from` |
| `principle` | Must have `derives_from` |
| `concept` | No mandatory relationships |
| `artifact` | Must reference at least one `law` or `principle` |
| `pillar`, `document`, `project`, `release`, `persona`, `collection` | Parseable; basic rules only in v0.1 |

### Annotations

```vdll
@author("Name")
@created("YYYY-MM-DD")
@reviewed("YYYY-MM-DD")
@status("canonical" | "draft" | "deprecated")
@pillar("soul" | "vision" | "labs" | "music")
```

### Version Format

`major.minor` — e.g., `"5.0"`, `"1.10"`. Must match `^\d+\.\d+$`.

## CLI Usage

### `vdl validate <file_or_dir>`

Check all `.vdl` files and report errors with precise file/line/column locations.

```bash
# Validate a single file
vdl validate tests/fixtures/seven_laws.vdl
# → ✓ Validated 1 files, 8 entities. No errors.

# Validate a directory
vdl validate tests/fixtures/

# Validate an invalid file (returns exit code 1)
vdl validate tests/fixtures/invalid_cycle.vdl
# → [test.vdl:5:5] Validation error: Cycle detected: cycle.a → cycle.c → cycle.b → cycle.a
```

### `vdl compile <file_or_dir>`

Run the full pipeline and generate all output targets.

```bash
vdl compile tests/fixtures/seven_laws.vdl
# → Found 1 VDL file(s) to compile.
# → ✓ Validation and graph construction complete.
# → Generating output targets…
# →   output/graph.json
# →   output/soul/
# →   output/search.json
# →   output/graph.dot
# → ✓ Compilation complete.
```

Output structure:
```
output/
├── graph.json      # JSON Graph (entities + relationships + metadata)
├── soul/           # Markdown files with YAML frontmatter
│   ├── soul-law-i.md
│   └── ...
├── search.json     # Flat search index array
└── graph.dot       # Graphviz DOT visualization
```

### `vdl graph <entity_id>`

Export a subgraph centered on an entity to DOT format (stdout).

```bash
vdl graph soul.law.i
# → digraph subgraph { ... }
```

### `vdl diff <id> <v1> <v2>`

Compare two entities. In v0.1, searches for entities whose ID contains the query substring and diffs the first two matches.

```bash
vdl diff soul law
# → Diff: soul.law.i vs soul.law.ii
```

### `vdl search <query>`

Search entity IDs and titles with a case-insensitive regex.

```bash
vdl search autonomy
# → soul.law.i | law | Autonomy Is Mandatory
```

## Architecture Overview

### Stage 1: Lexer (`src/lexer/`)

Hand-written byte-oriented scanner. Tokenizes VDL source into a stream of `Token` values with `Span` information (byte offset, line, column). Handles all keywords, string literals (with escapes and multiline support), annotations `@name("value")`, delimiters, and both line (`//`) and block (`/* */`) comments.

### Stage 2: Parser (`src/parser/`)

Built on [`chumsky`](https://github.com/zesterer/chumsky) 0.9 — a parser combinator library chosen for excellent error recovery and maintainability. Transforms the token stream into an AST where every node carries a `SourceLocation` for precise error reporting.

### Stage 3: Validator (`src/validator/`)

Six independent rule checks run in sequence:
1. **Type constraints** — enforce mandatory relationships per entity type
2. **Reference integrity** — all relationship targets must resolve
3. **Evidence presence** — non-artifacts must have evidence
4. **Evidence completeness** — required sub-fields present
5. **DAG validation** — DFS with GRAY/BLACK coloring detects cycles
6. **Version format** — regex `^\d+\.\d+$`

### Stage 4: Graph Builder (`src/graph/`)

Constructs an in-memory directed knowledge graph:
- **Nodes**: `IndexMap<String, Entity>` for deterministic ordering
- **Edges**: Flat `Vec<Edge>` plus adjacency list for O(1) lookups
- **Queries**: `ancestors()`, `descendants()`, `related()` via BFS

### Stage 5: Code Generator (`src/codegen/`)

Four output targets:
- **JSON** (`json.rs`) — `serde`-serialized graph with metadata
- **Markdown** (`markdown.rs`) — YAML frontmatter + evidence sections
- **Search Index** (`search.rs`) — flat array with `content_text` and `tags`
- **DOT** (`dot.rs`) — Graphviz with color-coded nodes per entity type

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
│   │   ├── mod.rs       # parse() with chumsky
│   │   ├── ast.rs       # AST types (Entity, Relationship, Evidence, …)
│   │   └── error.rs     # ParseError
│   ├── validator/
│   │   ├── mod.rs       # validate() orchestration
│   │   ├── rules.rs     # 6 rule implementations
│   │   └── error.rs     # ValidationError
│   ├── graph/
│   │   ├── mod.rs
│   │   ├── node.rs      # KnowledgeGraph, Edge
│   │   └── query.rs     # GraphQuery (ancestors, descendants, related)
│   ├── codegen/
│   │   ├── mod.rs
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
│   │   └── invalid_cycle.vdl
│   ├── snapshot/        # insta snapshot files
│   ├── cli_test.rs      # Integration tests via cargo run
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
| `miette` 7.2 | Rich diagnostic reporting |
| `indexmap` | Deterministic HashMap |
| `regex` | Search query matching |
| `chrono` | Timestamps in JSON metadata |
| `insta` (dev) | Snapshot testing |
| `tempfile` (dev) | Temporary directories in tests |

## License

MIT — See [LICENSE](LICENSE) for details.

## Acknowledgments

Built for the **Voidlight** knowledge operating system. The Seven Laws and Divine Alignment framework are original works by Khayren.
