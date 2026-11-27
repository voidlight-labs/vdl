# Split entities implementation report

Status: incomplete. The current draft compiles and the existing library suite passes, but acceptance is not met. Independent review returned FAIL.

## Changes made

- `src/cli/commands.rs` and `src/cli/mod.rs` add the opt-in `compile --split-entities` flag and dispatch.
- `src/project/mod.rs` exports the option-aware build entry point.
- `src/project/build.rs` plans entity outputs, checks case-folded path collisions, and renders entity pages through the normal guarded output planner. Existing `build_project` calls the option-aware function with split mode disabled.
- `src/codegen/split.rs` adds an initial entity page renderer with YAML title/id/version, text and body fields, reference fields, and child list.
- `docs/cli.md` documents the flag. `docs/ROADMAP.md` records the feature as in progress.
- `docs/plans/split-entities-impl-plan.md` records the implementation plan.

## Commands and evidence

- `cargo build --release` with `CARGO_TARGET_DIR=.cargo-target`: passed. The current pre-change release binary compiled the workspace snapshot to `.split-entities-snapshot/vdl/dist/` and produced the two manifest Markdown targets. This baseline was not byte-compared after the patch.
- `cargo check --tests` with `CARGO_TARGET_DIR=.cargo-target`: passed. At that point it reported one unused-variable warning, subsequently removed.
- `cargo test --lib` with `CARGO_TARGET_DIR=.cargo-target`: passed, 172 passed and 0 failed.
- In the analysis phase, the existing release binary ran `validate .` in `the master codex project (external, private)`: 326 files and 472 entities validated. The current compiler source was not re-run against that external source after implementation.
- No split build was run against the copied snapshot. No end-to-end tests were added. Link resolution, Arabic preservation, output count, and no-flag parity remain unverified.

## Known gaps from independent review

- All pages receive no navigation value, so prev/next is absent.
- Body include handling is a manual scan and does not respect fenced code or schema compose flags. The ordering for field includes and body includes is not aligned with the existing composition rules.
- Inline reference/prose values are skipped, and active body include directives remain in the rendered body.
- Entity filenames use only the final dotted ID segment, so IDs in the same source folder can collide. Markdown URL characters and labels are not escaped.
- Generated relative links and child ordering have not been validated end to end.

## Gate

Independent reviewer verdict: FAIL. Reviewer evidence: `src/project/build.rs` passes `None` for navigation and scans includes manually; `src/codegen/split.rs` omits inline values and uses the final ID segment for filenames. The implementation requires another pass before it meets the approved acceptance criteria.

## Round 2

### Changes

- Entity pages now use full ID-derived filenames, shortening long IDs/folder components deterministically and avoiding Windows device names. Case-folded collision detection still runs on planned paths.
- Split links percent-encode every URL path segment separately. Markdown labels escape brackets and backslashes. The legacy relative-link helper remains unchanged to protect single-document output.
- Child directives are read with the model's fenced-code-aware helper and resolved through the existing package/import resolver. Body includes are emitted as links only for braced bodies with schema compose targets. Inline ref/prose lists preserve order.
- Child ordering starts with body directives, then uses schema-declared composed fields. An explicit `includes` field defines the container index list where present. Navigation is grouped by source folder, follows container order, skips singleton groups, and retains each container context on shared entities.
- Split frontmatter emits title, ID, and supplied version/status text fields.
- Added filename/encoding and Markdown label unit tests.

### Verification

- `CARGO_TARGET_DIR=F:/vdl-compiler-build-target cargo test` was attempted and failed before compilation because Cargo could not open `F:/vdl-compiler-build-target/debug/.cargo-build-lock` (Access denied). This drive is outside the writable workspace sandbox. The same full suite was run with the workspace-local `.cargo-target` and passed: 174 library tests, all integration tests, and doctest (see command output).
- `cargo build --release` with `.cargo-target` passed.
- Clean snapshot compile without the flag compared SHA-256 for the two manifest Markdown outputs against `.baseline-dist`: 0 differing files.
- `vdl compile <clean snapshot> --split-entities` produced 472 pages. Scripted scan found 1,145 internal Markdown links and 0 missing targets. Divine Alignment index has 10 ordered entries and Human OS has 11. Dalil `soul.revelation.quran.30.30` page contains Arabic text and `original_text`, `translation`, and `locator` fields.
- `vdl validate the master codex project (external, private)` passed: 326 files, 472 entities.

### Remaining gate state

An independent review after the first fixes returned FAIL. It found composed fields omitted from navigation, YAML control-character escaping gaps, and insufficient regression tests. The split frontmatter now reuses the existing YAML quote serializer. The container index intentionally uses an explicit `includes` field when present, matching the required 10-entry Divine Alignment index; other reference fields still render as linked entity relations in their page body. When no `includes` exists, composed field targets supply the index list. Tests still do not cover all requested scenarios. The fresh final review is pending.

The build target requested by the brief remains unavailable under the active filesystem boundary: Cargo failed with Access denied opening `F:/vdl-compiler-build-target/debug/.cargo-build-lock`. Full tests passed using `.cargo-target` inside the workspace. No writes were made to the master codex project.

### Final independent review

Fresh echo review verdict: FAIL. It confirmed the shared YAML serializer, isolated split URL escaping, and deterministic container iteration. Remaining code gap: split planning selects only the explicit `includes` field when present and omits other composed collections, including inline refs, from child/navigation planning (`src/project/build.rs`). Remaining verification gap: no split integration tests were added, and snapshot E2E was not rerun after the latest YAML-only adjustment. Gate is FAIL until these are addressed and reviewed again.

## Round 3

### Changes

- Child/navigation planning now collects all schema-declared composed reference collections in schema and member order, after active braced-body include directives. Inline refs remain linked in the rendered page body and are deliberately excluded from reading-order navigation.
- The container page renders each composed collection in a separate ordered section. This preserves the 10-item `includes` section for Divine Alignment while keeping other composed collections visible. Navigation uses all composed collections and body directives, groups by source folder, and retains a container link for shared entities. Inline refs are intentionally excluded from reading-order navigation, while inline prose and refs render in authored order in the entity body.
- Added `tests/split_entities_test.rs`, an end-to-end fixture test covering entity count, both include syntaxes, relative link resolution, case-insensitive filename collision errors, absent metadata, inline prose/reference order, shared navigation neighbors, Arabic text fields, multiple compose collections, and no-flag output parity.

### Verification

- `CARGO_TARGET_DIR=.cargo-target cargo test`: passed, 175 library tests, 93 integration tests including 2 split tests, and 1 doctest.
- `CARGO_TARGET_DIR=.cargo-target cargo build --release`: passed.
- Clean master-codex snapshot, default compile SHA-256 parity against `.baseline-dist`: 0 mismatches across the two manifest Markdown outputs.
- Split compile of the clean snapshot: 472 pages. Link scan: 1,146 internal links, 0 unresolved. Container index counts: Divine Alignment `includes` section 10, Human OS body-includes section 11. Dalil Arabic and `original_text`, `translation`, `locator` fields were verified in the Round 2 scan, and the integration fixture asserts Arabic preservation.
- `vdl validate the master codex project (external, private)`: passed, 326 files and 472 entities.
- The requested Cargo target on F: is denied by the sandbox. All build/test commands used `.cargo-target` inside the workspace.

### Review

The independent reviewer identified duplicate navigation positions when an entity appears in multiple composed collections in one container. The planner now deduplicates IDs for navigation in first-occurrence order, without removing their entries from separate index sections. The fixture asserts prev/next neighbors for the shared child in both containers. Full tests and clean snapshot checks were rerun after this fix.

Fresh independent echo review verdict: PASS. It confirmed ordered sections for body includes and every non-inline compose collection, navigation deduplication in first-occurrence order, and fixture assertions for neighbor order in both containers. Reviewer noted optional hardening to assert exactly one navigation row per container on a shared entity. `git diff --check` was clean apart from Git's line-ending notice.

## Round 4

### Decision update

The owner-approved source-folder layout is replaced by `entities/<first-id-segment>/<full-entity-id>.md`. The package directory keeps the output organized and limits path depth. The complete ID is the filename stem, with no truncation or hash suffix. Internal link targets use this map, and link labels use the entity heading title with the ID as fallback. The collision guard remains active. This supersedes round 1's source-folder requirement because URLs for deeply nested source paths were too long.

### Changes

- `src/codegen/split.rs` now creates the shallow full-ID path and resolves Markdown labels from entity titles.
- `src/project/build.rs` uses those paths for planned output and all relation links.
- `tests/split_entities_test.rs` and split unit tests assert full ID filenames, package directory, title labels, and path lengths on fixtures.
- `docs/cli.md` documents the flat package layout.
- CLI help now describes the new layout. The split planner rejects IDs outside the stated lowercase ASCII pattern, and Windows preflights component and total path lengths before writing any output.

### Verification and filesystem constraint

- `cargo test` and `cargo build --release` passed with `CARGO_TARGET_DIR=.cargo-target` in the workspace.
- A clean copy from `the master codex project (external, private)` compiled default outputs with 0 SHA-256 differences against the saved baseline.
- Split compilation of the clean copy fails on a real entity whose full ID is 256 ASCII characters. Appending `.md` makes the filename component 259 characters. Windows cannot create a component longer than 255 characters, even with a shallow parent path. The planner now reports this before writing any outputs. Therefore the full-ID filename rule cannot emit all 472 pages on this Windows filesystem. Page count, complete link scan, and path-length acceptance are not met in Round 4.
- `vdl graph <clean snapshot> --format json` inspection found another full ID at 254 characters, producing a 257-character filename component. A flat relative output path for the 256-character ID is at least 270 characters (`entities/<package>/<id>.md`), already beyond the requested 260-character bound before adding the absolute project path.
- The clean Round 4 snapshot's default compile still had 0 SHA-256 differences. Split compilation stopped in preflight with the explicit 259-character diagnostic, exit code 1, and confirmed `entities/` was not created, so there was no partial split output.
- The supplied ID pattern did not include hyphens or numeric-only later segments, while valid existing project IDs such as `demo.include-root` and `quran.30.30` use them. The path guard preserves those IDs while rejecting path separators, empty segments, and Windows device basenames. The stated pattern also does not cap ID length. This source project's IDs exceed the filesystem component limit. Full filename preservation, `.md` extension, and complete output cannot all be satisfied here without changing one of those requirements.
- Master validation passed: 326 files and 472 entities. Fresh independent review returned FAIL because the Windows component limit makes complete output impossible under the full-ID filename decision. The MAX_PATH guard resolves the absolute destination before measuring, and the ID path check rejects Windows device names. The complete output remains impossible due to the separate 255-character component limit.
- Final fresh independent review confirmed the full-ID naming implementation, reserved-name guard, passing tests/build/validation, and the 255-character filename blocker. Gate verdict: FAIL because the 472-page split build and full link scan cannot complete under the approved full-ID filename decision on Windows.
