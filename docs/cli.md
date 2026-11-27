# CLI Usage

`validate`, `compose`, `graph`, `search`, and `stats` accept a single `.vdl`
file or a directory scanned recursively. `compile` accepts a project
directory with `vdl.toml`.

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

## `vdl compile <project_dir> [--split-entities]`

A project directory **must** contain `vdl.toml`. Every registered target is
validated and rendered before the first output is written: `[[build.markdown]]`
targets compose one Markdown document per entity root, and optional
`[[build.graph]]` targets write `json`, `dot`, or `index` artifacts for the
whole project (identical bytes to `vdl graph --format`). A missing manifest is
an error; there is no fallback pipeline. See [schema-projects.md](schema-projects.md)
for the manifest shape.

`--split-entities` writes one page per entity under
`out_dir/entities/<package>/<full-entity-id>.md`. Reference fields and
container child lists link to generated pages.

```bash
vdl compile tests/fixtures/project
# → tests/fixtures/project/dist/runbooks/deploy.md
# → tests/fixtures/project/dist/runbooks/recovery.md
# → Compilation complete.
```

```bash
vdl compile path/to/project/ --split-entities
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

## `vdl graph <file_or_dir> [--format json|dot|index] [--root <entity_id>] [--out <file>]`

Emit the project graph: entities become nodes, every reference-field target
becomes a directed edge labeled with the field name. Nothing is hardcoded:
the graph is read entirely from the validated schema, and nodes follow the
loader's declaration order.

- `--format json` (default): a JSON object with `generator`, `nodes`
  (`id`, `type`, optional `label` from the heading field), and `edges`
  (`from`, `to`, `field`). No timestamp is emitted, so equal inputs produce
  equal bytes.
- `--format dot`: a Graphviz `digraph`. Node labels prefer the heading value
  and fall back to the ID; fill colors are assigned per declared type in
  first-seen order.
- `--format index`: a JSON array of search documents (`id`, `type`, optional
  `label`, `content`). `content` gathers the ID, type name, heading, text
  fields, and body; reference values are not included.

`--root <entity_id>` narrows all formats to one entity and its one-hop
neighborhood: the entities it references and the entities referencing it.
`--out` writes to a file (refusing to overwrite any project source, exactly
like `compose --out`); without it the output goes to stdout.

```bash
vdl graph tests/fixtures/schema/runbooks.vdl --format dot
vdl graph path/to/project/ --root core-guide
vdl graph path/to/project/ --format index --out search.json
```

## `vdl search <query> <file_or_dir>`

Search entity content for a case-insensitive substring. Matches cover entity
IDs, type names, heading labels, text fields, and body content. One line is
printed per matching entity in declaration order:

```
core-guide | Runbook | Core Guide
```

With no matches the command reports `No entities matching '<query>'.` and
still exits successfully.

```bash
vdl search deploy path/to/project/
```

## `vdl rename <old-id> <new-id> <file_or_dir> [--write]`

Rename an entity ID across a project's sources: the entity's declaration,
every reference to it (quoted, dotted, or bare), and exact
`include <id>` directives in braced bodies. Quoted and dotted references are
rewritten to the quoted new ID; bare references keep their bare spelling with
the new ID's final segment, and a rewrite is refused when that spelling would
resolve to something else. Include directives written as bare short names are
reported as warnings for manual follow-up.

Without `--write` the command prints a dry run (per-file edit counts) and
writes nothing. The rewritten project is validated in memory before any file
is touched, and revalidated after writing.

```bash
vdl rename soul.alignment.foundation soul.alignment.core path/to/project/
vdl rename old.id new.id path/to/project/ --write
```

## `vdl stats <file_or_dir> [--json]`

Report field population and reference health. For every declared type: the
entity count and, per field, how many entities supply it (`supplied/total`)
with its kind and required/optional marker, so gaps on optional fields stand
out. The report also lists entities no reference field points at, in
declaration order; roots that reference others but are never referenced
themselves appear here by design, next to forgotten ones. `--json` emits the
same report as a JSON object.

```bash
vdl stats path/to/project/
vdl stats path/to/project/ --json
```
