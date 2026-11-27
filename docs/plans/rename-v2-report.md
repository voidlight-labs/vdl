# vdl rename v2

## Plan and acceptance criteria

1. Extend the rename plan to keep local declaration IDs and preserve reference spelling (bare local, dotted unquoted, quoted full ID), while updating resolvable include directives in place.
2. Plan file and matching entity-container folder renames from old local name to new local name; reject case-folded entity and destination path collisions. Predict sorted source order and report each moved entity's old/new position without suggesting or applying an alternate name.
3. Make apply perform source span rewrites and planned path moves safely, then reload, validate, and confirm the entity count is unchanged.
4. Add behavior tests including a small owner-repro fixture; document the command output and design choices here.
5. Run `CARGO_TARGET_DIR=.cargo-target cargo test` and `CARGO_TARGET_DIR=.cargo-target cargo build --release`; run the demo dry-run and write paths. Request Echo's independent R1-R5 gate.

Acceptance: R1-R5 and requested regression cases pass; dry-run is read-only and concise; `--write` leaves a valid project with the same entity count. No grammar, validator, compile/Markdown pipeline, commit, or push changes.

## Initial root-cause evidence

- `src/rename.rs` writes the full new ID into declarations, upgrades non-bare references to quoted IDs, warns instead of rewriting resolvable bare includes, and has no path-renaming plan.
- `RenamePlan::apply` writes files individually and does not account for source order after path changes.
- The existing `tests/rename_test.rs` checks basic bare preservation and exact include rewrites, but not local declarations, dotted spelling, filesystem moves, ordering, or entity-count invariance.

## Changes by file

- `src/rename.rs`: package-local bare-name validation against lexer and parser segment rules; bare, dotted, quoted, and include argument form preservation; re-resolution of every bare reference and active include using the model's dedented directive scanner; raw byte offsets handle LF, CRLF, and lone CR; project-wide explicit-import guard; scoped source folder/file moves; case-folded sibling collision checks including nested moves; order-shift calculation; path-aware apply.
- `src/cli/mod.rs`: per-file before/after spellings, path moves, order positions, source edit totals; reload the new path for single-file invocation; post-write entity-count assertion.
- `tests/rename_test.rs`: coverage for dry-run, preserved forms, package-crossing quoted/dotted refs, file/folder moves, unrelated same-named source, order shift, entity and filesystem collisions, bare/include resolution shadowing, instance-free stale imports, invalid declaration spelling including trailing hyphens, single-file apply, active composition, standalone brace Markdown, local/global include spelling collision, mixed line endings, validation, and entity-count invariance.
- `tests/fixtures/rename-v2/`: owner-style container + two children, cross-file bare reference, full/quoted/local includes, fenced and indented Markdown examples plus standalone brace text, active body composition, cross-package quoted+dotted refs, and a same-named unrelated source file.
- `docs/cli.md`: updated rename behavior and dry-run/write contract.

No grammar, validator, compile pipeline, Markdown pipeline, or split-entities code changed.

## Design decisions

- The declaration is emitted as the new local bare name. A rename to a different package is rejected because a package-local declaration in the existing source cannot represent that full ID; this also prevents a write/reload mismatch.
- Existing bare references are re-resolved after the simulated rename. References to the target use the new local name; any other bare reference that would retarget or become ambiguous rejects the plan. Explicit imports directly naming the old entity are rejected because import spans are not modeled for safe rewriting.
- Only the target entity's own `<old-local>.vdl` file is renamed. A matching ancestor folder of that entity's source is renamed, and its descendant sources move with it; project root components and unrelated same-named files are excluded.
- Order shifts are predicted from sorted source paths and declaration order, and reported without auto-renaming to an alternate name.
- Include directives update full and resolvable local spellings; quotes around full IDs are retained.

## Verification evidence

- `CARGO_TARGET_DIR=.cargo-target cargo test`: PASS, all suites including unit, CLI, graph, project, rename, schema, snapshot, split-entities, and doctests; **292 passed, 0 failed**.
- `CARGO_TARGET_DIR=.cargo-target cargo build --release`: PASS.
- Targeted regression: `CARGO_TARGET_DIR=.cargo-target cargo test --test rename_test -- --nocapture`: PASS, 17 passed.
- Release demo from `tests/fixtures/rename-v2/`: `vdl rename guide.sections.child-a guide.sections.child-z <demo>` reported 8 source edits, the file and folder move, and order changes `child-a: 1 -> 2`, `child-b: 2 -> 1`. It showed bare `child-a -> child-z`, quoted full ID retained with quotes, dotted ID retained unquoted, and all three include spellings preserved.
- Same demo `--write`: PASS; output `Renamed ... across 4 file(s)`. Follow-up `vdl validate` PASS: **6 files, 5 entities**. `vdl compose <demo> --root guide.sections.child-b` included `Child A body.` through the rewritten include; `vdl stats --json` reported **5 entities**.
- Echo reviews 1 through 6 found issues that were fixed and regression-tested. Review 6 identified an overlapping ancestor/sibling folder move; the folder target is now mapped through planned ancestor moves and covered by `rename_maps_sibling_container_through_matching_ancestor`. The final independent gate is recorded in `rename-v2-echo-review-1.md`.

## Compact demo dry-run output

```text
1 edit(s) in .../children/child-a/child-a.vdl -> .../children/child-z/child-z.vdl
  Declaration: "guide.sections.child-a" -> child-z
4 edit(s) in .../children/child-b/child-b.vdl
  Include: guide.sections.child-a -> guide.sections.child-z
  Include: "guide.sections.child-a" -> "guide.sections.child-z"
  Include: child-a -> child-z
  Reference: child-a -> child-z
2 edit(s) in .../external/reader.vdl
  Reference: "guide.sections.child-a" -> "guide.sections.child-z"
  Reference: guide.sections.child-a -> guide.sections.child-z
1 edit(s) in .../container.vdl
  Reference: child-a -> child-z
folder: .../children/child-a -> .../children/child-z
order shift: guide.sections.child-a: 1 -> 2
order shift: guide.sections.child-b: 2 -> 1
Dry run: 8 edit(s) would rename ...
```

## Gate follow-up plan

Echo's first independent review found four correctness gaps. Before changes:

1. Restrict file rename to the target entity's source file and folder rename to matching ancestor folders of that source; apply folder moves to descendant project sources only. Never transform the project root or unrelated same-named files.
2. Reject cross-package renames that cannot be represented by a package-local declaration. Re-resolve every existing bare reference against the simulated project and reject any target change; reject imports that explicitly name the old ID unless their source can be safely rewritten.
3. Rewrite quoted include arguments without removing quotes, and compute the argument byte span after the directive prefix rather than searching the whole line.
4. Reload the renamed file path for single-file invocations. The focused tests and full requested verification now pass; final independent review is recorded in `rename-v2-echo-review-1.md`.

### Follow-up after second review

Echo identified additional edge cases before final acceptance. The next verification slice covers:

1. Validate the generated local declaration as one lexer-recognized bare name; inspect project-wide `SchemaDoc::imports`, including files with no entities.
2. Match raw include scanning to the model's active Markdown rules after accounting for the braced body's common indentation; simulate and re-resolve every active include, including wildcard shadowing.
3. Check case-folded sibling destinations against filesystem entries (including empty folders), and resolve nested move destinations in their original parent namespace.
4. Keep the demo's body composition active and test both successful include expansion and rejected include retargeting; confirm formatted changes remain limited to the rename handler.

All four follow-up checks pass. The fixture composes the renamed child body, rejects include shadowing, confirms the instance-free import guard, validates local declaration syntax, and covers case-folded sibling and nested destinations.

### Follow-up after third review

Echo found two remaining include edge cases. Before the final gate:

1. Use `body_include_directives` on the parser's dedented lexer body to select active lines; map those line indexes to raw source offsets. Do not infer Markdown base indentation from arbitrary source lines.
2. Resolve every include spelling before deciding whether it points to the renamed entity; an unquoted exact text match alone is insufficient when global and package-local IDs can shadow one another.
3. Add focused tests for a standalone `}` Markdown line and a colliding global/local include spelling, then rerun the full suite, release build, and demo.

These include-scanner fixes pass the standalone-brace and global/local spelling regressions.

### Follow-up after fourth review

Echo found two further cases:

1. Mirror the parser's segment rule for local names, including rejection of a trailing hyphen, in addition to checking lexer tokenization.
2. Map normalized include line indexes back to raw bytes while treating CRLF and lone CR as line boundaries; add a mixed-newline include regression.
3. Rerun targeted and full verification, then request a fresh gate review.

The trailing-hyphen and lone-CR regressions and full verification now pass. Final independent review is recorded in `rename-v2-echo-review-1.md`.

### Follow-up after fifth review

1. Keep full dotted declaration spelling for files without a `package`; use the local segment only when a package header supplies the namespace. Validate each segment with parser-compatible rules.
2. When the target source is named `<old-local>.vdl`, also move its same-named sibling container folder and descendant project sources; preserve root boundaries and report resulting sorted-source order changes.
3. Add package-less dotted-ID and sibling-container-folder regressions, rerun all requested checks, then request a fresh independent gate.

Those regressions pass. The final release demo reports 8 span edits across 4 source files, moves `children/child-a/child-a.vdl` and its containing folder, and reports both changed entity positions. `--write`, `validate`, `compose`, and `stats --json` all pass; validation and stats both report 5 entities. The full suite passes with 292 tests, and the release build succeeds. Echo review 6 found a nested path edge case that is closed in the follow-up below.

### Follow-up after sixth review

Echo caught a path-mapping edge case when the matching sibling container folder is nested under another folder that is also renamed. Its destination now passes through the ancestor mapping before its basename changes, so the planned destination matches the sequence used by apply and order prediction. The regression places `old.vdl` and an `old/` child folder inside `old/`, then checks the dry-run destination, `--write`, validation, and unchanged entity count. The final full suite passes with 292 tests, release build succeeds, and the owner-style demo was rerun against the rebuilt release binary. The new path assertion uses platform-native path components. Final Echo gate: see re-review 8 in `rename-v2-echo-review-1.md`.
