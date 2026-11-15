# Historical QA evidence — consolidated ledger

**Baseline archive:** `514b797fb0a6a128050cf75ac2428fc12095106a` · **Reports reviewed:** 104/104.

This document consolidates the historical `QA_*.md` corpus without promoting old PASS statements into current certification. The exact original bytes remain permanently inspectable through the immutable baseline links in the ledger below. Current QA policy and current commands live in [../QA.md](../QA.md).

## Evidence rule

A historical QA report answers only:

1. which revision/pass it described;
2. which commands/checks it says were executed;
3. what result it recorded; and
4. what it explicitly did **not** execute.

It does not prove the same property for the present branch. Source/static validators are useful regression guards, but they are not interchangeable with compiled Rust tests, native macOS/Keychain behavior, crash/restart testing, or manual UI/system integration.

## Historical command families preserved

Across the reports, the recurring native release sequence was:

```sh
cargo fmt --all
cargo fmt --all -- --check
./scripts/prepare-rustnote-mac.sh
./scripts/validate-prepared-runtime.sh
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1
```

Some earlier passes used narrower variants such as `cargo test -p rustnote-vault --locked`, `cargo metadata --locked --format-version 1`, or omitted `--all-features`. Those exact variants remain in the immutable original reports.

The recurring source/static security and Vault guards were:

```sh
./scripts/validate-security-e0.sh
./scripts/validate-security-e1.sh
./scripts/validate-security-e2.sh
./scripts/validate-security-e3.sh
./scripts/validate-security-e0-e3-closure.sh
./scripts/validate-vault-2b1-foundation.sh
...
./scripts/validate-vault-2b42-final-pre-s1-closure.sh
```

Many E0–E3/architecture reports used documented `RUSTNOTE_*_STATIC_ONLY=1` modes when Cargo or platform dependencies were unavailable. A static-only PASS is preserved as **static-only evidence**, not native execution.

Browser/editor/PDF evidence commonly used:

```sh
node scripts/run-browser-behavior-tests.mjs
node scripts/run-browser-behavior-tests.mjs --require-editor-deps
```

The dependency-free suite grew over time, so counts such as 19/19, 20/20, 36/36, 85/85, 91/91, 93/93, 105/105 and 111/111 describe **different historical suite sizes** and should not be compared as if they were the same test set.

The backup-contract sequence used:

```sh
bash scripts/validate-backup-2b43-1-key-contract.sh
bash scripts/validate-backup-2b43-2-byte-format.sh
bash scripts/validate-backup-2b43-3-secure-key-store.sh
```

## Thematic evidence summary

### Architecture A0–B6

The architecture records are predominantly **structural/source validation plus manual behavioral checklists**. They trace the move to typed editor bridges, modular ProseMirror/PDF sources, explicit library/storage boundaries, application-owned execution, shared editor-document specification, workflow orchestration, behavioral-test decomposition, typed errors and search translation.

The reports repeatedly require manual or native checks for editor initialization, attachment import/recovery, media playback, PDF behavior, Settings/restore operations and macOS preparation. Several static-only paths explicitly say they are not substitutes for a development-Mac run.

### E0–E3 security series

The E0–E3 reports consistently distinguish static/source closure from native Rust execution. Earlier packaging environments often lacked `cargo`, `rustc` and `rustfmt`; those reports explicitly leave formatting, compilation, strict Clippy, Rust tests and sometimes `cargo audit` or representative-device crypto benchmarking as external gates.

The sequence records progressively stronger checks for authenticated crypto formats, selective encryption, SQLite/FTS/WAL cleanup, Lock read-lifetime behavior, crash-journal recovery, canonical plaintext ownership, offline backup privacy obligations, UI/async privacy lifetime and adversarial closure.

Important historical caveat: a report saying a Rust regression was **present in source** is not evidence that the regression executed. Many reports say this explicitly.

### Vault 2b / 2b.42

Vault reports record source/static closure of I1–I7, granular object persistence, `PrivacyCoordinator`, typed plaintext ownership, publication tokens, protected revision indexing, external backup ownership and scoped publication.

Most passes again separate assistant-side source/static validation from native macOS compilation/tests. The notable exception is the **2b.42.8 no-VACUUM experiment**, whose report records a native macOS forensic test with **1 passed, 0 failed, 290 filtered out**, plus a source validator PASS. That experiment supported the later production decision to use `secure_delete + FTS rebuild + verified WAL TRUNCATE` without per-Protect `VACUUM`; the report still required final native closure of the canonicalized tree.

2b.42.9 records dependency-free/static cleanup closure (including 105/105 browser/editor/PDF tests) but still marks native Rust closure pending.

### Native-compile 2b.13–2b.41

These 29 reports are **historical pass-by-pass compile/Clippy/test repair records**, not one coherent current release certificate. Many were written in a packaging environment without Cargo and explicitly defer authoritative native compile/Clippy/test evidence to macOS; others record targeted source/browser/validator results associated with defects found by native runs.

This series should be read chronologically, not as 29 independent current requirements. Batch 5 merged it with the paired plan/implementation history in [Native compile and toolchain evolution](../development/NATIVE_TOOLCHAIN_HISTORY.md).

### Editor/list/image/PDF/platform records

The editor/list reports preserve behavior decisions around native ProseMirror list ownership, list-exit indentation, list-boundary continuity, image paste/drop staging and paragraph UI. Dependency-free JS suites passed in the reported environments; dependency-backed ProseMirror tests were often deferred because `editor-build/node_modules` was intentionally absent.

PDF records preserve JS runtime/layout/integrity checks while explicitly noting that Rust protocol tests and live macOS WKWebView preview could not run in the packaging environment.

The macOS packaging record establishes the 12.0 deployment-target contract but states that source/plist configuration alone is insufficient; the built Mach-O must be inspected.

### Backup 2b.43

- **2b.43.1:** reported **12/12** key-authority contract PASS; contract/audit only.
- **2b.43.2:** reported **18/18** byte-format validator PASS and **111/111** dependency-free browser/source behavior; contract-only, no production reader/writer.
- **2b.43.3:** reports secure-store/key-authority source/API objectives as PASS while explicitly keeping managed-backup storage on the old writer and leaving the corrected native macOS compile/Clippy/test rerun as a gate before 2b.43.4.

These records must not be interpreted as proof that encrypted managed-backup v3 persistence is deployed.

## Retention policy

Exactly **35** QA reports remain in the working tree because current validator scripts read their filenames/content directly. That is a QA-architecture dependency, not a documentation preference. The other **69** reports are safe to remove from the working tree after this consolidation because their exact originals are retained in Git and no current script/test requires them.

The documentation-consolidation project does **not** redesign those validators. The remaining report dependencies are recorded in the [archive/no-loss map](../ARCHIVE.md) and belong to the separate QA-refinement work.

## Complete 104-report ledger

| Report | Theme | Evidence classification | Working-tree disposition |
| --- | --- | --- | --- |
| [`QA_ARCHITECTURE_2A5_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_2A5_CLOSURE.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A0.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A0.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A1.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A1.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A1_A4_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A1_A4_CLOSURE.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A2.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A2.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A3.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A3.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A4.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A4.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A5.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A5.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A6.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_A7.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A7.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_B1.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B1.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_B2.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B2.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_B3.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B3.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_B4.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B4.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_B5.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B5.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_ARCHITECTURE_B6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B6.md) | Architecture | Primarily source/static architecture guard + manual regression checklist; some JS behavioral coverage | Git-only after Batch 4 |
| [`QA_BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md) | Backup | Contract/static format or key-authority QA | **Retained temporarily — validator reads exact file** |
| [`QA_BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md) | Backup | Contract/static format or key-authority QA | Git-only after Batch 4 |
| [`QA_BACKUP_2B43_3_SECURE_KEY_STORE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_BACKUP_2B43_3_SECURE_KEY_STORE.md) | Backup | Secure-store/key-authority source/API QA; native rerun and v3 storage integration remain separate | Git-only after Batch 4 |
| [`QA_DIOXUS_0_7_10_BASELINE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_DIOXUS_0_7_10_BASELINE.md) | Platform/baseline | Version/configuration/platform packaging baseline checks | Git-only after Batch 4 |
| [`QA_IMAGE_PASTE_STAGING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_IMAGE_PASTE_STAGING.md) | Editor/UI | Dependency-free browser/static behavior + manual/editor checks; dependency-backed ProseMirror often deferred | Git-only after Batch 4 |
| [`QA_LIST_BACKSPACE_NATIVE_LIFT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_BACKSPACE_NATIVE_LIFT.md) | Editor/UI | Dependency-free browser/static behavior + manual/editor checks; dependency-backed ProseMirror often deferred | Git-only after Batch 4 |
| [`QA_LIST_BOUNDARY_CONTINUITY.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_BOUNDARY_CONTINUITY.md) | Editor/UI | Dependency-free browser/static behavior + manual/editor checks; dependency-backed ProseMirror often deferred | Git-only after Batch 4 |
| [`QA_LIST_EXIT_INDENT_RESET.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_EXIT_INDENT_RESET.md) | Editor/UI | Dependency-free browser/static behavior + manual/editor checks; dependency-backed ProseMirror often deferred | Git-only after Batch 4 |
| [`QA_LIST_NATIVE_SIMPLIFICATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_NATIVE_SIMPLIFICATION.md) | Editor/UI | Dependency-free browser/static behavior + manual/editor checks; dependency-backed ProseMirror often deferred | Git-only after Batch 4 |
| [`QA_MACOS_APP_PACKAGING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_MACOS_APP_PACKAGING.md) | Platform/baseline | Version/configuration/platform packaging baseline checks | Git-only after Batch 4 |
| [`QA_NATIVE_COMPILE_2B13.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B13.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B14.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B14.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B15.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B15.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B16.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B16.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B17.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B17.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B18.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B18.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B19.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B19.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B20.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B20.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B21.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B21.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B22.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B22.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | Git-only after Batch 4 |
| [`QA_NATIVE_COMPILE_2B23.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B23.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | Git-only after Batch 4 |
| [`QA_NATIVE_COMPILE_2B24.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B24.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B25.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B25.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B26.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B26.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B27.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B27.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B28.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B28.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B29.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B29.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B30.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B30.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B31.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B31.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B32.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B32.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B33.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B33.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B34.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B34.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B35.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B35.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B36.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B36.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B37.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B37.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B38.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B38.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B39.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B39.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B40.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B40.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_NATIVE_COMPILE_2B41.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B41.md) | Native compile | Pass-local compile/Clippy/test closure record; many reports distinguish packaging/static evidence from authoritative Mac execution | **Retained temporarily — validator reads exact file** |
| [`QA_PARAGRAPH_UI_REFINEMENT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_PARAGRAPH_UI_REFINEMENT.md) | Editor/UI | Dependency-free browser/static behavior + manual/editor checks; dependency-backed ProseMirror often deferred | Git-only after Batch 4 |
| [`QA_PDF_RUNTIME_INTEGRITY.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_PDF_RUNTIME_INTEGRITY.md) | PDF | JS/PDF behavior and runtime-integrity checks; Rust/WKWebView execution environment-dependent | Git-only after Batch 4 |
| [`QA_PDF_VIEWER_UI_RESTORATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_PDF_VIEWER_UI_RESTORATION.md) | PDF | JS/PDF behavior and runtime-integrity checks; Rust/WKWebView execution environment-dependent | Git-only after Batch 4 |
| [`QA_SECURITY_E0.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A10.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A10.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A11.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A11.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A12.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A12.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A13.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A13.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A14.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A14.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A19.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A19.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | **Retained temporarily — validator reads exact file** |
| [`QA_SECURITY_E0_E3_CLOSURE_2A6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A6.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A7.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A7.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A8.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A8.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_CLOSURE_2A9.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A9.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A15.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A15.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A16.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A16.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A17.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A17.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | **Retained temporarily — validator reads exact file** |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A18.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A18.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | **Retained temporarily — validator reads exact file** |
| [`QA_SECURITY_E1.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E1.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E2.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E2.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_SECURITY_E3.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E3.md) | Security/E0–E3 | Static/source security closure plus declared Rust regressions; native Rust often explicitly deferred | Git-only after Batch 4 |
| [`QA_VAULT_2B10_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B10_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B11_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B11_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | **Retained temporarily — validator reads exact file** |
| [`QA_VAULT_2B12_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B12_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | **Retained temporarily — validator reads exact file** |
| [`QA_VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B1_FOUNDATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B1_FOUNDATION.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B2_PRIVACY_AUTHORITY.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B2_PRIVACY_AUTHORITY.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B3_PUBLICATION_TOKEN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B3_PUBLICATION_TOKEN.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_10_INDEPENDENT_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_10_INDEPENDENT_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_11_FINAL_QA_PACKAGE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_11_FINAL_QA_PACKAGE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_1_6_POST_AUDIT_REPAIR.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_1_6_POST_AUDIT_REPAIR.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_3_NOTE_LOCAL_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_3_NOTE_LOCAL_UI.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_4_MULTI_NOTE_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_4_MULTI_NOTE_UI.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_7_NATIVE_COMPILE_FIX.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_7_NATIVE_COMPILE_FIX.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_7_PROFILING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_7_PROFILING.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B42_8_NO_VACUUM_FORENSIC.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_8_NO_VACUUM_FORENSIC.md) | Vault | Source/static + reported native macOS forensic regression | **Retained temporarily — validator reads exact file** |
| [`QA_VAULT_2B42_9_CLEANUP.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_9_CLEANUP.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | **Retained temporarily — validator reads exact file** |
| [`QA_VAULT_2B4_GRANULAR_CUTOVER.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B4_GRANULAR_CUTOVER.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B5_CONSOLIDATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B5_CONSOLIDATION.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B6_INDEPENDENT_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B6_INDEPENDENT_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B7_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B7_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B8_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B8_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VAULT_2B9_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B9_CLOSURE.md) | Vault | Static/source/adversarial Vault closure; browser tests where applicable; native Rust usually a separate gate | Git-only after Batch 4 |
| [`QA_VERSION_V0.12.6_BUILD_2B43.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VERSION_V0.12.6_BUILD_2B43.md) | Platform/baseline | Version/configuration/platform packaging baseline checks | Git-only after Batch 4 |

