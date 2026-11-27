---
title: VDL Compiler Dalil Work Plan
type: spec
status: draft
pillar: labs
created: 2026-11-14
updated: 2026-11-14
tags: [voidlight, vdl, compiler, plan]
function: dalil-compiler-plan
---
# VDL Compiler Work Plan: Dalil Module

> **Lokasi update 2026-11-24:** project VDL dipindah keluar dari vault — folder `vdl/` kini di repo master codex (root project). Path `vdl/...` di dokumen ini adalah catatan historis saat project masih di dalam vault; baca sebagai `repo master codex\...` (flattened, tanpa prefix `vdl/`).


> Rencana kerja compiler (repo ini, Rust) buat
> mendukung dalil module per desain di proposal (repo voidlight-codex, `labs/projects/vdl/plans/`)
> (revisi ronde 2). Plan ini dibuat sebelum eksekusi; scope bisa direvisi
> owner sebelum mulai.
>
> **Status eksekusi (2026-11-14):** C1 **DIBATALKAN** — audit_note ternyata
> sudah didukung compiler (schema-driven; terverifikasi via fixture). C2
> **DONE** — commit `3b0b9ae`. C5 (Phase 2) **DONE** — commits `0b98c71` +
> `a18af43`; detail di bagian C5. `cargo test --test project_test` punya 3
> failure **pre-existing** di HEAD bersih (miette line-wrap di environment
> ini, bukan regresi). Regresi vault hijau: dist byte-identical dengan config
> default, 321 file / 397 entitas. Binary baru ter-install di `~/.cargo/bin`.
> Yang tersisa: C3 (schema patch di vault) + migrasi dalil; C4 (validation
> rule co-occurrence) menunggu keputusan owner.

## 1. Konteks & Temuan Survey (grounded)

Compiler ~10.2k lines, struktur: `lexer/` → `parser/` (+ `parser/schema.rs`
untuk dokumen schema) → `validator/` → `codegen/` (markdown, compose, dot,
json, search) → `project/` (loader, manifest, build).

Temuan yang nentuin scope:

1. **Tipe entity hardcoded di lexer** (`src/lexer/mod.rs:464-495`):
   `axiom`, `artifact`, `revelation`, `synthesis`, `analogy` = keyword
   (`Token::Revelation` dll.), dipetakan di `src/lexer/token.rs:112-140`,
   dienum di `src/parser/ast.rs:50-55`. Konsekuensi: tipe baru `audit_note`
   **tidak** bisa ditambah lewat schema.vdl saja, butuh perubahan
   lexer/token/AST/parser (+validator kalau ada type-specific rule).
2. **Label type di-render hardcoded bold** (`src/codegen/compose.rs:194,236-239`):
   `format!("**{}**", label)`. Label h4 (`#### REVELATION`) akan rusak kalau
   lewat jalur ini; perlu perubahan emission.
3. **Compose generic terhadap schema** untuk sisanya: `render_entity`,
   `inline_body`, `passages` inline, heading field — semua lewat `TypeDecl`
   schema. Field baru (`original_text` dll.) dan ref baru (`notes` →
   `audit_note`) kemungkinan besar cukup lewat schema.vdl (data-driven),
   diverifikasi saat implementasi.
4. UTF-8 quoted string sudah diperbaiki (commit `7ea6329`); konten Arab
   harus ditest eksplisit (lexer + output markdown).
5. Aturan dari handoff compiler repo: baca `AGENTS.md` repo dulu, jangan
   overwrite unrelated work, wajib tests, no commit/push tanpa instruksi.

## 2. Change List

### C1. Tipe entity baru `audit_note` (compiler) — **DIBATALKAN**

Terbukti tidak diperlukan: schema documents mendukung nama type apa pun yang
dideklarasikan di schema sebagai bare-word instance (`Type "<id>"` spelling),
sesuai AGENTS.md compiler ("Domain vocabulary is forbidden in Rust"; guard
test `renaming_types_needs_no_rust_change`). Fixture `audit_note` validate
hijau di compiler existing. ~~Detail implementasi awal~~ (tersimpan sebagai
riwayat):
- `lexer/token.rs`: mapping reverse + nama.
- `parser/ast.rs`: `EntityType::AuditNote` variant + Display.
- `parser/mod.rs`: branch deklarasi entity `audit_note <name> { ... }`
  (parity dengan `revelation`: title optional, body optional).
- `validator/`: pastikan schema-driven type check menangani tipe baru
  (entity `audit_note` divalidasi terhadap `type "audit_note"` di schema).
  Kalau ada hardcoded list di rules.rs, tambah.
- Files kemungkinan kecil + terlokasi; estimasi < 200 lines termasuk tests.

### C2. Label emission heading-aware (compiler) — **DONE**

- `compose.rs` (`render_body_includes` + `labelled_body`): label mulai `#`
  di-emit verbatim, label lain tetap `**bold**` (backward compatible).
- Commit: `3b0b9ae feat(codegen): render heading-style type labels verbatim`
  (di atas `410c890`), termasuk test
  `heading_labels_render_verbatim_and_plain_labels_stay_bold`.
- `cargo test --lib`: 196 passed (incl. test baru). `cargo test
  --test project_test`: 3 failure **pre-existing** di HEAD bersih
  (miette line-wrap saat capture stderr di environment ini), bukan regresi.

### C3. Schema patch di vault (data, bukan compiler)

- `vdl/schema.vdl`: field baru di `revelation` (`original_text`,
  `transliteration`, `translation`, `narrator`, `book_section`,
  `numbering`, `grade_status`, `grade_by`, `sanad_status`, `notes` →
  ref `audit_note`), type baru `audit_note`, dan `audit_note` masuk
  allowed inline types untuk `passages` (ref list di type
  `artifact`/`axiom`/`revelation`).
- Schema label: `[REVELATION]` → `#### REVELATION` dst. per proposal.
- **Urutan: setelah C1+C2 selesai dan ter-install**, karena schema baru
  butuh compiler baru buat validate.

### C4. Optional validation rule (Phase 1.5, minta approval dulu)

- Rule co-occurrence: kalau `original_text` terisi, `transliteration` +
  `translation` wajib; kalau `sanad_status` = "verified", `grade_status`
  wajib untuk hadith. Implementasi di `validator/rules.rs`.
- Ini tighten yang bikin data module gak setengah jalan; skip kalau mau
  minimal.

### C5. Phase 2: render mode cites (compiler) — **DONE**

Dikerjakan setelah approval owner desain (penempatan sitasi setelah label,
default `none`, short pakai heading field target). Implementasi:

- Flag schema baru `cite` di ref field: lexer `Token::Cite`, parser modifier,
  `FieldDecl.cite`, validator rule **V16** (`cite` hanya sah di ref field,
  parser permissive + satu diagnostic) + test `v16_*` + row README.
- `ComposeOptions { cites, number_headings }` + `compose_with_options`:
  mode **none** (default, byte-identical), **link** (ID target), **short**
  (heading-field text target, fallback ID). Baris sitasi format
  `**Cites:** ...`, ditempatkan setelah label, sebelum body.
- Inline-rendered entities kini juga mengeluarkan text field yang diisi
  sebagai metadata (`**auditor:** sanad-auditor`) antara label dan body —
  menutup temuan auditor/date yang hilang di compose. Root tidak pernah
  emit metadata (text field root sudah ada di frontmatter).
- Manifest: per-target `cites = "none|link|short"` di `vdl.toml` (di-normalize
  + reject nilai tak dikenal); CLI compose `--cites` sebagai override.
- Commits: `0b98c71` (schema flag + V16), `a18af43` (render modes + metadata).
- Tests: 203 lib passed (incl. 8 test baru), schema_test 13 passed;
  3 failure project_test tetap pre-existing (miette wrap).
- Regresi vault: dist DA + Human OS **byte-identical** dengan config default;
  validate 321 file / 397 entitas hijau. Live demo fixture: short/link/none
  sesuai desain.

## 3. Test & Verification Strategy

1. Unit tests in-file mengikuti gaya existing (compose.rs punya contoh
   pola test + inline fixtures): lexer token, parser entity baru, label
   emission, schema validation `audit_note`.
2. E2E fixture: 3 file sample dari proposal (quran 30/30, muslim/2653,
   bukhari/7471) sebagai fixture test di compiler repo + `tests/` integration.
3. Full pipeline di vault setelah install binary baru:
   `cargo test` (repo) → `cargo build --release` → install ke
   `~/.cargo/bin` → `vdl validate .` (vault, harus tetap green di 321
   file existing) → patch schema (C3) → validate lagi → compose sample
   dalil → spot-check output markdown (`#### REVELATION`, arab RTL raw,
   audit_note ter-render).
4. Regresi: dist Divine Alignment & Human OS di-diff sebelum/sesudah,
   perubahan hanya di label blok (bold → h4) di DA; Human OS gak berubah.

## 4. Urutan Kerja & Commit

1. Baca `AGENTS.md` + working tree `vld-compiler` (wajib per handoff).
2. C1 (lexer/parser/AST/validator) + tests → commit "feat(lexer/parser): add audit_note entity type".
3. C2 (label emission) + tests → commit "feat(codegen): heading-aware type labels".
4. Build + install binary → C3 schema patch di vault + validate + compose sample → commit vault "vdl: dalil module schema + label migration".
5. C4 optional kalau di-approve → commit terpisah.
6. Update `vdl/README.md` + proposal status draft → accepted.
- Tidak push tanpa instruksi eksplisit. Commit per module sesuai preferensi owner.

## 5. Risiko & Catatan

- **Validator type-specific rules**: survey belum bedah 608 lines
  `validator/rules.rs` habis; kalau ada list hardcoded tipe, ditambah di
  C1. Dicek pertama kali saat implementasi.
- **RTL markdown**: output markdown raw text Arab; viewer biasanya
  handle RTL per paragraph. Kalau hasil render jelek, opsi nanti bungkus
  Arab di `<div dir="rtl">` di level renderer (Phase 2, bukan slice ini).
- **`vdl diff` / versioning**: gak disentuh; perlu kalau nanti tracking
  evolusi dalil, tapi out of scope.
- **Backward compat**: label legacy `[REVELATION]` di schema DA existing
  tetap bold (C2 fallback), jadi dist existing gak rusak sebelum C3.

## 6. Definition of Done

- `cargo test` di `vld-compiler` hijau (unit + e2e fixture dalil).
- Binary baru ter-install; `vdl validate .` di vault hijau dengan schema
  baru + 3 sample dalil valid.
- Compose sample menghasilkan `#### REVELATION` + `#### AUDIT NOTE`
  di output markdown.
- Dist DA/Human OS ter-regenerate tanpa regresi lain.
- Docs (compiler README/AGENTS.md, `vdl/README.md`) diupdate bila
  architecture berubah.
