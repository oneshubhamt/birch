#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path

editor = Path('src/ui/editor.rs').read_text()
protocol = Path('src/attachment_protocol.rs').read_text()

def require(text, marker, label):
    if marker not in text:
        raise SystemExit(f'{label} missing: {marker}')

# The two reusable FnMut editor handlers must clone the publication token per invocation
# before moving it into an async future.
needle = '''let editor = editor.clone();\n                            let publication_token = publication_token.clone();\n                            spawn(async move {'''
if editor.count(needle) < 2:
    raise SystemExit('editor onchange/oncommit publication-token per-invocation clones are incomplete')

# Handler-local ApplicationServices ownership prevents consuming the component-level handle.
require(
    editor,
    '''let publication_token = editor_publication_token.clone();\n                        let services = services.clone();\n                        EventHandler::new(move |command: EditorAttachmentCommand| {''',
    'attachment-command services ownership',
)
require(
    editor,
    '''let attachment_service = attachment_service.clone();\n                                    let workflow_service = workflow_service.clone();\n                                    let services = services.clone();\n                                    move |request: NoteRevisionRestoreRequest| {''',
    'revision-restore services ownership',
)

# Later render-tree consumers must still use the component-level service handle.
for marker in ('let settings = services.settings();', 'library: services.library(),'):
    require(editor, marker, 'post-handler services availability')

require(
    protocol,
    '''return attachment_json_error(\n                        400,\n                        "Image staging has an invalid note identity",\n                    );''',
    'native rustfmt image-staging formatting',
)

print('Rustnote 2b.14 editor native capture-closure static guard passed.')
PY

bash scripts/validate-vault-2b13-native-compile-closure.sh
bash scripts/validate-vault-2b12-aegis-privacy-disclosure-closure.sh
bash scripts/validate-vault-2b11-atomic-privacy-authority-closure.sh
bash scripts/validate-security-e0-e3-ownership-2a16.sh
bash scripts/validate-security-e0-e3-residue-2a17.sh
bash scripts/validate-security-e0-e3-ui-lifetime-2a18.sh
bash scripts/validate-security-e0-e3-adversarial-closure-2a19.sh
