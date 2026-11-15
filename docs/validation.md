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

## Project diagnostics

Multi-file projects add project-wide diagnostics on top of the rules above:

- exactly one schema block in the whole project; zero or several report every
  location involved;
- legacy fixed-type documents are rejected and must be migrated to schema form;
- missing and wrong-type references point at the instance that uses them, in
  whichever file or folder it lives;
- duplicate entity IDs name both the duplicate and the first declaration.

Example output:

```
[run.vdl:3:5] Validation error: entity references unresolved entity ...
```

Composition also detects cycles while rendering, for example `vdl compose tests/fixtures/invalid_cycle.vdl --root a`.
