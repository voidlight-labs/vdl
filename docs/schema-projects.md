# Schema Projects (multi-file builds)

A project is any directory containing `.vdl` files with exactly one `schema`
block. Sources are discovered recursively, including nested folders. Entity
IDs are project-global: a file's name and location never affect resolution,
and moving a source file does not change rendered bytes.

```
project/
├── vdl.toml
├── schema.vdl
├── runbooks/
│   ├── deploy.vdl
│   └── recovery.vdl
└── sections/
    └── nested/
        └── intro.vdl
```

`vdl validate <dir>` and `vdl compose <dir> --root <id>` merge the project
without requiring a manifest. `vdl compile <dir>` uses the manifest:

```toml
[build]
out_dir = "dist"

[[build.markdown]]
root = "deploy"
output = "runbooks/deploy.md"

[[build.markdown]]
root = "recovery"
output = "runbooks/recovery.md"

# Optional per-target cite rendering mode: "none" (default), "link", "short"
cites = "short"

# Optional per-target heading numbering
number_headings = true

# Optional graph artifacts rendered from the whole project
[[build.graph]]
format = "json"
output = "graph.json"

[[build.graph]]
format = "dot"
output = "graph.dot"

[[build.graph]]
format = "index"
output = "search.json"
```

`root` is an entity ID. `out_dir` is relative to the project root, and each
`output` is relative to `out_dir`. Types, fields, relationships, and the
`heading`/`body`/`compose` rules stay in `.vdl`; the manifest only lists
build targets and render options (`cites`, `number_headings`).

`[[build.graph]]` entries are optional; at least one `[[build.markdown]]`
target is always required. `format` is one of `json` (node/edge list),
`dot` (Graphviz digraph), or `index` (JSON search documents). Each artifact
is rendered from the whole validated project, matches the `vdl graph` output
for the same project byte for byte, and its destination passes the same
safety checks as Markdown targets: it must stay inside `out_dir`, must not
resolve to a `.vdl` path, and must not alias a source, the manifest, or
another target. Nothing is written until every target has been planned.

Diagnostics across the project:

- exactly one schema block in the whole project; zero or several report every
  location involved;
- legacy fixed-type documents are rejected and must be migrated to schema form;
- missing and wrong-type references point at the instance that uses them, in
  whichever file or folder it lives;
- duplicate entity IDs name both the duplicate and the first declaration.
