# Grammar Reference

VDL uses schema form: each document or project declares its own model.
The legacy v0.1 fixed-type form was removed on 2026-11-16; old documents
must be migrated to schema form.

---

## Schema documents (author-defined model)

A schema document is self-describing: it declares its own entity types and
instantiates them. No type name, field name, heading field, or composition
target is known to the compiler ahead of time.

Two layouts are supported:

- **Single file** — one `.vdl` file contains the schema and all instances.
  Exactly one schema block per file; zero or two are parse errors.
- **Project** — a directory of `.vdl` files. Exactly one file declares the
  schema; every other file may declare only instances, anywhere in the tree.
  See [Schema Projects](schema-projects.md).

`schema.vdl` is only a convention: no file name or folder is special.

```vdl
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "chapters" ref "Chapter" compose
        field "see_also" ref "Chapter"
    }

    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}

Chapter "intro" {
    label "Introduction"
    content """
    Prose starts here, no heading line.

    ### Internal heading stays as written
    """
}

Runbook "core-guide" {
    label "Core Guide"
    chapters ["intro"]
    see_also ["appendix"]
}
```

### Instance syntax

A schema instance can use either a bare type name or `entity "<type>"`. Its
fields can use either bare names or `field "<name>"`. Both forms build the
same model. Bare names are case-sensitive and match
`[A-Za-z_][A-Za-z0-9_]*`. Use the quoted form for names with punctuation or
spaces, or for type names that conflict with the `schema`, `entity`, and
`package` declarations.

In a project, bare type names are resolved against the declared schema,
including names such as `framework` or `artifact`.

**Versions are author-defined.** The schema form has no built-in version
field and no compiler-enforced version format. If your model needs versions,
declare a `version` text field and store whatever format you want; the
compiler treats it like any other text field.

### Packages and entity names

Declare a package at the start of a schema source file, before any schema
or instance declarations.

```vdl
package soul.alignment

artifact sleep-and-death {
    title "Sleep and Death"
    body {
        Prose starts here.
    }
}
```

The ID is `soul.alignment.sleep-and-death`. Entity declarations and reference
lists use the same naming rules:

- A bare short name is prefixed with the file's package, if declared.
- A bare dotted name is a full ID. Use it for references to other packages.
- A quoted ID is used as written. It retains the meaning it had before
  packages.

Bare entity names and packages use dot-separated segments of ASCII letters,
digits, underscores, and internal hyphens. Package names and entity IDs are
case-sensitive. Moving a file between directories leaves its package and IDs
intact. Manifest roots and `vdl compose --root` use the full ID.

Files may import one entity with an optional alias, or direct children of a
package with a one-level wildcard:

```vdl
import soul.human-os.*
import soul.revelation.quran.55.33 as ar-rahman
```

Bare references resolve in this order: explicit imports (including aliases),
the file's package, then wildcard imports. Ambiguous names report the
candidate IDs. Without a package, a bare name can fall back to a global ID.
Quoted IDs and dotted IDs keep their literal meaning. Import declarations are
file-local, so moving a source file does not change its imports or entity IDs.

### Field kinds

| Declaration | Value form | Meaning |
|-------------|------------|---------|
| `field "<name>" text` | `"..."` | Single-line quoted string |
| `field "<name>" body` | Multiline `{ … }` or `"""..."""` | Raw Markdown |
| `field "<name>" ref "<Type>"` | `[ "id", "id" ]` | Ordered collection of entity IDs of type `<Type>` |
| `field "<name>" ref ["<TypeA>", "<TypeB>"]` | `[ "id", "id" ]` | Ordered collection whose IDs can have any listed type |

Every `ref` is a collection. Declared fields are required by default.

### Modifiers

- `heading` — `text` fields only, at most one per type. It names the field
  used as that type's heading. A root entity renders it as `# ...`. Each
  composed reference increases the heading level by one. No heading is
  inferred from a field name.
- `compose` on a `ref` field marks the collection for document composition.
  Multiple composed fields render in declaration order, and each list renders
  in author order (never sorted).
- `compose ["Type"]` after a `body` kind opts that braced body into heading
  composition. A standalone `include <id>` line inserts the referenced entity
  at that position, including its heading and body. Includes follow local
  package and import resolution, target types are checked, and normal depth,
  cycle, and heading-number rules apply. Fenced code stays literal;
  triple-quoted bodies never interpret include lines. Body directives render
  before any composed reference fields on the same entity.
- `inline` — `ref` fields only. Emits each target's body in list order,
  without a heading, frontmatter, or nested composition. The list can also
  contain `prose { … }` literals between references. Inline entries do not
  consume heading depth or heading numbers. Each referenced target must
  supply its body. Populated text fields of an inline target render as
  `**field:** value` metadata lines between label and body.
- `cite` — `ref` fields only (validated by V16). Marks the collection as a
  citation list that the cite renderer can rewrite at render time.
  Non-`cite` reference fields are never touched by cite rendering.
- `optional` — permits an instance to omit the field. A supplied value still
  follows the declared field kind and reference rules.

A heading-composed target type must declare a heading field. An inline
target type must declare a body field.

### Labels

A type can set a rendered body label, for example
`type "Example" label "[EXAMPLE]" { … }` or
`type "Revelation" label "#### REVELATION" { … }`. A label that starts with
`#` is emitted verbatim on its own line (a Markdown heading); any other
label is emitted in bold. Both forms render before the body, including when
embedded through `inline`.

### Raw body literal rules

A braced body starts on the line after the opening brace. Close it with a
standalone `}` at the same indentation as the field. For a prose item in a
list, a comma may follow the closing brace. Other braces are Markdown
content.

```vdl
artifact "soul.alignment.sleep-and-death" {
    title "Sleep and Death"
    body {
        Prose starts here.
    }
}
```

Braced bodies work for schema-defined body fields, including bare `evidence`
when declared in the schema. Triple-quoted bodies also work.

Bodies are literal, with exactly five permitted transformations. Fidelity
means byte-identical after these:

1. `\r\n` and lone `\r` are normalized to `\n`.
2. No escape processing: `\"`, `\\` and friends stay as written.
3. One newline immediately after the opening delimiter is dropped.
4. A trailing whitespace-only line before the closing delimiter is dropped.
5. The longest common leading whitespace prefix across non-blank lines is
   stripped, so an indented source block does not become a Markdown code
   block.

Blank lines and relative indentation are preserved exactly. Non-ASCII text
is decoded as UTF-8 and survives byte-identical.
