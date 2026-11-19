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

### TOC build targets

**Status:** done
**Recorded:** 2026-11-20
**Date:** 2026-11-20

`[[build.toc]]` targets render a composed heading outline per root
(`compose_toc` over a no-body variant of the composition walk, include
directives included). `number_headings` matches the Markdown numbering;
destinations share the common target guards.

### Cross-file cite links

**Status:** done
**Recorded:** 2026-11-20
**Date:** 2026-11-20

A `[[build.markdown]]` entry can set `cite_links = true`: `link`-mode cites
whose targets are themselves registered Markdown targets render as relative
Markdown links between the compiled outputs; other targets keep the plain
ID. `ComposeOptions` gained `cite_links` (an entity-to-path map computed by
the build) and stopped being `Copy`.

### Enum-restricted text fields (V17)

**Status:** done
**Recorded:** 2026-11-20
**Date:** 2026-11-20

`field "<name>" text in ["a", "b"]` restricts a text field to the listed
values; instances with other values are rejected (V17), and restrictions on
non-text kinds are rejected at the declaration. `in` is a contextual keyword.
The Zed Tree-sitter grammar and highlights were updated and rebuilt.

### Reference rewrite (`vdl rename`)

**Status:** done
**Recorded:** 2026-11-20
**Date:** 2026-11-20

`vdl rename <old> <new>` rewrites an entity ID across sources: declaration,
reference values (quoted, dotted, bare; bare keeps its spelling with the new
final segment when resolution allows), and exact braced-body `include`
directives. The parser now retains byte spans for reference values and
instance IDs (`FieldValue.ref_spans`, `Instance.id_span`). Dry run by
default; `--write` applies after in-memory validation, and the project is
revalidated after writing.

### Field population report (`vdl stats`)

**Status:** done
**Recorded:** 2026-11-20
**Date:** 2026-11-20

`vdl stats` reports, per declared type, how many entities exist and how many
supply each field (`supplied/total` with kind and required/optional markers),
plus the list of entities no reference field points at (roots included, by
design). `--json` emits the same report as JSON.

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
