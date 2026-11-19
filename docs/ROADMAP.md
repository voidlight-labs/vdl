# Roadmap

## Planned

Nothing is planned right now.

## Postponed

### Co-occurrence validation

**Status:** postponed
**Recorded:** 2026-11-16

Require-if-verified guards for dalil files may instead be handled by a lint
script.

## Done

### Graph build targets in the manifest

**Status:** done
**Recorded:** 2026-11-20
**Date:** 2026-11-20

`vdl.toml` accepts optional `[[build.graph]]` targets (`format = "json" |
"dot" | "index"` plus an `output` path inside `out_dir`), so `vdl compile`
produces graph artifacts alongside Markdown. All destinations share the
Markdown targets' guards; graph artifacts are byte-identical to `vdl graph`
output. At least one Markdown target remains required.

### Schema-form graph & search outputs

**Status:** done
**Recorded:** 2026-11-16
**Date:** 2026-11-20

Built a graph over the validated schema project model with reference fields as
edges, plus JSON graph, Graphviz DOT, and search-index outputs. `graph`
emits all three formats (`--format json|dot|index`) with an optional `--root`
one-hop neighborhood filter; `search` queries entity content
(id, type, heading, text fields, body) as a case-insensitive substring.
`diff` remains dropped. The new implementation reads the schema-form model
only — no legacy fixed-type vocabulary was resurrected.

### Remove the legacy fixed-type pipeline

**Status:** done
**Date:** 2026-11-16

Removed fixed-type parsing and validation, the graph builder, its JSON, DOT,
search and Markdown generators, and the legacy-only `graph`, `search`, and
`diff` commands. Schema-form validation and Markdown composition remain.
