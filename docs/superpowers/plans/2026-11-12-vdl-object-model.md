# VDL User-Defined Object Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a VDL author declare entity types, instantiate them with inline Markdown bodies and ordered typed references, validate the result against that schema, and compose a selected root entity into one complete Markdown document.

**Architecture:** A second document form is added beside the existing legacy one. One file carries exactly one `schema` block followed by its `entity` instances. If the first non-annotation token is `schema` or `entity`, the file goes through `parse_document` → `SchemaDoc` (`src/model/`) → `validator::schema` → `codegen::compose`. Otherwise the existing Lexer → Parser → Validator → Graph → Codegen path runs untouched, so current fixtures keep working. No entity type, field name, heading field, or composition target is hardcoded: all of it comes from the authored schema.

**Tech Stack:** Rust 2021, chumsky 0.9 (same combinator style as `src/parser/mod.rs`), clap 4.5, thiserror, insta (existing dev-deps).

**Spec:** `docs/superpowers/specs/2026-11-12-vdl-object-model-addendum.md`

## Global Constraints

- Rust edition 2021; no new dependencies. `Cargo.toml` stays as is.
- Single-file contract: one `.vdl` file = one `schema` block + its instances. Cross-file entity merging is out of scope; `vdl compose` and `vdl validate` take exactly one path arg for schema documents.
- Exactly one schema block per schema document. Zero → parse error. Two → parse error.
- No commits in this plan's steps unless the user explicitly authorizes them (workspace rule: no commit without explicit request). Every "commit" step is skipped; verification is against the working tree.
- Diagnostics carry a `SourceLocation` (`file:line:col`) and name the offending declaration or reference.
- Zero domain vocabulary in Rust: no string comparison against `"Runbook"`, `"Section"`, `"sections"`, `"title"`, `"body"`. The rename test (`Runbook`→`Manual`, `Section`→`Chapter`, `sections`→`chapters`, `title`→`label`, `body`→`content`) is the enforcement mechanism.
- Non-ASCII text must survive byte-identical through lexing, parsing, and rendering (Indonesian corpus is UTF-8).
- Legacy path is kept as a regression check, not as milestone acceptance. The milestone gate is the acceptance fixture in Task 8.

## Language Surface (locked)

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

entity "Chapter" "intro" {
    field "label" "Introduction"
    field "content" """
Prose starts here, no heading line.

### Internal heading stays as written

Blank lines, **Markdown**, and "quotes" stay literal.
"""
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "chapters" [ "intro", "rules" ]
    field "see_also" [ "appendix" ]
}
```

Field kinds:

- `field "<name>" text` — single-line quoted string.
- `field "<name>" body` — raw triple-quoted string, Markdown verbatim (after the trimming/dedent rules below).
- `field "<name>" ref "<Type>"` — ordered collection of entity IDs, written `[ "id", "id" ]`. Every `ref` is a collection (no scalar refs in this slice).

Modifiers:

- `heading` — optional, `text` fields only, at most one per type. Names the field used as that type's heading text. Root entity renders it as `# ...`; a composed reference renders the target's heading field as `## ...`. No heading field is inferred by name.
- `compose` — optional, `ref` fields only. Marks the collection for document composition. More than one composed field renders in declaration order.

Rendering contract for composed collections: the target type must declare exactly one `heading` field (validated, not guessed); the renderer emits `## <that field's value>` followed by the target's body field if it has one. A composed target with no body renders heading only.

Raw body literal rules (documented, tested):

1. `\r\n` and lone `\r` are normalized to `\n` before anything else.
2. No escape processing; `\"`, `\\` and friends are literal text.
3. If the character immediately after the opening `"""` is a newline, it is dropped.
4. If the closing `"""` is preceded by a newline plus only whitespace, that trailing whitespace-only line is dropped.
5. The longest common leading whitespace prefix across all non-blank lines is stripped.
6. Blank lines and relative indentation are preserved exactly.

Fidelity is defined as byte-identical **after** rules 1-5. Rules 1-5 are the only permitted transformations and are covered by tests, including a CRLF case.

Document detection: leading `@name("value")` annotations skipped; first token `schema` or `entity` → schema document; anything else → legacy document.

## File Structure

| File | Responsibility |
|------|----------------|
| `src/model/mod.rs` (create) | `SchemaDoc`, `Schema`, `TypeDecl`, `FieldKind`, `FieldDecl`, `Instance`, `FieldValue`, `Value` — data only, no I/O |
| `src/lexer/token.rs` (modify) | Add `Schema`, `Type`, `Field`, `Ref`, `Compose`, `Body`, `Heading`, `Entity`, `RawString(String)` |
| `src/lexer/mod.rs` (modify) | Keyword table entries; `read_raw_string_literal()` |
| `src/parser/schema.rs` (create) | chumsky parser: schema block, `TypeDecl`, instances |
| `src/parser/mod.rs` (modify) | `Document` enum, `parse_document()` dispatch |
| `src/validator/schema.rs` (create) | Fourteen schema rules + error collection |
| `src/codegen/compose.rs` (create) | `compose(doc, root_id) -> VdlResult<String>` |
| `src/lib.rs` (modify) | `pub mod model;`, re-exports |
| `src/cli/commands.rs`, `src/cli/mod.rs` (modify) | `compose` subcommand; schema-aware `validate` |
| `tests/schema_test.rs` (create) | Acceptance + negative integration tests |
| `tests/fixtures/schema/runbooks.vdl` (create) | Acceptance fixture |
| `tests/fixtures/schema/invalid_*.vdl` (create) | Negative fixtures |
| `README.md`, `AGENTS.md` (modify) | Syntax, rules, commands, limitations |

---

### Task 1: Lexer support for schema documents

**Files:**
- Modify: `src/lexer/token.rs`, `src/lexer/mod.rs`
- Test: inline `mod tests` in `src/lexer/mod.rs`

**Interfaces:**
- Produces: `Token::{Schema, Type, Field, Ref, Compose, Body, Heading, Entity}` and `Token::RawString(String)`; `read_raw_string_literal(bytes, i, line, col, file) -> VdlResult<(String, Span)>` (module-private, mirrors `read_string_literal`).

- [ ] **Step 1: Write the failing tests**

- `lex_schema_keywords` — every new keyword lexes to its token.
- `raw_string_is_literal` — `"""` + `a \" b \\ c` + `"""` yields `a \" b \\ c` with no escape handling.
- `raw_string_trims_one_leading_newline` — `"""\nX\n"""` yields `X`.
- `raw_string_keeps_interior_blank_lines` — `"""\nA\n\n\nB\n"""` yields `A\n\n\nB`.
- `raw_string_keeps_non_ascii` — `tafsir — “kutip”` round-trips byte-identical.
- `raw_string_normalizes_crlf` — `"""\r\nA\r\nB\r\n"""` yields `A\nB`.
- `raw_string_unterminated_is_error` — returns `VdlError::Lexer`.
- `unknown_bare_word_still_errors` — `wibble` still fails.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib lexer` — Expected: compile error (variants and function missing).

- [ ] **Step 3: Implement**

Add token variants + `Display` arms. Register keywords at `src/lexer/mod.rs:266-305`: `schema`, `type`, `field`, `ref`, `compose`, `body`, `heading`, `entity`. In the string dispatcher, three consecutive `"` bytes route to `read_raw_string_literal`.

`read_raw_string_literal` captures the slice, decodes via `std::str::from_utf8` (error → `VdlError::Lexer` "invalid UTF-8 in raw string") instead of `bytes[i] as char`, applies rules 1 (CRLF), 3 (leading newline), 4 (trailing whitespace-only line). Dedent (rule 5) happens at parse time in Task 3.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --lib lexer` — Expected: PASS.

---

### Task 2: Model types and the schema block parser

**Files:**
- Create: `src/model/mod.rs`, `src/parser/schema.rs`
- Modify: `src/lib.rs`, `src/parser/mod.rs` (`pub mod schema;`)
- Test: inline tests in `src/parser/schema.rs`

**Interfaces:**
- Produces (in `src/model/mod.rs`):
  ```rust
  pub struct SchemaDoc { pub schema: Schema, pub entities: Vec<Instance> }
  pub struct Schema { pub name: String, pub types: Vec<TypeDecl>, pub source_location: SourceLocation }
  pub struct TypeDecl { pub name: String, pub fields: Vec<FieldDecl>, pub source_location: SourceLocation }
  pub struct FieldDecl { pub name: String, pub kind: FieldKind, pub heading: bool, pub compose: bool, pub source_location: SourceLocation }
  pub enum FieldKind { Text, Body, Refs { target: String } }
  pub struct Instance { pub type_name: String, pub id: String, pub values: Vec<FieldValue>, pub source_location: SourceLocation }
  pub struct FieldValue { pub name: String, pub value: Value, pub source_location: SourceLocation }
  pub enum Value { Text(String), Body(String), Refs(Vec<String>) }
  ```
  `SchemaDoc.schema` is **not** `Option` (single-schema contract).
- Produces (in `src/parser/schema.rs`): `pub fn schema_doc(file: PathBuf, source: &'a str) -> impl Parser<Token, SchemaDoc, Error = Simple<Token, Span>> + 'a`.

- [ ] **Step 1: Write the failing tests**

- `parses_text_field`, `parses_heading_field` (`field "label" text heading` → `heading: true`), `parses_body_field`, `parses_ref_field_with_compose`, `parses_multiple_types_in_order`.
- `compose_on_text_field_is_parse_error`, `heading_on_ref_field_is_parse_error` — each names the field.
- `missing_schema_block_is_parse_error` — input starting with `entity` alone.
- `two_schema_blocks_is_parse_error`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib parser::schema` — Expected: FAIL (module missing).

- [ ] **Step 3: Implement**

Build with `just(Token::Schema)` + `string` + `types` braced. `field` = `just(Token::Field)` + `string` + kind + modifiers, where `heading` is accepted only for `Text` and `compose` only for `Refs` (reject otherwise with a parser error naming the field). Reuse `span_to_location` from `src/parser/mod.rs` (make it `pub(crate)`). Register `pub mod model;` in `src/lib.rs` with re-exports.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --lib parser::schema` — Expected: PASS.

---

### Task 3: Instance parser, body dedent

**Files:**
- Modify: `src/parser/schema.rs`, `src/parser/mod.rs`
- Test: inline tests in `src/parser/schema.rs`

**Interfaces:**
- Produces: `pub enum Document { Legacy(Module), Schema(SchemaDoc) }`; `pub fn parse_document(tokens: &[(Token, Span)], source: &str, file: &Path) -> VdlResult<Document>`; internal `fn dedent(raw: &str) -> String`.

- [ ] **Step 1: Write the failing tests**

- `parses_instance_head` — `entity "Runbook" "core-guide" { … }`.
- `parses_text_value`, `parses_ref_list_order` (`["a","b"]` preserved, `["b","a"]` reversed), `parses_empty_ref_list`.
- `body_dedent_strips_common_indent` — 8-space indented body with a nested level keeps relative structure.
- `body_preserves_blank_lines` — two consecutive blank lines survive dedent.
- `body_fidelity_after_documented_transform` — a body written with indentation, CRLF, and a leading newline equals the expected LF string byte-for-byte.
- `parse_document_dispatches_on_schema_keyword`, `parse_document_dispatches_on_entity_keyword` (→ schema path, then parse error for missing schema), `parse_document_legacy_on_framework`, `annotations_before_schema_are_tolerated`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib parser::schema` — Expected: FAIL.

- [ ] **Step 3: Implement**

Instance grammar: `just(Token::Entity)` + two `string`s + braced values; each value is `just(Token::Field)` + `string` + (`string` → `Value::Text`, `RawString` → `Value::Body`, bracket list → `Value::Refs`). `dedent` = rule 5 (longest common whitespace prefix over non-blank lines). `parse_document` peeks past `Token::Annotation`; `Schema | Entity` → schema parser, else existing `parse()`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --lib` — Expected: PASS, legacy parser tests included.

---

### Task 4: Schema validation rules

**Files:**
- Create: `src/validator/schema.rs`
- Modify: `src/validator/mod.rs` (`pub mod schema;`), `src/validator/error.rs`
- Test: inline tests in `src/validator/schema.rs`

**Interfaces:**
- Consumes: `SchemaDoc`.
- Produces: `pub fn validate_schema(doc: &SchemaDoc) -> VdlResult<()>` collecting every violation into `VdlError::Validation` / `VdlError::ValidationErrors` like `validator::validate`.

| Rule | Rejects | Message must name |
|------|---------|------------------|
| V1 | instance type not declared | instance id + unknown type |
| V2 | duplicate entity id | id |
| V3 | duplicate type name | type name |
| V4 | duplicate field name inside one type | type + field |
| V5 | instance field not declared on its type | instance id + field |
| V6 | missing declared field | instance id + field |
| V7 | value shape mismatch (`text` given `Body`/`Refs`, `body` given `Text`, `ref` given non-list) | instance id, field, expected kind |
| V8 | unresolved reference id | referring entity + missing id |
| V9 | reference target entity's type ≠ declared target | referring entity, referenced id, expected vs actual |
| V10 | `compose` on a non-`ref` field | type + field |
| V11 | more than one `heading` field in a type, or `heading` on a non-`text` field | type + field |
| V12 | composed collection whose target type declares no `heading` field | type + field + target type |
| V13 | `ref` field whose declared target type is undeclared, **including when the reference list is empty** | type + field + target |
| V14 | the same field supplied twice in one instance | instance id + field |

- [ ] **Step 1: Write the failing tests** — one per rule (`v1_unknown_type_rejected` … `v14_duplicate_field_value_rejected`), each asserting error count and a message substring. Plus `v12_not_triggered_for_non_composed_ref` (a `ref` to a heading-less type is legal when not composed), `v13_fires_for_empty_reference_list`, `valid_fixture_document_passes`, `multiple_violations_are_collected` (`VdlError::ValidationErrors { count: 3, .. }`).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib validator::schema` — Expected: FAIL (module missing).

- [ ] **Step 3: Implement** — pure functions per rule returning `Vec<ValidationError>` (new variants in `src/validator/error.rs` where existing ones do not fit), orchestrated by `validate_schema`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --lib validator` — Expected: PASS.

---

### Task 5: Composition renderer

**Files:**
- Create: `src/codegen/compose.rs`
- Modify: `src/codegen/mod.rs`
- Test: inline tests in `src/codegen/compose.rs`

**Interfaces:**
- Consumes: `SchemaDoc`, `validate_schema`.
- Produces: `pub fn compose(doc: &SchemaDoc, root_id: &str) -> VdlResult<String>`.

Output shape (locked; `label`/`chapters`/`content` are fixture names, not renderer knowledge):

```
---
id: core-guide
type: Runbook
label: Core Guide
---

# Core Guide

<root body, if its type declares a body field>

## Introduction

<chapter body verbatim>

## Rules

<chapter body verbatim>

## See also

- appendix
```

- [ ] **Step 1: Write the failing tests**

- `frontmatter_lists_id_type_then_text_fields` — declaration order, heading field included as a normal text field.
- `root_heading_uses_heading_field` — `# ` value comes from the field marked `heading`, not from a field named `title`.
- `no_heading_field_means_no_h1` — root type without `heading` renders frontmatter + body only.
- `composed_section_heading_uses_target_heading_field`.
- `root_body_rendered_after_title` and `body_markdown_is_not_escaped`.
- `sections_render_in_declared_order`, `reordering_references_changes_output`.
- `shared_section_renders_in_both_documents`.
- `ordinary_refs_render_ids_without_bodies`.
- `unknown_root_is_an_error`, `unvalidated_document_is_rejected`.
- `yaml_special_characters_are_escaped` (reuse `escape_yaml_string` semantics from `src/codegen/markdown.rs`).
- `rename_needs_no_code_change` — same document with `Manual`/`Chapter`/`chapters`/`label`/`content` composes identically in structure.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib codegen::compose` — Expected: FAIL.

- [ ] **Step 3: Implement** — `compose` calls `validate_schema` first, then renders. Heading lookup is by the `heading` flag; body lookup by `FieldKind::Body`; composed fields by the `compose` flag in declaration order, values in list order.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --lib codegen` — Expected: PASS.

---

### Task 6: CLI wiring

**Files:**
- Modify: `src/cli/commands.rs`, `src/cli/mod.rs`
- Test: `tests/cli_test.rs`

**Interfaces:**
- Produces: `Commands::Compose { path: String, root: String, out: Option<PathBuf> }`; `vdl compose <path> --root <id> [--out <file>]`; schema-aware `vdl validate <path>`.

- [ ] **Step 1: Write the failing tests**

- `cli_compose_writes_file`, `cli_compose_prints_to_stdout`, `cli_compose_unknown_root_fails`, `cli_compose_rejects_legacy_document`, `cli_validate_accepts_schema_document`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test cli_test` — Expected: FAIL (`compose` unknown).

- [ ] **Step 3: Implement** — load one path, `parse_document`, branch on `Document`. `Document::Legacy` keeps existing behaviour byte-for-byte in `cmd_validate`; `cmd_compose` rejects it with a message saying composition needs a schema document.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --test cli_test` — Expected: PASS.

---

### Task 7: Acceptance fixtures

**Files:**
- Create: `tests/fixtures/schema/runbooks.vdl`, `invalid_unresolved.vdl`, `invalid_wrong_type.vdl`, `invalid_undeclared_target.vdl`, `invalid_duplicate_value.vdl`

- [ ] **Step 1: `runbooks.vdl`** — schema `"runbooks"`; `Runbook` (`label` text heading, `chapters` ref `Chapter` compose, `see_also` ref `Chapter`); `Chapter` (`label` text heading, `content` body). Chapters `intro`, `rules`, `appendix`; runbooks `core-guide` (`[intro, rules]`, see_also `[appendix]`) and `ops-guide` (`[rules, intro]`, see_also `[appendix]`). Bodies contain no top-level heading, at least one `###` internal heading, blank lines, bold text, a quote character, and non-ASCII prose.

- [ ] **Step 2: Negative fixtures** — unresolved id, wrong target type, undeclared target type with empty list, duplicate field value.

---

### Task 8: End-to-end tests

**Files:**
- Create: `tests/schema_test.rs`

- [ ] **Step 1: Write the failing tests**

- `acceptance_document_validates`
- `both_runbooks_compose_to_complete_markdown`
- `shared_section_body_is_identical_in_both_outputs`
- `reordering_changes_rendered_order`
- `ordinary_reference_excludes_target_body`
- `body_fidelity_after_documented_transform` — composed body equals the fixture literal byte-for-byte after rules 1-5.
- `crlf_source_renders_lf_body` — CRLF copy of the fixture in a temp dir yields the same output as the LF original.
- `unresolved_reference_is_rejected`, `wrong_target_type_is_rejected`
- `undeclared_target_type_rejected_even_with_empty_list`
- `duplicate_field_value_rejected`
- `renaming_types_needs_no_rust_change` — full rename set: `Runbook`→`Manual`, `Section`→`Chapter`, `sections`→`chapters`, `title`→`label`, `body`→`content`.
- `legacy_fixture_still_parses_as_legacy` (regression check, not a gate)
- `entity_first_schema_file_is_rejected` — file starting with `entity` reports the missing schema block.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test schema_test` — Expected: FAIL until the fixtures and API line up.

- [ ] **Step 3: Run to verify pass**

Run: `cargo test` — Expected: PASS.

---

### Task 9: Documentation and completion report

**Files:**
- Modify: `README.md`, `AGENTS.md`
- Create: `docs/superpowers/reports/2026-11-12-vdl-object-model-report.md`

- [ ] **Step 1:** README "Schema documents" section: locked syntax, the six raw-literal rules with the fidelity definition, the fourteen validation rules, single-file contract, commands, limitations (all fields required, one-level composition, no scalar refs, no cross-file merging, legacy path untouched).

- [ ] **Step 2:** `AGENTS.md`: new-path section listing each stage, the keyword dispatch reason, and the rule that domain vocabulary never enters Rust.

- [ ] **Step 3:** Report: fixture paths, exact commands, real test output counts, material compatibility impact (new reserved words `schema`/`type`/`field`/`ref`/`compose`/`body`/`heading`/`entity`), limitations.

- [ ] **Step 4:** Run `cargo build --release`, `cargo test`, and both `vdl` commands from Task 6; paste real output into the report.

---

## Review Focus

1. **Non-ASCII in raw bodies** — the existing `read_string_literal` does `bytes[i] as char` (`src/lexer/mod.rs`) and mangles UTF-8. Test: `raw_string_keeps_non_ascii` (Task 1), `body_fidelity_after_documented_transform` (Task 8).
2. **Indented `"""` block must not become a code block** — 8-space source indent would trip Markdown's 4-space rule. Test: `body_dedent_strips_common_indent` (Task 3).
3. **CRLF cross-platform** — same source with Windows line endings must render the same document. Test: `raw_string_normalizes_crlf` (Task 1), `crlf_source_renders_lf_body` (Task 8).
4. **Blank-line fidelity** — consecutive blank lines must not collapse. Test: `body_preserves_blank_lines` (Task 3).
5. **Reference order is authorial** — `[ "rules", "intro" ]` must render reversed, never sorted. Test: `reordering_changes_rendered_order` (Task 8).
6. **Ordinary refs must not leak bodies** — `see_also` pointing at a body-carrying entity must not inline it. Test: `ordinary_reference_excludes_target_body` (Task 8).
7. **Empty-list ref still validates its target type** — V13. Test: `undeclared_target_type_rejected_even_with_empty_list` (Task 8).

## Self-Review Notes

- Spec 1 → Tasks 1-3 (types, text/body/ref fields, headings). Spec 2 → Tasks 2-3, 7-8. Spec 3 → Task 4 (V1, V2, V6, V8, V9) plus V13/V14 for the gaps found in review. Spec 4 → Tasks 5, 8. Spec 5 → Tasks 1, 3, 8.
- Heading rendering is schema-declared (`heading`), so `# Core Guide` / `## Introduction` never depend on a field named `title`.
- Decisive check enforced twice: unit-level `rename_needs_no_code_change` (Task 5) and end-to-end `renaming_types_needs_no_rust_change` (Task 8), now covering field renames too.
- Single-file/one-schema contract is enforced by `SchemaDoc { schema: Schema, .. }` (no `Option`) and by parse errors for zero or two schema blocks.

---

## Post-implementation notes (2026-11-12)

Three deviations taken during execution, all documented in the completion report:

1. Modifier misuse (`heading` on a `ref`, `compose` on a `text`) is rejected by
   validator rules V10/V11 instead of the parser, so one located diagnostic owns
   the message. Tasks 2 and 4 were adjusted accordingly.
2. `parser::schema::parse_schema_document` is a function mirroring `parse()`,
   not a returned chumsky parser, because it owns the "exactly one schema block"
   pre-check that a combinator cannot express with a good message.
3. `vdl validate` validates each schema document independently inside a
   directory scan; legacy documents are still merged first.

Everything else landed as planned. Full test run: 148 lib + 9 CLI + 15 schema +
4 snapshot + 1 doc test, all passing.
