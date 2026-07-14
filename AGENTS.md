# Agent Documentation: VDL Compiler Architecture

> **For future agents working on this codebase.**
> Last updated: July 2026 | VDL Compiler v0.1

## Quick Orientation

This is the **Voidlight Definition Language (VDL)** compiler — a Rust CLI tool and library that parses `.vdl` declarative knowledge files into a directed acyclic graph (DAG) and generates JSON, Markdown, Search Index, and Graphviz DOT output.

The compiler follows a **strict 5-stage pipeline** with well-defined boundaries between each stage. All stages are deterministic and the graph lives entirely in memory (no external database).

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────────┐
│   Lexer     │ ──→ │   Parser    │ ──→ │  Validator  │ ──→ │Graph Builder│ ──→ │  Code Generator │
│ (src/lexer) │     │(src/parser) │     │(src/validator)│    │ (src/graph) │     │ (src/codegen)   │
└─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘     └─────────────────┘
      │                    │                   │                   │                    │
      ▼                    ▼                   ▼                   ▼                    ▼
  &str source        Vec<(Token,        VdlResult<()>     KnowledgeGraph       Files on disk
  → Vec<(Token,      Span)> → Module                        → files
     Span)>
```

### Stage 1: Lexer (`src/lexer/`)

**What it does:** Byte-oriented scanner. Converts raw VDL source text into a sequence of `Token` values with span information.

**Key types:**
- `Token` (`src/lexer/token.rs`) — enum of all keywords, literals, annotations, delimiters
- `Span` (`src/lexer/mod.rs`) — byte offset + line + column

**Public API:**
```rust
pub fn lex(source: &str, file: &Path) -> VdlResult<Vec<(Token, Span)>>
```

**Design decisions:**
- Hand-written scanner (not generated) for precise error control
- Unknown bare words are hard errors (no generic Ident token)
- String literals support `"` and `\` escapes plus raw multiline
- Annotations are lexed as single tokens: `@name("value")`

### Stage 2: Parser (`src/parser/`)

**What it does:** Uses `chumsky` 0.9 parser combinators to transform tokens into an AST.

**Key types:**
- `Module`, `Entity`, `Relationship`, `EvidenceBlock`, `Revelation`, `Synthesis`, `Analogy`, `Annotation` (`src/parser/ast.rs`)
- `ParseError` (`src/parser/error.rs`)

**Public API:**
```rust
pub fn parse(tokens: &[(Token, Span)], source: &str, file: &Path) -> VdlResult<Module>
```

**Design decisions:**
- `chumsky` chosen for maintainability and rich error recovery
- Custom `Span` implements `chumsky::span::Span` trait for stream compatibility
- Relationships are flattened: `requires ["a", "b"]` → two `Relationship` structs
- Every AST node carries `SourceLocation` for error propagation

### Stage 3: Validator (`src/validator/`)

**What it does:** Runs 6 independent validation rules. Collects errors where possible.

**Public API:**
```rust
pub fn validate(module: &Module) -> VdlResult<()>
```

**Rule implementations** (`src/validator/rules.rs`):
| Function | Rule |
|----------|------|
| `check_type_constraints` | Entity-type mandatory relationships |
| `check_reference_integrity` | All `target_id`s resolve to declared entities |
| `check_evidence_presence` | Non-artifacts must have evidence |
| `check_evidence_completeness` | Required sub-fields present per evidence type |
| `check_dag` | No cycles in `requires`/`derives_from` via DFS |
| `check_version_format` | Matches `^\d+\.\d+$` |

**Design decisions:**
- Each rule returns `Vec<ValidationError>` independently
- `ValidationError` implements `From<ValidationError> for VdlError`
- DAG check uses iterative DFS with GRAY/BLACK coloring
- Cycle messages report the full chain: `a → b → c → a`

### Stage 4: Graph Builder (`src/graph/`)

**What it does:** Constructs an in-memory directed graph from the validated AST.

**Key types:**
- `KnowledgeGraph` — nodes (IndexMap), edges (Vec), adjacency list
- `Edge` — from, to, rel_type
- `GraphError` — EntityNotFound, Cycle
- `GraphQuery` — query interface with `ancestors()`, `descendants()`, `related()`

**Public API:**
```rust
impl KnowledgeGraph {
    pub fn from_module(module: &Module) -> Result<Self, GraphError>
}

impl<'a> GraphQuery<'a> {
    pub fn ancestors(&self, entity_id: &str) -> Result<Vec<&Entity>, GraphError>
    pub fn descendants(&self, entity_id: &str) -> Result<Vec<&Entity>, GraphError>
    pub fn related(&self, entity_id: &str, rel_type: RelationshipType) -> Result<Vec<&Entity>, GraphError>
}
```

**Design decisions:**
- `IndexMap` preserves insertion order for deterministic output
- `from_module` also validates cycles (defense in depth)
- Queries use BFS with `visited` sets — never panic on missing nodes

### Stage 5: Code Generator (`src/codegen/`)

**What it does:** Generates four output targets from the knowledge graph.

| Module | Output | Key behavior |
|--------|--------|-------------|
| `json.rs` | `graph.json` | serde-structured; `compiled_at` timestamp; `evidence` omitted when None |
| `markdown.rs` | `soul/*.md` | YAML frontmatter + evidence sections; filename = `id.replace(['.', '_'], "-") + ".md"` |
| `search.rs` | `search.json` | Flat array; `content_text` concatenates all text; `tags` from annotations + type |
| `dot.rs` | `graph.dot` | Graphviz `digraph`; color-coded nodes; labeled edges |

**Design decisions:**
- Each generator is independent — they only read `KnowledgeGraph`
- JSON uses internal serde structs, not direct AST serialization
- Markdown frontmatter maps `@author`, `@created`, `@status`, `@pillar` annotations

## CLI (`src/cli/`)

**Entry point:** `main.rs` → `vdl::cli::run()`

**Commands:**
| Command | Pipeline stages | Notes |
|---------|----------------|-------|
| `validate` | 1-3 | Prints success count or rich miette diagnostics |
| `compile` | 1-5 | Creates `output/` directory with all 4 targets |
| `graph` | 1-5 | Emits 1-hop subgraph DOT to stdout |
| `diff` | 1-5 | v0.1 simplified: diffs first 2 matching entities |
| `search` | 1-5 | Case-insensitive regex on IDs and titles |

**Pipeline helper:** `parse_and_validate(files: &[PathBuf]) -> VdlResult<Module>`
- Reads each `.vdl` file
- Lexes → parses (per file)
- Merges all entities into single `Module`
- Validates merged module

## Error Handling Strategy

All errors flow through `VdlError` (`src/error.rs`):

```
LexerError    → VdlError::Lexer    { location, message }
ParseError    → VdlError::Parser    { location, message }
ValidationError → VdlError::Validation { location, message }
GraphError    → VdlError::Graph    { message }
CodegenError  → VdlError::Codegen  { message }
IOError       → VdlError::Io       { message }
```

**Every error variant carries source location** (file, line, column). The CLI converts `VdlError` to `miette::Report` for rich diagnostics with `[file:line:col]` prefixes.

## Testing Strategy

### Unit Tests
Each module has a `#[cfg(test)]` module testing its own boundaries:
- **Lexer:** Keyword coverage, string escapes, annotations, comments, error cases
- **Parser:** Simple entity, all relationships, full evidence, annotations, error cases
- **Validator:** Each rule tested independently
- **Graph:** Cycle detection, query correctness
- **Codegens:** Output structure verification

### Integration Tests (`tests/`)
- `cli_test.rs` — Spawns binary via `cargo run` for end-to-end validation
- `snapshot_test.rs` — Uses `insta` to snapshot JSON, Markdown, Search, and DOT outputs

### Fixtures (`tests/fixtures/`)
| Fixture | Purpose |
|---------|---------|
| `seven_laws.vdl` | Complete valid VDL: 1 framework + 7 laws with derives_from chains |
| `divine_alignment.vdl` | Framework with all relationship types + full evidence block + supporting concepts |
| `invalid_cycle.vdl` | Intentionally invalid: circular `requires` chain for DAG testing |

## Adding a New Validation Rule

1. Add a new function in `src/validator/rules.rs`:
   ```rust
   pub fn check_my_rule(module: &Module) -> Vec<ValidationError> { ... }
   ```
2. Add a `ValidationError` variant in `src/validator/error.rs` if needed
3. Update `From<ValidationError> for VdlError`
4. Call the new rule in `src/validator/mod.rs::validate()`
5. Add unit tests in `src/validator/mod.rs` tests module

## Adding a New Output Target

1. Create `src/codegen/<target>.rs`
2. Implement `generate(graph: &KnowledgeGraph, output_path: &Path) -> VdlResult<()>`
3. Export in `src/codegen/mod.rs`
4. Wire into CLI in `src/cli/mod.rs::run()` under `Commands::Compile`
5. Add unit tests and snapshot test

## Parser Choice Rationale

We chose **`chumsky`** over `nom` or `pest` because:
- **Error recovery:** Built-in support for labeled errors and recovery parsing
- **Maintainability:** Combinator-based approach is readable and refactorable
- **DSL fit:** VDL's block-structured grammar maps naturally to chumsky's `delimited_by`, `repeated`, `choice`
- **Trade-off:** Slightly slower than `nom` or a generated parser, but negligible for VDL file sizes

## Known Limitations (v0.1)

- **No incremental compilation** — full recompile every run
- **No plugin system** — output targets are hardcoded
- **String values only** — properties are `HashMap<String, String>`
- **In-memory graph only** — no external database
- **CLI `diff` is simplified** — no true version history; compares by substring match
- **No WASM target** — CLI binary only

## File Inventory

| File | Lines (approx) | Owner Agent | Status |
|------|---------------|-------------|--------|
| `src/lexer/mod.rs` | 694 | Lexer_Agent | ✅ Complete |
| `src/parser/mod.rs` | 641 | Parser_Agent | ✅ Complete |
| `src/validator/rules.rs` | 637 | Validator_Agent | ✅ Complete |
| `src/validator/mod.rs` | 276 | Validator_Agent | ✅ Complete |
| `src/graph/node.rs` | 134 | GraphBuilder_Agent | ✅ Complete |
| `src/graph/query.rs` | 152 | GraphBuilder_Agent | ✅ Complete |
| `src/codegen/json.rs` | 417 | Codegen_JSON_Agent | ✅ Complete |
| `src/codegen/markdown.rs` | 457 | Codegen_Markdown_Agent | ✅ Complete |
| `src/codegen/search.rs` | 361 | Codegen_Search_Agent | ✅ Complete |
| `src/codegen/dot.rs` | 280 | Codegen_DOT_Agent | ✅ Complete |
| `src/cli/mod.rs` | 416 | CLI_Agent | ✅ Complete |

## How to Regenerate This Document

If you modify the architecture significantly, update this file to reflect:
1. New pipeline stages or changed data flow
2. New public API functions
3. New validation rules or codegen targets
4. Changed design decisions and their rationale
