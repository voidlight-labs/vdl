# VDL Compiler v0.1

> **Voidlight Definition Language** — A standalone Rust compiler that transforms declarative knowledge definitions into JSON Graph, Markdown, Search Index, and Graphviz DOT output.

## Quick Start

```bash
# 1. Install
git clone https://github.com/voidlight-labs/vdl
cd vdl
cargo install --path .

# 2. Validate a VDL file
vdl validate tests/fixtures/seven_laws.vdl

# 3. Compile to all output formats
vdl compile tests/fixtures/seven_laws.vdl
# Output appears in ./output/
```

## Overview

VDL is a domain-specific language for describing, structuring, and interconnecting knowledge systems. The compiler follows a strict 5-stage pipeline:

```
VDL Source (.vdl) → Lexer → Parser → Validator → Graph Builder → Code Generator
                                                          ↓
                                              JSON | Markdown | Search | DOT
```

## Installation

### Prerequisites

- [Rust](https://rustup.rs/) toolchain (edition 2021 or later)
- `cargo` package manager

### Option 1: Install globally with `cargo install` (recommended)

This is the easiest way to use `vdl` from anywhere on your system.

```bash
# Clone the repository
git clone https://github.com/voidlight-labs/vdl
cd vdl

# Install the release binary to ~/.cargo/bin
cargo install --path .

# Now you can run `vdl` from any directory
vdl --help
```

> **Note:** Make sure `~/.cargo/bin` is in your `PATH`. If you installed Rust via `rustup`, this is usually already configured.

### Option 2: Run without installing

If you don't want to install globally, you can build and run directly:

```bash
# Build in release mode
cargo build --release

# Run the binary
./target/release/vdl --help
```

### Option 3: Copy the binary to a system directory

For system-wide access (requires `sudo` on Linux/macOS):

```bash
cargo build --release
sudo cp target/release/vdl /usr/local/bin/
vdl --help
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

```vdl
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

```vdl
@author("Name")
@created("YYYY-MM-DD")
@reviewed("YYYY-MM-DD")
@status("canonical" | "draft" | "deprecated")
@pillar("soul" | "vision" | "labs" | "music")
```

### Version Format

`major.minor` — e.g., `"5.0"`, `"1.10"`. Must match `^\d+\.\d+$`.

## Schema Documents (author-defined model)

Everything above describes the legacy v0.1 form: fixed entity types, evidence
blocks, one compiler rule set. A schema document is a second, self-describing
form. It declares its own entity types and instantiates them, and the compiler
reads every decision from that schema. No type name, field name, heading field,
or composition target is known to the compiler ahead of time.

### Document shape

One file holds exactly one `schema` block followed by its `entity` instances.
Dispatch is positional: a file whose first non-annotation token is `schema` or
`entity` is a schema document; anything else stays on the legacy path. Zero
schema blocks, two schema blocks, or entities without a schema block are parse
errors. Cross-file merging is out of scope.

```vdl
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "chapters" ref "Chapter" compose
        field "see_also" ref "Chapter"
    }

    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}

entity "Chapter" "intro" {
    field "label" "Introduction"
    field "content" """
    Prose starts here, no heading line.

    ### Internal heading stays as written
    """
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "chapters" [ "intro" ]
    field "see_also" [ "appendix" ]
}
```

### Field kinds

| Declaration | Value form | Meaning |
|-------------|------------|---------|
| `field "<name>" text` | `"..."` | Single-line quoted string |
| `field "<name>" body` | `"""..."""` | Multiline raw Markdown, rendered verbatim |
| `field "<name>" ref "<Type>"` | `[ "id", "id" ]` | Ordered collection of entity IDs of type `<Type>` |

Every `ref` is a collection; there are no scalar references in this slice. All
declared fields are required; optional fields are not supported yet.

### Modifiers

- `heading` — `text` fields only, at most one per type. It names the field used
  as that type's heading. A root entity renders it as `# ...`; a composed
  reference renders the target's heading field as `## ...`. No heading is ever
  inferred from a field name.
- `compose` — `ref` fields only. Marks the collection for document composition.
  Multiple composed fields render in declaration order, and each list renders
  in author order (never sorted).

A composed collection's target type must declare a heading field; this is
validated, not guessed.

### Raw body literal rules

Bodies are literal, with exactly five permitted transformations. Fidelity means
byte-identical after these:

1. `\r\n` and lone `\r` are normalized to `\n`.
2. No escape processing: `\"`, `\\` and friends stay as written.
3. One newline immediately after the opening `"""` is dropped.
4. A trailing whitespace-only line before the closing `"""` is dropped.
5. The longest common leading whitespace prefix across non-blank lines is
   stripped, so an indented source block does not become a Markdown code block.

Blank lines and relative indentation are preserved exactly. Non-ASCII text is
decoded as UTF-8 and survives byte-identical.

### Validation rules

| Rule | Rejects |
|------|---------|
| V1 | instance of an undeclared type |
| V2 | duplicate entity ID |
| V3 | duplicate type name |
| V4 | duplicate field name inside one type |
| V5 | field supplied that its type does not declare |
| V6 | declared field not supplied |
| V7 | value shape does not match the declared field kind |
| V8 | reference to an undeclared entity ID |
| V9 | reference whose target entity has a different type |
| V10 | `compose` on a non-`ref` field |
| V11 | `heading` on a non-`text` field, or more than one `heading` per type |
| V12 | composed collection whose target type declares no `heading` field |
| V13 | `ref` field whose declared target type is undeclared, empty list included |
| V14 | the same field supplied twice in one instance |
| V15 | more than one `body` field in a type, even with no instances |

Every diagnostic carries `file:line:column` and names the offending declaration
or reference. All violations are collected, not short-circuited.

### Composition output

`vdl compose <file> --root <entity-id>` renders one Markdown document:

```
---
id: "core-guide"
type: "Runbook"
label: "Core Guide"
see_also:
  - "appendix"
---

# Core Guide

## Introduction

<chapter body verbatim>

## Rules

<chapter body verbatim>
```

Frontmatter lists `id`, `type`, the root type's `text` fields in declaration
order, then non-composed `ref` collections as YAML lists. `id`, type names, text
values, and reference IDs are emitted as double-quoted YAML strings, so values
like `true`, `null`, or `001` survive a YAML parser as strings. Author-defined
keys are quoted whenever they would be ambiguous.

`id` and `type` are compiler-owned keys. A root field that would be emitted
under either name is a composition error, reported before any output is
written.

Composed collections expand one level deep: each referenced entity contributes
its heading and body. An ordinary reference contributes only its ID, never the
target's body.

`--out` refuses to write over the input file. The check compares canonical paths
(so relative segments and symlinks are caught) and, on Unix, device/inode
identity (so hard links are caught too). On platforms where the standard library
exposes no file identity, a hard-link alias is not detected.

## CLI Usage

> All commands that take a `<path>` argument accept either a single `.vdl` file or a directory. Directories are scanned recursively for `.vdl` files.

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
# → Validation failed with 1 error(s):
# → [tests/fixtures/invalid_cycle.vdl:5:5] Validation error: Cycle detected: ...
```

### `vdl compile <file_or_dir>`

Run the full pipeline and generate all output targets in the `output/` directory.

```bash
vdl compile tests/fixtures/seven_laws.vdl
# → Found 1 VDL file(s) to compile.
# → ✓ Validation and graph construction complete.
# → Generating output targets…
# →   → output/graph.json
# →   → output/soul/
# →   → output/search.json
# →   → output/graph.dot
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

### `vdl graph <entity_id> [path]`

Export a 1-hop subgraph centered on an entity to DOT format (stdout).

```bash
# Use .vdl files in the current directory (default)
vdl graph soul.law.i

# Or specify a file/directory
vdl graph soul.law.i tests/fixtures/
# → digraph subgraph { ... }
```

### `vdl diff <id> <v1> <v2> [path]`

Compare two entities. In v0.1, searches for entities whose ID contains the query substring and diffs the first two matches.

```bash
vdl diff soul law
# → Diff: soul.law.i vs soul.law.ii
```

### `vdl search <query> [path]`

Search entity IDs and titles with a case-insensitive regex.

```bash
vdl search autonomy
# → soul.law.i | law | Autonomy Is Mandatory
```

### `vdl compose <file> --root <entity_id> [--out <file>]`

Render a schema document into one complete Markdown file. Requires a schema
document (one `schema` block); legacy entity files are rejected. Prints to
stdout when `--out` is omitted.

```bash
vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide
vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide --out core-guide.md
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
│   │   └── ast.rs       # AST types (Entity, Relationship, Evidence, …)
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
