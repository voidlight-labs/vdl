# CLI Usage

`validate` and `compose` accept a single `.vdl` file or a directory scanned
recursively. `compile` accepts a project directory with `vdl.toml`.

## `vdl validate <file_or_dir>`

Check schema sources and report errors with file/line/column locations. A
single file must contain a schema block. A directory is loaded as one project,
with exactly one schema across all its sources; no build manifest is needed.
Legacy fixed-type documents are no longer supported and must be migrated.

```bash
vdl validate tests/fixtures/schema/runbooks.vdl
vdl validate path/to/project/
```

See [validation.md](validation.md) for the rules.

## `vdl compile <project_dir>`

A project directory **must** contain `vdl.toml`. Every registered Markdown
target is validated and rendered before the first output is written. A missing
manifest is an error; there is no fallback pipeline.

```bash
vdl compile tests/fixtures/project
# → tests/fixtures/project/dist/runbooks/deploy.md
# → tests/fixtures/project/dist/runbooks/recovery.md
# → Compilation complete.
```

## `vdl compose <file_or_dir> --root <entity_id> [--out <file>] [--cites <mode>]`

Render one entity into a complete Markdown document. Without `--out`, the
result is printed to stdout. Composition follows the same project loading
rules as validation and does not require a manifest. See
[composition.md](composition.md) for output shape, cite rendering, and safety.

```bash
vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide
vdl compose path/to/project/ --root deploy --out deploy.md
vdl compose path/to/project/ --root deploy --cites short
```

Composition cycles are reported when rendering the affected root, for example
`vdl compose tests/fixtures/invalid_cycle.vdl --root a`.
