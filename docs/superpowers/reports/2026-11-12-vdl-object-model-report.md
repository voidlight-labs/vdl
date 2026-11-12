# Completion Report: VDL User-Defined Object Model

Date: 2026-11-12
Spec: `docs/superpowers/specs/2026-11-12-vdl-object-model-addendum.md`
Plan: `docs/superpowers/plans/2026-11-12-vdl-object-model.md`

## What shipped

A second VDL document form in which the author declares entity types, then
instantiates them with inline Markdown bodies and ordered typed references, and
the compiler renders a selected root entity into one complete Markdown document.
The legacy v0.1 entity form is untouched and still parses, validates, compiles,
and snapshots as before.

### Fixture paths

| Path | Purpose |
|------|---------|
| `tests/fixtures/schema/runbooks.vdl` | Acceptance: `Runbook`/`Chapter`, two runbooks sharing `intro` and `rules`, `see_also` ordinary reference to `appendix` |
| `tests/fixtures/schema/invalid_unresolved.vdl` | Composed reference to a non-existent ID |
| `tests/fixtures/schema/invalid_wrong_type.vdl` | `chapters` requires `Chapter`, reference resolves to a `Runbook` |
| `tests/fixtures/schema/invalid_undeclared_target.vdl` | Declared target type is a typo, reference list empty |
| `tests/fixtures/schema/invalid_duplicate_value.vdl` | Same field supplied twice in one entity |

### Commands

```bash
cargo build --release
cargo test

./target/release/vdl validate tests/fixtures/schema/runbooks.vdl
./target/release/vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide
./target/release/vdl compose tests/fixtures/schema/runbooks.vdl --root core-guide --out core-guide.md
```

Real output of `vdl validate`:

```
✓ Validated 1 files, 5 entities. No errors.
```

Real output of `vdl compose ... --root core-guide`:

```
---
id: core-guide
type: Runbook
label: "Core Guide"
see_also:
  - appendix
---

# Core Guide

## Introduction

Prose starts here without a heading line.

### Internal heading stays as written

Text with **bold**, a “curly quote”, and an — em dash.

## Rules

Rule one: keep the body verbatim.

Rule two: never reorder authored lists.
```

Real output of the undeclared-target negative case (exit code 1):

```
Error:   × [tests/fixtures/schema/invalid_undeclared_target.vdl:7:9] Validation
  │ error: field 'chapters' of type 'Runbook' references undeclared type
  │ 'Chaptre'
```

## Grammar as implemented

```vdl
schema "<name>" {
    type "<Type>" {
        field "<name>" text [heading]
        field "<name>" body
        field "<name>" ref "<Type>" [compose]
    }
}

entity "<Type>" "<id>" {
    field "<name>" "value"
    field "<name>" """raw markdown"""
    field "<name>" [ "id", "id" ]
}
```

All names are quoted strings, which avoids collisions with the legacy keyword
set (`title`, `text`, `body` are already legacy tokens) and keeps the rename
property free. `heading` names the field used for `#`/`##`; `compose` marks an
ordered reference collection for expansion.

## Validation rules implemented

V1 undeclared instance type · V2 duplicate entity ID · V3 duplicate type name ·
V4 duplicate field declaration · V5 undeclared field supplied · V6 missing
required field · V7 value shape mismatch · V8 unresolved reference · V9 wrong
target type · V10 `compose` on non-`ref` · V11 `heading` misuse or more than one
`heading` · V12 composed target without a `heading` field · V13 undeclared
`ref` target type even with an empty list · V14 same field supplied twice.

V13 and V14 are the two gaps found in review of the first plan draft; both are
covered by fixtures and unit tests.

## Tests actually run

```
cargo test

unittests src/lib.rs           148 passed, 0 failed, 2 ignored
unittests src/main.rs            0 passed
tests/cli_test.rs                9 passed
tests/schema_test.rs            15 passed
tests/snapshot_test.rs           4 passed
Doc-tests vdl                    1 passed
```

New end-to-end coverage in `tests/schema_test.rs`:

- both runbooks compose to complete Markdown, with trailing newline;
- one `Chapter` shared by two runbooks renders identically in both;
- swapping authored list order swaps rendered order;
- the ordinary `see_also` reference lists `appendix` and never inlines its body;
- body fidelity after the documented trimming and dedent rules, byte-for-byte;
- a CRLF copy of the fixture renders byte-identical to the LF original;
- unresolved reference, wrong target type, undeclared target with empty list,
  and duplicate field value are each rejected;
- the full rename `Runbook→Manual`, `Chapter→Sop`, `chapters→procedures`,
  `label→caption`, `content→prose` validates and composes with no Rust change;
- a synthetic `Widget`/`Slot` vocabulary validates and composes;
- legacy `seven_laws.vdl` still parses as `Document::Legacy`;
- a file starting with `entity` reports the missing schema block.

## Compatibility impact

- New reserved words: `schema`, `type`, `field`, `ref`, `compose`, `body`,
  `heading`, `entity`, plus the `"""` raw literal. A legacy document that used
  any of these as a bare word would now fail lexing; none of the existing
  fixtures do.
- `vdl validate` now dispatches per file: legacy documents are merged and
  validated as a set (unchanged behaviour), schema documents are validated
  standalone. A directory mixing both works; legacy output text is unchanged.
- `Document` replaces direct `Module` returns only in the new
  `parser::parse_document` entry point. `parser::parse`, `validator::validate`,
  the graph builder, and all four legacy codegen targets are unchanged.
- `codegen::markdown::escape_yaml_string` is now `pub(crate)` and shared with
  the composer.

## Deviations from the plan (deliberate)

1. **Modifier misuse is rejected by the validator, not the parser.** The plan
   had `field "x" text compose` as a parse error. Keeping the parser permissive
   means one located diagnostic (V10/V11) owns the message instead of two
   competing code paths.
2. **`parse_schema_document` is a function, not a returned chumsky parser.**
   It mirrors `parser::parse`'s shape and owns the "exactly one schema block"
   pre-check, which a plain combinator cannot express as a good message.
3. **`vdl validate` accepts a directory for schema documents too.** Each file is
   validated independently, which follows from the single-file contract.

## Limitations (kept out on purpose)

All fields required (no `optional`); composition one level deep; no scalar
references; one file per schema; no inheritance, methods, plugin validators, or
external content resolvers; schema documents do not enter the knowledge graph,
so `compile`/`graph`/`search`/`diff` remain legacy-only. No commit was made.
