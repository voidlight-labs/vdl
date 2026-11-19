# Agent Documentation: VDL Compiler

> **For agents working on this codebase.**
> Last updated: 2026-11-20

## Quick Commands

```bash
cargo build --release
./target/release/vdl --help

cargo install --path .

# All test suites (lib, schema, project, cli, snapshot, doctests)
cargo test

# Unit tests only
cargo test --lib

cargo check --tests
cargo insta review   # update snapshots after output format changes
```

## Architecture

**Single document form.** VDL documents are schema documents: they declare
their own entity types and fields in a `schema` block, and the compiler
learns the model from the source (no type/field names in Rust — the guard
test is `tests/schema_test.rs::renaming_types_needs_no_rust_change`).

**Pipeline:** Lexer → Parser → Validator (schema rules V1–V16) → Code
Generator (`compose.rs`) → Graph & search (`graph/`). The legacy fixed-type
pipeline (graph builder, JSON/DOT/search/legacy-Markdown codegen,
`graph`/`diff`/`search` commands) was removed on 2026-11-16; `graph` and
`search` were re-implemented over the schema model on 2026-11-20 and `diff`
remains dropped.

```
src/
├── lexer/          # Byte-oriented scanner → Token stream
├── parser/         # chumsky 0.9 combinators → SchemaDoc or SchemaFragment
├── model/          # SchemaDoc: user-declared types, fields, instances
├── project/        # Recursive discovery, vdl.toml manifest, guarded Markdown build
├── validator/      # 16 schema rules → VdlResult<()>
├── codegen/        # compose.rs: schema model → composed Markdown
├── graph/          # Graph build over the schema model; JSON/DOT/search-index renderers
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
- `validate()` collects all errors; CLI converts `VdlError` → `miette::Report`
  for rich diagnostics

### Schema documents

Read this before changing the parser, validator, or renderer.

- **Schema instances have two spellings.** `entity "<type>" "<id>"` and
  `Type "<id>"` produce the same model. Field values likewise accept
  `field "<name>"` or a bare name.
- **Packages are per file.** An optional `package` header precedes any schema
  and instances. Imports are file-local: `import prefix.*` or
  `import full.id as alias` resolve bare references after all project
  entities load. Precedence is explicit alias, local package, then wildcard.
  Bare dotted and quoted IDs retain their spelling.
- **The single-file parser requires exactly one schema.**
  `parse_schema_document` rejects zero or two blocks. The project-facing
  `parse_schema_fragment` also accepts files that only declare instances.
- **Domain vocabulary is forbidden in Rust.** Heading is a schema flag
  (`FieldDecl::heading`), body is a field kind (`FieldKind::Body`),
  composition is a schema flag (`FieldDecl::compose`).
- **Composition follows nested collections.** `codegen::compose` expands each
  entity's heading and body, then its composed collections in declaration
  order and author order. An opted-in braced body (`body compose ["Type"]`)
  can place standalone `include id` directives in its Markdown; directives
  are inactive inside fenced code and in triple-quoted literals.
- **References can list several allowed types.** `ref ["A", "B"]` validates
  every target against the allowed types. An `inline` collection renders each
  referenced body and any literal `prose { … }` items in order; populated
  text fields render as `**field:** value` metadata lines.
- **Labels are heading-aware.** A type's `label` starting with `#` renders
  verbatim as a Markdown heading; other labels render in bold.
- **Body literals are raw.** Braced and triple-quoted bodies share newline
  normalization and parse-time dedent. The closing brace must be alone on its
  line and aligned with the field.
- **Schema rules live in `validator::schema`, not the parser.** The parser
  accepts `heading` on a ref field or `compose` on a text field so that one
  located diagnostic (V10/V11) owns the message.
- **Frontmatter is always quoted YAML.** `id` and `type` are reserved for the
  compiler; a root field that would emit under either name is a composition
  error. Do not relax this without a real YAML round-trip test (no YAML crate
  is a dependency).
- **`compose --out` guards project inputs before truncating.** Comparisons
  use `fsutil::paths_alias`/`canonical_anchor`; do not add a second identity
  implementation.
- **Adding a schema rule:** add a function returning `Vec<ValidationError>`
  in `src/validator/schema.rs`, call it from `collect_schema_errors`, add a
  unit test named `v<N>_*`, and add the row to `docs/validation.md`.

### CLI (`src/cli/`)
- Commands: `validate`, `compile`, `compose`, `graph`, `search`.
- `validate`, `compose`, `graph`, and `search` accept a project directory
  without a manifest.
- `graph` emits `--format json|dot|index` with an optional `--root` one-hop
  filter; `search` runs a case-insensitive substring query over entity content.
  Both reuse the guarded `--out` write from `compose`.
- `compile` requires `vdl.toml` (reports a manifest example when missing).
  Legacy documents are rejected with a clear message — the legacy pipeline
  was removed 2026-11-16.

### Schema projects (multi-file builds)

`src/project/` owns everything above the single document; read it before
changing discovery, manifests, or output writing.

- **Discovery is recursive and name-blind.** `schema.vdl` is a convention
  only. File names and folders never affect entity IDs.
- **One loader serves `validate`, `compose`, and `compile`.**
- **`vdl.toml` is parsed with `toml` and normalized.** Unknown keys, missing
  fields, and empty target lists produce a manifest-path diagnostic with an
  example. Per-target render options: `number_headings`, `cites`.
- **Build is plan-then-write.** Every target is checked and all Markdown is
  rendered in memory before anything is written.
- **Project tests carry the contract.** `tests/project_test.rs` covers the
  compile-and-move byte comparison, cross-file diagnostics, manifest
  failures, destination conflicts, source/manifest protection, symlinks, and
  hard links. `tests/fixtures/project/` is the reference layout.

## Testing

- `cli_test.rs` — spawns binary via `cargo run` for end-to-end validation
- `snapshot_test.rs` — hand-written equality of compose output
- `schema_test.rs` — schema acceptance, negative cases, CRLF fidelity, and
  the rename check (no domain vocabulary in `src/`)
- `project_test.rs` — multi-file loading, manifest targets, build guards
- `graph_test.rs` — graph/index/search over multi-file projects, one-hop
  filtering, determinism, and `--out` guards

Known environment quirk: some `project_test` diagnostics-assertion tests can
fail on narrow terminals due to `miette` stderr line-wrapping — pre-existing,
not a regression. Verify against a clean checkout before treating failures
as caused by your change.

## Zed extension (`editors/zed/`)

The local extension uses its own Tree-sitter grammar and Markdown injections.
When surface syntax changes, update the editor grammar and queries, then
parse the fixtures and representative project sources. See
[`editors/zed/README.md`](editors/zed/README.md).

## Known Limitations

- **No scalar references.** Every `ref` is an ordered collection.
- **One schema block per project (or per single file).**
- **No inheritance, executable methods, plugin validators, or external
  content resolvers.**
- **Graph/index outputs are CLI-only.** `vdl.toml` registers Markdown targets
  only; there are no manifest targets for graph, DOT, or search-index files,
  and there is no `diff`.
- **No cross-project references.** Every referenced ID must be declared in
  the same project.

## Updating This Document

If you modify the architecture, update this file to reflect: new pipeline
stages or changed data flow, new public API functions, new validation rules
or codegen targets, changed design decisions.
