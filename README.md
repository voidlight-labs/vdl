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

# 3. Compile a legacy document to all output formats
vdl compile tests/fixtures/seven_laws.vdl
# Output appears in ./output/

# 4. Build a schema project using vdl.toml
vdl compile tests/fixtures/project
# Markdown targets appear under the project’s dist/ directory
```

## Overview

VDL is a domain-specific language for describing, structuring, and interconnecting knowledge systems. Legacy documents follow this pipeline:

```
VDL Source (.vdl) → Lexer → Parser → Validator → Graph Builder → Code Generator
                                                          ↓
                                              JSON | Markdown | Search | DOT
```

Schema projects load nested sources, resolve file-local imports, validate a
project-wide model, and render the Markdown targets declared in `vdl.toml`.

Zed syntax highlighting, including embedded Markdown, is available through
the [local VDL extension](editors/zed/README.md).

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

One file can hold a `schema` block followed by its instances. A schema instance
can use either a bare type name or `entity "<type>"`. Its fields can use either
bare names or `field "<name>"`. Both forms build the same model. Bare names
are case-sensitive and match `[A-Za-z_][A-Za-z0-9_]*`. Use the quoted form for
names with punctuation or spaces, or for type names that conflict with the
`schema`, `entity`, and `package` declarations.

In a project, a bare type name declared in the schema selects the schema
grammar even if it also starts a legacy declaration, such as `framework`
or `artifact`. Standalone schema documents start with their schema block or a package header.
Existing legacy sources remain supported.

Two layouts are supported:

- **Single file** — one `.vdl` file contains the schema and all instances.
  `vdl compose <file>` and `vdl validate <file>` keep this contract: exactly one
  schema block, zero or two are parse errors.
- **Project** — a directory of `.vdl` files. Exactly one file declares the
  schema; every other file may declare only instances, anywhere in the tree.
  See [Schema Projects](#schema-projects-multi-file-builds).

`schema.vdl` is only a convention: no file name or folder is special.

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

Chapter "intro" {
    label "Introduction"
    content """
    Prose starts here, no heading line.

    ### Internal heading stays as written
    """
}

Runbook "core-guide" {
    label "Core Guide"
    chapters ["intro"]
    see_also ["appendix"]
}
```

### Packages and entity names

Declare a package at the start of a schema source file, after any leading
annotations and before any schema or instance declarations.

```vdl
package soul.alignment

artifact sleep-and-death {
    title "Sleep and Death"
    body {
        Prose starts here.
    }
}
```

The ID is `soul.alignment.sleep-and-death`. Entity declarations and reference
lists use the same naming rules:

- A bare short name is prefixed with the file's package, if declared.
- A bare dotted name is a full ID. Use it for references to other packages.
- A quoted ID is used as written. It retains the meaning it had before packages.

Bare entity names and packages use dot-separated segments of ASCII letters,
digits, underscores, and internal hyphens. Package names and entity IDs are
case-sensitive. Moving a file between directories leaves its package and IDs
intact. Manifest roots and `vdl compose --root` use the full ID.

Files may import one entity with an optional alias, or direct children of a
package with a one-level wildcard:

```vdl
import soul.human-os.*
import soul.revelation.quran.55.33 as ar-rahman
```

Bare references resolve in this order: explicit imports (including aliases),
the file's package, then wildcard imports. Ambiguous names report the candidate
IDs. Without a package, a bare name can fall back to a global ID. Quoted IDs and dotted IDs keep their literal meaning.
Import declarations are file-local, so moving a source file does not change
its imports or entity IDs.

### Field kinds

| Declaration | Value form | Meaning |
|-------------|------------|---------|
| `field "<name>" text` | `"..."` | Single-line quoted string |
| `field "<name>" body` | Multiline `{ … }` or `"""..."""` | Raw Markdown |
| `field "<name>" ref "<Type>"` | `[ "id", "id" ]` | Ordered collection of entity IDs of type `<Type>` |
| `field "<name>" ref ["<TypeA>", "<TypeB>"]` | `[ "id", "id" ]` | Ordered collection whose IDs can have any listed type |

Every `ref` is a collection. Declared fields are required by default.

### Modifiers

- `heading` — `text` fields only, at most one per type. It names the field used
  as that type's heading. A root entity renders it as `# ...`. Each composed
  reference increases the heading level by one. No heading is inferred from a
  field name.
- `compose` on a `ref` field marks the collection for document composition.
  Multiple composed fields render in declaration order, and each list renders
  in author order (never sorted).
- `compose ["Type"]` after a `body` kind opts that braced body into heading
  composition. A standalone `include <id>` line inserts the referenced entity
  at that position, including its heading and body. Includes follow local
  package and import resolution, target types are checked, and normal depth,
  cycle, and heading-number rules apply. Fenced code stays literal; triple-
  quoted bodies never interpret include lines. Body directives render before
  any composed reference fields on the same entity.
- `inline` — `ref` fields only. Emits each target's body in list order, without
  a heading, frontmatter, or nested composition. The list can also contain
  `prose { … }` literals between references. Inline entries do not consume
  heading depth or heading numbers. Each referenced target must supply its body.
- `optional` — permits an instance to omit the field. A supplied value still
  follows the declared field kind and reference rules.

A heading-composed target type must declare a heading field. An inline target
type must declare a body field.

A type can set a rendered body label, for example
`type "Example" label "[EXAMPLE]" { … }`. Markdown emits that label in bold
on its own line before the body, including when embedded through `inline`.

### Raw body literal rules

A braced body starts on the line after the opening brace. Close it with a
standalone `}` at the same indentation as the field. For a prose item in a
list, a comma may follow the closing brace. Other braces are Markdown content.

```vdl
artifact "soul.alignment.sleep-and-death" {
    title "Sleep and Death"
    body {
        Prose starts here.
    }
}
```

Braced bodies work for schema-defined body fields. Use the explicit
`field "evidence" {` spelling for a schema field named `evidence`, since bare
`evidence {` opens a legacy evidence block. Triple-quoted bodies still work.

Bodies are literal, with exactly five permitted transformations. Fidelity means
byte-identical after these:

1. `\r\n` and lone `\r` are normalized to `\n`.
2. No escape processing: `\"`, `\\` and friends stay as written.
3. One newline immediately after the opening delimiter is dropped.
4. A trailing whitespace-only line before the closing delimiter is dropped.
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
| V6 | required field not supplied |
| V7 | value shape does not match the declared field kind |
| V8 | reference to an undeclared entity ID |
| V9 | reference whose target entity has a different type |
| V10 | Composition on an unsupported field kind |
| V11 | `heading` on a non-`text` field, or more than one `heading` per type |
| V12 | composed target lacks the heading or body required by the composition mode |
| V13 | A declared target type is missing, even when no instance references it |
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
"label": "Core Guide"
"see_also":
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
values, reference IDs, and author-defined keys are emitted as double-quoted
YAML strings. Escapes preserve control characters and Unicode line separators
through YAML parsing.

`id` and `type` are compiler-owned keys. A root field that would be emitted
under either name is a composition error, reported before any output is
written.

Composed collections expand recursively. Each referenced entity contributes its
heading and body, then its own composed collections. Nested headings increment
one level at a time, up to Markdown's six heading levels. Composition cycles
are errors. An ordinary reference contributes only its ID in root frontmatter.

`--out` compares the open input and output files before truncating the output.
On Unix it compares device and inode, and on Windows it compares volume serial
number and file ID. Both checks catch hard links and symlinks. Other platforms
compare canonical paths.

## Schema Projects (multi-file builds)

A project is any directory containing `.vdl` files with exactly one `schema`
block. Sources are discovered recursively, including nested folders. Entity IDs
are project-global: a file's name and location never affect resolution, and
moving a source file does not change rendered bytes.

```
project/
├── vdl.toml
├── schema.vdl
├── runbooks/
│   ├── deploy.vdl
│   └── recovery.vdl
└── sections/
    └── nested/
        └── intro.vdl
```

`vdl validate <dir>` and `vdl compose <dir> --root <id>` merge the project
without requiring a manifest. `vdl compile <dir>` uses the manifest:

```toml
[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/deploy.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/recovery.md"
```

`root` is an entity ID. `out_dir` is relative to the project root, and each
`output` is relative to `out_dir`. Types, fields, relationships, and the
`heading`/`body`/`compose` rules stay in `.vdl`; the manifest only lists build
targets.

Diagnostics across the project:

- exactly one schema block in the whole project; zero or several report every
  location involved;
- a project that mixes schema documents with legacy entity documents is
  rejected, naming the legacy files;
- missing and wrong-type references point at the instance that uses them, in
  whichever file or folder it lives;
- duplicate entity IDs name both the duplicate and the first declaration.

### Heading numbering

Set `number_headings = true` in a `[[build.markdown]]` entry to number that
output's composed headings. The root title stays unnumbered. Its first child
renders as `## 1. Title`; that child's second child renders as
`### 1.2 Title`. Each composed level adds one segment, through H6.
Sibling numbering follows body directives first and composed reference fields
afterward.

The default is `false`. The setting does not affect `vdl compose`, frontmatter,
entity IDs, ordinary references, or Markdown headings written inside a body.

### Build safety

All targets are validated and every Markdown document is rendered in memory
before anything is written. A build fails without touching existing output when
a root is unknown, a destination is a directory, targets are equal or one target
would need to be both a file and a directory, a path leaves `out_dir`, or a
destination aliases a project source or the manifest. Paths are compared after
resolution, so symlinked parents and hard links are covered. The resolved
`out_dir` must remain inside the resolved project root; symlinks that stay inside
are allowed, while symlinks that escape are rejected. Outputs whose effective
filename ends in `.vdl` are rejected so a later build cannot parse generated
Markdown as source. These checks apply before any output is written.

`compose --out` applies the same protection to every source in the project and
to `vdl.toml`, before truncating the output.

## CLI Usage

> All commands that take a `<path>` argument accept either a single `.vdl` file or a directory. Directories are scanned recursively for `.vdl` files.

### `vdl validate <file_or_dir>`

Check all `.vdl` files and report errors with precise file/line/column locations.
A directory that contains schema documents is validated as one project, even
without `vdl.toml`; directories of legacy documents keep the per-file behavior.

```bash
# Validate a single file
vdl validate tests/fixtures/seven_laws.vdl
# → ✓ Validated 1 files, 8 entities. No errors.

# Validate a project directory (schema + instance files)
vdl validate path/to/project/

# Validate a directory of legacy documents
vdl validate path/to/legacy/

# Validate an invalid file (returns exit code 1)
vdl validate tests/fixtures/invalid_cycle.vdl
# → Validation failed with 1 error(s):
# → [tests/fixtures/invalid_cycle.vdl:5:5] Validation error: Cycle detected: ...
```

### `vdl compile <file_or_dir>`

A directory containing `vdl.toml` is a schema project: every registered
Markdown target is validated and rendered before the first byte is written.
A directory without a manifest uses the legacy pipeline below.

```bash
# Schema project
vdl compile path/to/project/
# →   → path/to/project/dist/runbooks/deploy.md
# →   → path/to/project/dist/runbooks/recovery.md
# → ✓ Compilation complete.

# Legacy pipeline
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

The legacy output structure is:
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

### `vdl compose <file_or_dir> --root <entity_id> [--out <file>]`

Render one entity into a complete Markdown file. The input may be one `.vdl`
schema document or a project directory; a directory is merged from all its
sources and needs no manifest. Legacy entity files are rejected. Prints to
stdout when `--out` is omitted.

```bash
vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide
vdl compose path/to/project/ --root deploy --out deploy.md
```

`--out` refuses to truncate any project source or `vdl.toml`, including aliases
reached through symlinks or hard links.

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
│   │   ├── schema.rs    # 15 schema rules
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

## License

MIT — See [LICENSE](LICENSE) for details.

## Acknowledgments

Built for the **Voidlight** knowledge operating system. The Seven Laws and Divine Alignment framework are original works by Khayren.
