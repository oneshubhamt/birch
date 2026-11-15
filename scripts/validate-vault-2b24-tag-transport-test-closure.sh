#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
root = Path('.')
tags = (root/'src/storage/tags.rs').read_text()
transport = (root/'src/storage/transport.rs').read_text()
protection = (root/'src/storage/protection.rs').read_text()

# Global tag mutation must have an ordinary serialized branch for the empty
# protected-note set while preserving the protected complete-set path.
helper = tags[tags.index('fn begin_all_protected_notes_tag_mutation'):tags.index('fn refresh_notes_after_tag_mutation')]
for marker in (
    'if before.is_empty()',
    'TagMutationGuard::Ordinary {',
    '_storage: self.lock_operations()?',
    'TagMutationGuard::Protected(',
    'self.begin_protected_notes_mutation(&before)?',
    'if before == after',
):
    assert marker in helper, marker
assert helper.index('TagMutationGuard::Ordinary {') < helper.index('let after =')
assert helper.index('let after =') < helper.index('if before == after')

# Existing ordinary tag regressions remain present and therefore exercise the
# production branch on the native suite.
for name in (
    'fn rename_tag_updates_every_note_and_search_index()',
    'fn rename_tag_to_existing_name_merges_without_duplicates()',
    'fn delete_tag_removes_only_the_tag_relationships()',
):
    assert name in tags, name

# Transport lifetime test must use the nonblocking Vault-state probe and may
# not rematerialize a transport attachment while Lock is admitted.
name='fn protected_pdf_transport_resource_keeps_lock_waiting_until_request_releases_it()'
start=transport.index(name)
tail=transport[start:]
try:
    end=tail.index('\n    #[test]', len(name))
except ValueError:
    end=len(tail)
body=tail[:end]
for marker in (
    'protected_note_accepts_vault_reads_for_test(&storage, &note.id)',
    'done_rx.try_recv().is_err()',
    'drop(resource);',
    'recv_timeout(Duration::from_secs(15))',
):
    assert marker in body, marker
probe_start=body.index('started_rx.recv().expect("lock started")')
drop_pos=body.index('drop(resource);')
assert 'transport.attachment_resource(' not in body[probe_start:drop_pos]

# Metadata-locality test gets diagnostic messages only; its core locality and
# plaintext-shadow assertions must remain intact.
metadata = protection[protection.index('fn protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only'):protection.index('fn vault_manifest_swap_is_rollback_safe_and_missing_component_fails_closed')]
for marker in (
    'source mutation must not rewrite NotePrivateState',
    'citation mutation must rewrite MetadataState',
    'PDF annotation mutation must rewrite MetadataState',
    'document link mutation must rewrite MetadataState outgoing links',
    'protected sources must not create plaintext note_sources rows',
    'protected note links must not create plaintext note_links rows',
):
    assert marker in metadata, marker

print('Rustnote 2b.24 tag/transport test closure guard passed.')
PY

bash scripts/validate-vault-2b23-attachment-roundtrip-lifetime-closure.sh
