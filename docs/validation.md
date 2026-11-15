# Validation Rules

Every diagnostic carries `file:line:column` and names the offending
declaration or reference. All violations are collected, not short-circuited.

## Schema documents

| Rule | Rejects |
|------|---------|
| V1 | instance of an undeclared type |
| V2 | duplicate entity ID |
| V3 | duplicate type name |
| V4 | duplicate field name inside one type |
| V5 | field supplied that its type does not declare |
| V6 | required field not supplied |
| V7 | value shape does not match the declared field kind |
| V8 | reference to an undeclared entity ID |
| V9 | reference whose target entity has a different type |
| V10 | Composition on an unsupported field kind |
| V11 | `heading` on a non-`text` field, or more than one `heading` per type |
| V12 | composed target lacks the heading or body required by the composition mode |
| V13 | A declared target type is missing, even when no instance references it |
| V14 | the same field supplied twice in one instance |
| V15 | more than one `body` field in a type, even with no instances |
| V16 | `cite` on a non-`ref` field |

Implementation: `src/validator/schema.rs`.

## Legacy documents

The legacy (v0.1) form runs six rule checks in sequence:

1. **Type constraints** — enforce mandatory relationships per entity type
   (see the entity types table in [grammar.md](grammar.md#entity-types))
2. **Reference integrity** — all relationship targets must resolve
3. **Evidence presence** — non-artifacts must have evidence
4. **Evidence completeness** — required sub-fields present
5. **DAG validation** — DFS with GRAY/BLACK coloring detects cycles
6. **Version format** — regex `^\d+\.\d+$` (legacy only; schema documents
   have no built-in version concept)

Implementation: `src/validator/rules.rs`.

## Project diagnostics

Multi-file projects add project-wide diagnostics on top of the rules above:

- exactly one schema block in the whole project; zero or several report every
  location involved;
- a project that mixes schema documents with legacy entity documents is
  rejected, naming the legacy files;
- missing and wrong-type references point at the instance that uses them, in
  whichever file or folder it lives;
- duplicate entity IDs name both the duplicate and the first declaration.

Example output:

```
[tests/fixtures/invalid_cycle.vdl:5:5] Validation error: Cycle detected: ...
```
