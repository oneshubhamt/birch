# QA — Rustnote Vault 2b.3 Publication-token consolidation

## Scope

QA targets only the surfaces changed by 2b.3 plus inherited security/application regressions. The granular encrypted-object cutover remains outside scope.

## Static/security validation

Final executable results in this environment:

- `validate-security-e0.sh` static-only: **PASS**;
- `validate-security-e1.sh` static-only: **PASS**;
- `validate-security-e2.sh` static-only: **PASS**;
- `validate-security-e3.sh` static-only: **PASS**;
- `validate-security-e0-e3-ownership-2a16.sh`: **PASS**;
- `validate-security-e0-e3-residue-2a17.sh`: **PASS**;
- `validate-security-e0-e3-ui-lifetime-2a18.sh`: **PASS** with 2b.3 token assertions replacing superseded manual generation markers;
- `validate-security-e0-e3-adversarial-closure-2a19.sh`: **PASS** with equivalent/stronger token assertions;
- `validate-vault-2b1-foundation.sh`: **PASS**;
- `validate-vault-2b2-privacy-authority.sh`: **PASS**;
- `validate-vault-2b3-publication-token.sh`: **PASS**.

The 2b.3 guard rejects feature-level reintroduction of `privacy_epoch.current()/is_current()` publication choreography and rejects predictive “next generation” token schemes.

## Source-shape and call-site QA

- lexical delimiter scan: **PASS — 37 present changed/added Rust files; 1 intentionally removed Rust file (`src/portability/rtf.rs`)**;
- added-line trailing-whitespace scan: **PASS**;
- changed-Rust hygiene (`TODO`, `FIXME`, `HACK`, `dbg!`, `todo!`, `unimplemented!`): **PASS**;
- changed async/free-function signature arity audit: **PASS**, including all 17 direct `synchronize_current_editor` call sites;
- baseline-integrity sweep: **PASS — no 2b.2 source file removed except the explicitly requested deletion of `src/portability/rtf.rs`**.

No native Rust parser/compiler is available in this container; these checks are intentionally supplemental, not substitutes for the native release gate.

## Dependency-free regression QA

- shell syntax using `bash -n`: **PASS — 36/36 scripts**;
- all first-party JS/MJS syntax using `node --check`: **PASS — 45/45 files**;
- canonical editor-document specification: **PASS — 21 nodes / 14 marks**;
- editor-document contract corpus: **PASS — 36 cases**;
- dependency-free browser/editor/PDF behavior suite: **PASS — 36/36 tests**.

## Differential architecture QA versus untouched 2b.2

The broad A/B architecture probes were run on both the untouched 2b.2 baseline and the 2b.3 tree with the same available tools.

Equivalent results:

- A1-A5 and A7 static modes: **PASS on baseline and 2b.3**;
- A0: both stop because Cargo is unavailable;
- A6, B1 and B6: both reproduce the same pre-existing Node/static failures;
- B2, B4 and B5: both reach the same unavailable-`zsh`/chained-validator limitation;
- B3 initially exposed a stale validator marker: it looked specifically for raw `document::eval(&update_script)`. 2b.3 routes that await through `publish_editor_bool(...)` to obtain a publication lease. After updating the guard to recognize the helper while retaining finalizer-before-browser-await ordering, B3's own workflow checks pass and it then reaches the same missing-`zsh` limitation as baseline.

No new broad architecture-validator regression remains attributable to 2b.3.

## Targeted adversarial coverage added/required

Native Rust tests (present in source; execution deferred to native toolchain):

- `publication_authorization_is_invalidated_on_note_admission_and_cannot_refresh_mid_transition`;
- `publication_authorization_is_invalidated_on_library_admission`;
- `admitted_transition_drains_started_publication_lease_before_advancing`;
- `lock_invalidates_publication_and_reader_authorization_before_reader_drain_completes`;
- `protected_note_export_resource_holds_lock_until_native_lifetime_ends`;
- `ordinary_note_export_resource_blocks_encrypt_until_native_lifetime_ends`.

Existing attachment/revision lifetime tests remain required by inherited validators.

## Manual omission sweeps

The final source audit covered:

- `ApplicationExecutor` results that can publish `Note`, protected title/body/metadata, revision projections, sources/citations, attachment metadata or note-link metadata;
- async WebView handoffs carrying protected-derived editor state or registries;
- autosave result publication;
- native Open/Reveal/Save-As/PDF print lifetimes;
- whole-library restore publication;
- raw feature-level privacy-generation comparisons;
- direct coordinator type leakage into feature UI;
- obsolete/predictive token APIs.

Ordinary-only async UI operations (layout/preferences, notebook chrome, responsive behavior, etc.) intentionally remain outside the privacy-token mechanism.

## Native Rust toolchain intentionally not run

`cargo`, `rustc`, and `rustfmt` are absent from this execution environment. The following remain release-blocking on the development Mac:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
```

Then rerun the security validators including `validate-vault-2b3-publication-token.sh`.

## Final QA verdict

All static/security, structural, dependency-free behavioral and differential checks executable in this environment are green after the final source fixes. The source is ready for final packaging; native Rust compilation/Clippy/tests remain the explicit development-Mac release gate above.

## RTF import/export removal QA

The explicit final-scope deletion was audited independently of the publication-token work:

- `src/portability/rtf.rs`: **deleted**;
- canonical portability enum: **no RTF variant**;
- `.rtf` import recognition: **absent**;
- RTF import inspection/counting: **absent**;
- RTF native file-dialog filter: **absent**;
- RTF export filter/choice: **absent**;
- RTF-specific editor contract dependency: **absent**;
- repository source/test sweep for explicit RTF capability markers: **clean**.

`validate-vault-2b3-publication-token.sh` now makes this removal a release invariant, so RTF support cannot silently return through an old portability branch.

## Final package integrity

The final source tree is packaged only after the closure checks above. Packaging verification requires:

- re-extracting the ZIP into a clean directory;
- exact file-set equality with the audited source tree;
- byte-for-byte equality for every packaged file;
- rerunning the 2b.3 and 2b.2 Vault validators from the extracted tree;
- rerunning inherited 2a.18/2a.19 privacy-lifetime/adversarial guards from the extracted tree;
- rerunning the 36-case editor-document contract and 36-test dependency-free browser/editor/PDF suite from the extracted tree;
- confirming the extracted production/test tree contains no RTF capability marker.

These package-level checks are part of 2b.3 closure; only the native Rust toolchain gate remains external to this container.
