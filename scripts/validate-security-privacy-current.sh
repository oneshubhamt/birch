#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "[security-privacy-current] current publication/lifetime contracts"
node --test \
  tests/js/navigation-publication-contract.test.mjs \
  tests/js/note-local-privacy-contract.test.mjs \
  tests/js/multi-note-privacy-contract.test.mjs \
  tests/js/library-wide-privacy-contract.test.mjs \
  tests/js/vault-action-privacy-contract.test.mjs

echo "[security-privacy-current] current residue/ownership closure contracts"
node --test \
  tests/js/vault-no-vacuum-forensic-contract.test.mjs \
  tests/js/vault-cleanup-2b42-9-contract.test.mjs \
  tests/js/vault-independent-closure-2b42-10-contract.test.mjs

cat <<'EOF'
Native CI is the behavioral authority for:
- rustnote-vault crypto/context/tamper/streaming/KDF tests;
- PrivacyCoordinator publication, admission, revocation and concurrency tests;
- VaultRuntime materialization/revocation lifetime tests;
- typed/durable plaintext ownership and fail-closed filesystem tests;
- Protect/Lock/Restore residue, WAL/FTS, backup and crash-recovery tests;
- staging/attachment/backup repository restart and ownership recovery tests.
EOF

echo "Current security/privacy source-contract suite passed."
