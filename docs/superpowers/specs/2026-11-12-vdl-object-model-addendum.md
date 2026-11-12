# Spec: User-Defined VDL Object Model (Milestone Addendum)

Status: accepted 2026-11-12. Supersedes earlier VDL multi-domain milestone requirements.

## Objective

Implement the first end-to-end slice of VDL as a declarative knowledge modeling
language. A knowledge base author must be able to define entity types, instantiate
them, connect them through typed references, and compile the resulting model into
a complete Markdown document.

## Scope precedence

This addendum supersedes earlier milestone requirements to prove Soul v0.1
compatibility, introduce Soul and General validation profiles, or use Voidlight
pillars as built-in types. The existing Voidlight corpus is authored in Markdown.
Vault migration is a separate task.

Inspect the existing compiler and reuse relevant stages where practical. Legacy
fixtures are useful implementation context. Passing them is outside this
milestone's acceptance criteria.

## Required behavior

1. **Schema declarations.** Authors can declare entity types in VDL source. The
   schema can define text fields, a multiline body, and an ordered collection of
   references to entities of a declared target type.
2. **Entity declarations.** Authors can instantiate schema-defined types with
   stable IDs. A single entity can be referenced from multiple entities without
   duplicating its body.
3. **Schema-driven validation.** The compiler rejects unknown types, missing
   required fields, duplicate entity IDs, unresolved references, and references
   whose target type differs from the schema. Diagnostics identify the offending
   declaration or reference.
4. **Ordered composition.** The schema can designate an ordered reference
   collection for document composition. The compiler renders the selected root
   entity's metadata and the referenced content in declared order. Ordinary
   references do not include their targets' bodies in the output.
5. **Content in VDL.** A schema-defined entity can carry its body directly in
   `.vdl` source. Preserve line breaks, blank lines, and Markdown syntax within
   the body according to documented literal rules.

## Acceptance fixture

Synthetic schema with two types:

- `Runbook` has a `title` and an ordered `sections` collection referencing
  `Section`. The schema marks `sections` for composition.
- `Section` has a `title` and a multiline `body`.

Declare two runbooks that share one section. Verify both compile to complete
Markdown, reordering section references changes rendered order, and an ordinary
reference does not include another entity's body.

Negative cases: unresolved section ID, and a reference to a `Runbook` where a
`Section` is required.

**Decisive check:** `Runbook`, `Section`, and the meaning of `sections` for
composition come from the authored schema. Renaming the types and updating their
declarations and references requires no Rust code changes.

## Implementation boundaries

Smallest grammar and internal representation that satisfy the fixture. Document
resulting syntax, validation rules, and reproduction commands.

Use local `.vdl` files. Out of scope: databases, synchronization, schema
distribution, inheritance, executable methods, plugin validators, external
content resolvers, advanced graph queries. Leave the existing Markdown vault and
its build pipeline outside the change.
