# Agent Documentation: VDL Compiler

> **For agents working on this codebase.**
> Last updated: July 2026 | VDL Compiler v0.1

## Quick Commands

```bash
# Build and run
cargo build --release
./target/release/vdl --help

# Install globally
cargo install --path .

# Test (92 unit tests + 3 CLI integration tests + 4 snapshot tests)
cargo test

# Run only unit tests
cargo test --lib

# Run specific test module
cargo test validator::tests

# Update snapshots (if output format changes)
cargo insta review

# Check without running tests
cargo check --tests
```

## Architecture

**5-stage pipeline:** Lexer → Parser → Validator → Graph Builder → Code Generator

Each stage is a separate module under `src/`. All stages are deterministic. The graph lives entirely in memory (no external database).

```
src/
├── lexer/          # Byte-oriented scanner → Token stream
├── parser/         # chumsky 0.9 combinators → AST (Module)
├── validator/      # 6 independent rules → VdlResult<()>
├── graph/          # In-memory DAG with IndexMap nodes
├── codegen/        # 4 output targets (json, markdown, search, dot)
├── cli/            # clap 4.5 commands + miette error reporting
├── error.rs        # VdlError enum with source locations
├── lib.rs          # Public library API
└── main.rs         # Binary entrypoint → cli::run()
```

## Key Design Decisions

### Error Handling
- All errors flow through `VdlError` (`src/error.rs`)
- Every variant carries `SourceLocation` (file, line, column)
- `validate()` collects all errors; returns `VdlError::ValidationErrors` if multiple, single error if one
- CLI converts `VdlError` → `miette::Report` for rich diagnostics

### Validation Rules (`src/validator/rules.rs`)
Each rule is a separate function returning `Vec<ValidationError>`:
1. `check_type_constraints` — entity-type mandatory relationships
2. `check_reference_integrity` — all `target_id`s resolve
3. `check_evidence_presence` — non-artifacts must have evidence
4. `check_evidence_completeness` — required sub-fields present
5. `check_dag` — no cycles in `requires`/`derives_from` (DFS)
6. `check_version_format` — matches `^\d+\.\d+$` (compiled once via `LazyLock`)

### Graph Builder (`src/graph/`)
- `KnowledgeGraph` fields are `pub(crate)` for internal mutation; external users use accessor methods
- `entity_type_color()` in `graph/mod.rs` is the **canonical color scheme** used by both `codegen/dot.rs` and `cli::cmd_graph()`
- `GraphQuery` precomputes reverse adjacency in `new()` for efficient `descendants()` queries
- Cycle detection in both validator and graph builder is intentional (defense in depth)

### CLI (`src/cli/`)
- All commands accept `<path>` arg (file or directory); `graph`/`diff`/`search` default to `"."` if omitted
- Version string uses `env!("CARGO_PKG_VERSION")` — update in `Cargo.toml` only
- Each command is a separate function: `cmd_validate()`, `cmd_compile()`, `cmd_graph()`, `cmd_diff()`, `cmd_search()`

### Lexer (`src/lexer/`)
- Hand-written byte scanner (not generated)
- Unknown bare words are hard errors (no generic `Ident` token)
- String literal parsing extracted to `read_string_literal()` helper (shared with annotation parsing)
- Annotations lexed as single tokens: `@name("value")`

### Parser (`src/parser/`)
- Uses `chumsky` 0.9 for maintainability and error recovery
- Custom `Span` implements `chumsky::span::Span` trait
- Relationships flattened: `requires ["a", "b"]` → two `Relationship` structs
- Every AST node carries `SourceLocation`

### Code Generators (`src/codegen/`)
Each generator is independent and only reads `KnowledgeGraph`:
- `json.rs` — serde-structured; `compiled_at` from `chrono`; `evidence` omitted when `None`
- `markdown.rs` — YAML frontmatter; filename = `id.replace(['.', '_'], "-") + ".md"`
- `search.rs` — flat array; `content_text` concatenates all text; `tags` from annotations + type
- `dot.rs` — Graphviz `digraph`; uses shared `entity_type_color()`

## Testing

### Test Helpers (`src/test_helpers.rs`)
Shared helpers for all test modules (marked `#[cfg(test)]` in `lib.rs`):
- `test_location()` — default `SourceLocation`
- `test_entity(id, entity_type, version)` — minimal `Entity`
- `test_relationship(rel_type, target_id)` — `Relationship`
- `test_evidence_block()` — `EvidenceBlock` with one `Revelation`

Local test wrappers (e.g., `entity()`, `make_entity()`) delegate to these helpers.

### Integration Tests (`tests/`)
- `cli_test.rs` — spawns binary via `cargo run` for end-to-end validation
- `snapshot_test.rs` — uses `insta` to snapshot JSON, Markdown, Search, DOT outputs

### Fixtures (`tests/fixtures/`)
- `seven_laws.vdl` — complete valid VDL: 1 framework + 7 laws with `derives_from` chains
- `divine_alignment.vdl` — framework with all relationship types + full evidence block
- `invalid_cycle.vdl` — intentionally invalid: circular `requires` chain

## Adding a New Validation Rule

1. Add function in `src/validator/rules.rs`:
   ```rust
   pub fn check_my_rule(module: &Module) -> Vec<ValidationError> { ... }
   ```
2. Add `ValidationError` variant in `src/validator/error.rs` if needed
3. Update `From<ValidationError> for VdlError`
4. Call the new rule in `src/validator/mod.rs::validate()`
5. Add unit tests in `src/validator/mod.rs` tests module

## Adding a New Output Target

1. Create `src/codegen/<target>.rs`
2. Implement `generate(graph: &KnowledgeGraph, output_path: &Path) -> VdlResult<()>`
3. Export in `src/codegen/mod.rs`
4. Wire into CLI in `src/cli/mod.rs::cmd_compile()`
5. Add unit tests and snapshot test

## Known Limitations (v0.1)

- **No incremental compilation** — full recompile every run
- **No plugin system** — output targets are hardcoded
- **String values only** — properties are `HashMap<String, String>`
- **In-memory graph only** — no external database
- **CLI `diff` is simplified** — no true version history; compares by substring match
- **No WASM target** — CLI binary only

## Updating This Document

If you modify the architecture, update this file to reflect:
1. New pipeline stages or changed data flow
2. New public API functions
3. New validation rules or codegen targets
4. Changed design decisions
