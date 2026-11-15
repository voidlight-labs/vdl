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
```

`root` is an entity ID. `out_dir` is relative to the project root, and each
`output` is relative to `out_dir`. Types, fields, relationships, and the
`heading`/`body`/`compose` rules stay in `.vdl`; the manifest only lists
build targets and render options (`cites`, `number_headings`).

Diagnostics across the project:

- exactly one schema block in the whole project; zero or several report every
  location involved;
- a project that mixes schema documents with legacy entity documents is
  rejected, naming the legacy files;
- missing and wrong-type references point at the instance that uses them, in
  whichever file or folder it lives;
- duplicate entity IDs name both the duplicate and the first declaration.
