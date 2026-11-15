# CLI Usage

> All commands that take a `<path>` argument accept either a single `.vdl`
> file or a directory. Directories are scanned recursively for `.vdl` files.
> Schema-project commands must be run from the project root (where
> `vdl.toml` lives).

## `vdl validate <file_or_dir>`

Check all `.vdl` files and report errors with precise file/line/column
locations. A directory that contains schema documents is validated as one
project, even without `vdl.toml`; directories of legacy documents keep the
per-file behavior.

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

See [validation.md](validation.md) for the rule tables.

## `vdl compile <file_or_dir>`

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

## `vdl compose <file_or_dir> --root <entity_id> [--out <file>] [--cites <mode>]`

Render one entity into a complete Markdown document. See
[composition.md](composition.md) for output shape, heading numbering, cite
rendering, and build safety.

```bash
vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide
vdl compose path/to/project/ --root deploy --out deploy.md
vdl compose path/to/project/ --root deploy --cites short
```

## `vdl graph <entity_id> [path]`

Export a 1-hop subgraph centered on an entity to DOT format (stdout).

```bash
# Use .vdl files in the current directory (default)
vdl graph soul.law.i

# Or specify a file/directory
vdl graph soul.law.i tests/fixtures/
# → digraph subgraph { ... }
```

## `vdl diff <id> <v1> <v2> [path]`

Compare two entities. In v0.1, searches for entities whose ID contains the
query substring and diffs the first two matches.

```bash
vdl diff soul law
# → Diff: soul.law.i vs soul.law.ii
```

## `vdl search <query> [path]`

Search entity IDs and titles with a case-insensitive regex.

```bash
vdl search autonomy
# → soul.law.i | law | Autonomy Is Mandatory
```
