# Roadmap

## Planned

### Schema-form graph & search outputs (planned next)

**Status:** planned next
**Recorded:** 2026-11-16

Build a graph from the validated schema project model, using reference fields
as edges. Provide JSON graph, Graphviz DOT, and search-index outputs, with
`graph` and `search` CLI commands. `diff` is dropped unless a concrete need
emerges.

The legacy implementation was removed on 2026-11-16. The schema-form compose
model differs from the former fixed-type `KnowledgeGraph`; this work is a new
implementation over the schema model, not a resurrection of the old pipeline.

## Postponed

### Co-occurrence validation

**Status:** postponed
**Recorded:** 2026-11-16

Require-if-verified guards for dalil files may instead be handled by a lint
script.

## Done

### Remove the legacy fixed-type pipeline

**Status:** done
**Date:** 2026-11-16

Removed fixed-type parsing and validation, the graph builder, its JSON, DOT,
search and Markdown generators, and the legacy-only `graph`, `search`, and
`diff` commands. Schema-form validation and Markdown composition remain.
