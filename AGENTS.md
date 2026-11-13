# Agent Documentation: VDL Compiler

> **For agents working on this codebase.**
> Last updated: November 2026 | VDL Compiler v0.1

## Quick Commands

```bash
# Build and run
cargo build --release
./target/release/vdl --help

# Install globally
cargo install --path .

# Test compiler, CLI, schema projects, snapshots, and doctests
cargo test --locked

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

**Two document forms.** Legacy entity documents run the five stages above.
Schema documents (author-defined types) short-circuit the graph stage:
`parse_document` → `SchemaDoc` → `validator::schema` → `codegen::compose`.
A schema project merges fragments from every source before validation. It
uses the schema declarations to distinguish compact instances from legacy
declarations; see "Schema projects" before changing the parser.

```
src/
├── lexer/          # Byte-oriented scanner → Token stream
├── parser/         # chumsky 0.9 combinators → AST (Module), SchemaDoc, or SchemaFragment
├── model/          # SchemaDoc: user-declared types, fields, instances
├── project/        # Recursive discovery, vdl.toml manifest, guarded Markdown build
├── validator/      # 6 legacy rules + 15 schema rules → VdlResult<()>
├── graph/          # In-memory DAG with IndexMap nodes
├── codegen/        # json, markdown, search, dot + compose (schema docs)
├── fsutil.rs       # File identity: canonical anchors, device/inode, Windows file ID
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

### Schema documents (second document form)

Added in the user-defined object model slice. Read this before changing the
parser, validator, or renderer.

- **Schema instances have two spellings.** `entity "<type>" "<id>"` and
  `Type "<id>"` produce the same model. Field values likewise accept
  `field "<name>"` or a bare name. In a project, a bare type declared in
  the schema can also use a legacy keyword as its name.
- **Packages are per file.** An optional `package` header precedes any schema
  and instances. Imports are also file-local: a one-level `import prefix.*`
  or an explicit `import full.id as alias` resolves bare references after all
  project entities load. Precedence is explicit alias, local package, then
  wildcard; ambiguous and missing imports retain source locations. Bare dotted
  and quoted IDs retain their spelling.
- **The single-file parser requires exactly one schema.**
  `parse_schema_document` rejects zero or two blocks. The project-facing
  `parse_schema_fragment` also accepts files that only declare instances.
- **Domain vocabulary is forbidden in Rust.** No comparison against `"Runbook"`,
  `"Section"`, `"sections"`, `"title"`, or `"body"` anywhere in `src/`. Heading
  is a schema flag (`FieldDecl::heading`), body is a field kind
  (`FieldKind::Body`), composition is a schema flag (`FieldDecl::compose`).
  `tests/schema_test.rs::renaming_types_needs_no_rust_change` is the guard.
- **Composition follows nested collections.** `codegen::compose` expands each
  entity's heading and body, then its composed collections in declaration order
  and author order. An opted-in braced body (`body compose ["Type"]`) can place
  standalone `include id` directives in its Markdown; directives are inactive
  inside fenced code and in triple-quoted literals. Includes use the same
  heading depth, numbering, cycle, and six-level rules. Ordinary refs go into
  the root's frontmatter as YAML lists and never inline a body.
- **References can list several allowed types.** `ref ["A", "B"]` keeps one
  ordered list of IDs and validates every target against the allowed types.
  An `inline` collection renders each referenced body and any literal
  `prose { … }` items in order. It leaves heading depth and numbering unchanged.
  A schema type's optional `label` is rendered before its body.
- **Body literals are raw.** Braced and triple-quoted bodies share newline
  normalization and parse-time dedent. The closing brace must be alone on
  its line and aligned with the field. Bare `evidence {` retains its legacy
  meaning; use `field "evidence" {` for a schema body with that name.
- **Schema rules live in `validator::schema`, not the parser.** The parser
  accepts `heading` on a ref field or `compose` on a text field so that one
  located diagnostic (V10/V11) owns the message.
- **Frontmatter is always quoted YAML.** `id`, `type`, text values, and ref IDs
  go through `yaml_double_quoted`; `yaml_key` also uses it for every author key.
  Long keys use YAML's explicit key syntax. `id` and
  `type` are reserved for the compiler, and a root field that would emit under
  either name is a composition error. Do not relax this without a real YAML
  round-trip test (no YAML crate is a dependency).
- **`compose --out` guards project inputs before truncating.** It opens the output
  without truncating, compares the open handle with every project source and
  `vdl.toml`, and only then truncates. On Unix and Windows, `fsutil::same_file`
  compares device/inode or volume serial number/file ID; other platforms use
  canonical anchors. Hard links and symlinks are covered.
- **Adding a schema rule:** add a function returning `Vec<ValidationError>` in
  `src/validator/schema.rs`, call it from `collect_schema_errors`, add a unit
  test named `v<N>_*`, and add the row to the README table.

### Schema projects (multi-file builds)

A project is a directory of `.vdl` sources with exactly one `schema` block.
`src/project/` owns everything above the single document; read it before
changing discovery, manifests, or output writing.

- **Discovery is recursive and name-blind.** `project::discover_vdl_files`
  sorts paths for determinism. `schema.vdl` is a convention only. File names
  and folders never affect entity IDs.
- **Loading merges fragments.** `project::load_schema_project` calls
  `parse_schema_fragment` per schema-form file, collects the single schema and
  all instances, then runs the existing `validator::schema` rules once. AST
  locations keep each file, line, and column, so cross-file references point at
  the usage and duplicate IDs name both declarations.
- **Three page-level failures are explicit.** No schema block, more than one
  schema block (all locations listed), and a project mixing schema documents
  with legacy entity documents. A legacy-only directory keeps the old pipeline.
- **One loader serves `validate`, `compose`, and `compile`.** `validate` and
  `compose` accept a project directory without a manifest. `compile` uses
  `vdl.toml` when present; a schema directory without one reports the manifest
  example and the expected path.
- **`vdl.toml` is parsed with `toml` and normalized.** `[build] out_dir` and
  every `[[build.markdown]] output` are relative, normalized, and rejected when
  they are absolute or climb out of the project. Unknown keys, missing fields,
  and empty target lists produce a manifest-path diagnostic with an example.
- **Markdown numbering is per target.** The optional `number_headings` boolean
  defaults to false. The renderer uses composed sibling order and depth to
  generate prefixes. Root headings and authored body text are unchanged.
- **Build is plan-then-write.** `project::build_project` checks every target
  (root exists, destination is not a directory, stays inside `out_dir` after
  symlink resolution, does not alias a source or the manifest, does not collide
  with another target), renders all Markdown in memory, and only then creates
  directories and writes. Validation failures leave existing output untouched.
  File comparisons use `fsutil::paths_alias`/`canonical_anchor`; do not add a
  second identity implementation.
- **Project tests carry the contract.** `tests/project_test.rs` covers the
  compile-and-move byte comparison, cross-file diagnostics, manifest failures,
  destination conflicts, source/manifest protection, symlinks, and hard links.
  `tests/fixtures/project/` is the reference layout.


### CLI (`src/cli/`)
- All commands accept `<path>` arg (file or directory); `graph`/`diff`/`search` default to `"."` if omitted
- Version string uses `env!("CARGO_PKG_VERSION")` — update in `Cargo.toml` only
- Each command is a separate function: `cmd_validate()`, `cmd_compile()`, `cmd_graph()`, `cmd_diff()`, `cmd_search()`, `cmd_compose()`
- `validate` probes for schema form and delegates to `project::load_schema_project`; legacy directories keep the original per-file loop
- `compile` uses `project::build_project` when the directory has `vdl.toml`, reports a missing manifest for a schema directory, and otherwise runs the legacy graph pipeline
- `compose` always loads through the project loader; `--out` protects every project source and the manifest

### Lexer (`src/lexer/`)
- Hand-written byte scanner (not generated)
- Bare words that are not reserved keywords become identifier tokens. Schema
  instances use them for type and field names.
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
- `schema_test.rs` — schema document acceptance, negative cases, CRLF fidelity,
  and the rename check that proves no domain vocabulary lives in `src/`
- `project_test.rs` — multi-file project loading, `vdl.toml` targets, the
  compile-and-move byte comparison, cross-file diagnostics, and output guards
  (including symlink and hard-link aliases)

### Fixtures (`tests/fixtures/`)
- `seven_laws.vdl` — complete valid VDL: 1 framework + 7 laws with `derives_from` chains
- `divine_alignment.vdl` — framework with all relationship types + full evidence block
- `invalid_cycle.vdl` — intentionally invalid: circular `requires` chain
- `schema/runbooks.vdl` — schema document acceptance fixture: two runbooks sharing chapters
- `schema/invalid_*.vdl` — unresolved reference, wrong target type, undeclared
  target type with an empty list, duplicate field value
- `project/` — multi-file project fixture: `vdl.toml`, one schema file, two
  runbooks in separate files, and shared sections under nested directories

### Zed extension (`editors/zed/`)

The local extension uses its own Tree-sitter grammar and Markdown injections.
When surface syntax changes, update the editor grammar and queries, then parse
the fixtures and representative project sources. See
[`editors/zed/README.md`](editors/zed/README.md) for the build and install steps.

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

### Schema documents

- **No scalar references.** Every `ref` is an ordered collection.
- **One schema block per project (or per single file).** Schema distribution
  (several schema blocks combined) is out of scope. A project may span folders;
  a single-file document still contains its own schema.
- **No inheritance, executable methods, plugin validators, or external content
  resolvers.**
- **Schema projects produce Markdown only.** `vdl compile <dir>` with `vdl.toml`
  renders the registered Markdown targets. `vdl graph`, `vdl search`, and
  `vdl diff` still operate on legacy documents only; schema projects do not
  reach the knowledge graph.
- **No cross-project references.** Every referenced ID must be declared in the
  same project; vault migration is out of scope.

### Legacy

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
