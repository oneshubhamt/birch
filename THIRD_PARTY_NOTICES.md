# Third-party notices

Rustnote uses direct ProseMirror modules, including the official `prosemirror-tables` module, and esbuild under their respective MIT licences. Exact package names and versions are pinned in `editor-build/package.json` and recorded in the generated `assets/prosemirror-editor-manifest.json`.

The scientific-notation editor vendors:

- **KaTeX 0.16.27**, distributed under the MIT License. The complete notice is in `editor-src/vendor/katex-LICENSE.txt`.
- **KaTeX fonts**, distributed under the SIL Open Font License 1.1. The complete notice is in `editor-src/vendor/katex-fonts-OFL.txt`.
- **mhchemParser 4.2.1**, distributed under the Apache License 2.0. The complete notice is in `editor-src/vendor/mhchem-parser-LICENSE.txt`.

Rustnote's scientific node/view design was informed by the MIT-licensed `prosemirror-math` project. Rustnote owns its integrated implementation and does not install a ProseMirror wrapper.

Rustnote does not include Tiptap or another ProseMirror wrapper.

Rustnote's optional in-app PDF preview currently uses **Mozilla PDF.js 5.7.284 legacy** under the Apache License 2.0. Desktop preparation downloads the official `pdfjs-dist@5.7.284` npm package, verifies the pinned SHA-512, extracts the matching `legacy/build/pdf.mjs`, `legacy/web/pdf_viewer.mjs`, and `legacy/build/pdf.worker.mjs` inputs, and bundles them with Rustnote's viewer using the project's pinned esbuild with a conservative Safari 14 syntax target. The display/viewer/worker inputs therefore remain version-matched while WKWebView never parses the raw upstream modules directly. Matching standard-font and WASM decoder assets are served through Rustnote's private PDF protocol. Generated PDF.js build inputs, cache, and prepared runtime live under `target/rustnote-pdfjs/`; the upstream Apache notice is materialized as `assets/pdfjs/PDFJS-LICENSE.txt` by the preparation script.

**Security boundary.** PDF.js 5.7.284 is in the affected range for CVE-2026-16633 / GHSA-hq66-cqwq-w95j. Rustnote uses the upstream-documented scripting workaround as an enforced application invariant: PDF scripting is disabled. Rustnote additionally disables XFA and PDF.js eval support, does not instantiate a scripting manager, and runs the preview under a restrictive CSP without `unsafe-inline` or `unsafe-eval`. The prepared-runtime validator checks these controls. The single 5.7.284 legacy runtime is a compatibility-development baseline; a patched modern runtime can be reintroduced behind a proven WebKit capability gate after the PDF performance architecture is stabilized.

Rustnote uses **imagesize 0.15.0** (MIT) to read image dimensions from attachment headers without decoding full image pixel data.

Rustnote's isolated `rustnote-vault` security crate uses:

- **aegis 0.9.15** for AEGIS-256 authenticated encryption. Rustnote v1 encrypted objects use the AEGIS-256 variant with a 256-bit authentication tag.
- **argon2 0.5.3** (RustCrypto, MIT OR Apache-2.0) for Argon2id password-based key derivation. Password-derived material wraps random Rustnote scope keys; it does not directly encrypt note/attachment payloads.
- **getrandom 0.3.4** to obtain cryptographically secure randomness from the operating system for keys, nonces, and salts.
- **zeroize 1.9.0** (MIT OR Apache-2.0) for best-effort zeroization of owned secret key material and sensitive temporary buffers.

These dependencies are isolated from the UI, editor, SQLite, FTS5, and attachment persistence layers by `crates/rustnote-vault`.
