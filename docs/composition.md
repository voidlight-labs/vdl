# Composition & Rendering

## `vdl compose <file_or_dir> --root <entity-id>`

Renders one entity into a complete Markdown document. The input may be one
`.vdl` schema document or a project directory; a directory is merged from all
its sources and needs no manifest. Legacy entity files are rejected. Prints
to stdout when `--out` is omitted.

```sh
vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide
vdl compose path/to/project/ --root deploy --out deploy.md
```

## Output shape

```
---
id: "core-guide"
type: "Runbook"
"label": "Core Guide"
"see_also":
  - "appendix"
---

# Core Guide

## Introduction

<chapter body verbatim>

## Rules

<chapter body verbatim>
```

Frontmatter lists `id`, `type`, the root type's `text` fields in declaration
order, then non-composed `ref` collections as YAML lists. `id`, type names,
text values, reference IDs, and author-defined keys are emitted as
double-quoted YAML strings. Escapes preserve control characters and Unicode
line separators through YAML parsing.

`id` and `type` are compiler-owned keys. A root field that would be emitted
under either name is a composition error, reported before any output is
written.

Composed collections expand recursively. Each referenced entity contributes
its heading and body, then its own composed collections. Nested headings
increment one level at a time, up to Markdown's six heading levels.
Composition cycles are errors. An ordinary reference contributes only its ID
in root frontmatter.

## Labels

A type's label renders before its body: labels that start with `#` are
emitted verbatim as Markdown headings, other labels are emitted in bold.
This applies to entities embedded through `inline` as well.

## Inline metadata

An `inline` target also renders its populated text fields as
`**field:** value` metadata lines between its label and body. The root
entity does not do this — its text fields live in frontmatter.

## Heading numbering

Set `number_headings = true` in a `[[build.markdown]]` entry to number that
output's composed headings. The root title stays unnumbered. Its first child
renders as `## 1. Title`; that child's second child renders as
`### 1.2 Title`. Each composed level adds one segment, through H6. Sibling
numbering follows body directives first and composed reference fields
afterward.

The default is `false`. The setting does not affect `vdl compose`, frontmatter,
entity IDs, ordinary references, or Markdown headings written inside a body.

## Cite rendering (`--cites`, manifest `cites`)

A `cite`-marked `ref` field can be rewritten at render time into a citation
line. The mode comes from `--cites <mode>` on `vdl compose`, or from a
`cites = "none|link|short"` key on a `[[build.markdown]]` manifest entry
(`compose` defaults to `none`). Rendered modes emit a `**Cites:** ...` line
after the entity's label and before its body:

- `link` lists full entity IDs;
- `short` lists each target's heading field value (falling back to the full
  ID when the target has no heading);
- `none` — the default — leaves output byte-identical to the pre-cite
  behavior.

Inline entities also render their populated text fields as
`**field:** value` metadata lines between label and body (the root entity
keeps text fields in frontmatter instead).

## Build safety

All targets are validated and every Markdown document is rendered in memory
before anything is written. A build fails without touching existing output
when a root is unknown, a destination is a directory, targets are equal or
one target would need to be both a file and a directory, a path leaves
`out_dir`, or a destination aliases a project source or the manifest. Paths
are compared after resolution, so symlinked parents and hard links are
covered. The resolved `out_dir` must remain inside the resolved project
root; symlinks that stay inside are allowed, while symlinks that escape are
rejected. Outputs whose effective filename ends in `.vdl` are rejected so a
later build cannot parse generated Markdown as source. These checks apply
before any output is written.

`compose --out` applies the same protection to every source in the project
and to `vdl.toml`, before truncating the output.

`--out` compares the open input and output files before truncating the
output. On Unix it compares device and inode, and on Windows it compares
volume serial number and file ID. Both checks catch hard links and symlinks.
Other platforms compare canonical paths.
