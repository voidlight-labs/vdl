# Split entities implementation plan

## Scope

Add opt-in `vdl compile <project> --split-entities`. Keep manifest compilation without the flag byte-identical. Do not change grammar or validation. Work only in `src/`, `tests/`, `docs/`, `README.md`, and `vdl.toml`, plus an isolated source snapshot under this workspace. Never write to the source project on F:.

## Steps

1. Snapshot `the master codex project (external, private)` into ignored/temporary workspace storage and compile its current manifest with the current release binary to capture baseline files.
2. Add focused tests for split rendering/build behavior and CLI flag; add implementation to plan destinations before writing, derive deterministic entity paths from source folders and IDs, render markdown/frontmatter/links/index navigation from parsed values.
3. Run focused tests, project validation, split build on the workspace snapshot, verify all Markdown links and expected child ordering/UTF-8, and compare no-flag outputs against baseline byte-for-byte.
4. Update CLI documentation and roadmap, write the implementation report, request an independent read-only gate review, and address findings.

## Verification criteria

- Baseline project validates and produces 472 entities.
- Split snapshot emits one Markdown page per entity, detects case-insensitive path collisions, and all generated local links resolve.
- Both field `includes` and braced-body `include` preserve authored order; container index includes prev/next navigation.
- Frontmatter has title, ID, and present version. Text-only dalil fields and Arabic remain intact.
- No-flag build output bytes match the pre-change baseline.
- Relevant tests and independent review pass, or report exact blocker/evidence.

## Round 2 fix and verification plan

1. Encode full IDs into collision-resistant filenames, and independently escape Markdown labels and URL path segments.
2. Derive ordered children using schema composition flags, `body_include_directives`, and the existing short-name resolver. Replace active include directives in rendered bodies, and preserve inline ref/prose order.
3. Derive navigation from container child sequences and group by source folder. Keep navigation context labels where a child is shared.
4. Add focused regression tests, run the complete test suite and requested snapshot checks, then obtain a fresh independent echo review.
