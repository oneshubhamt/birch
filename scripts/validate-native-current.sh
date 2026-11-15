#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Current native source coverage is intentionally concern-based rather than
# historical-pass-based. Compilation/lints/tests are the native macOS CI job;
# this fast source-side gate covers pinned toolchain/deployment configuration
# plus the maintained current browser/privacy/storage contract corpus.
echo "[native-current] pinned toolchain and deployment contracts"
bash scripts/validate-dioxus-0.7.10-baseline.sh
bash scripts/validate-macos-deployment-target.sh

echo "[native-current] no executable native-history document dependencies"
python3 - <<'PY'
from pathlib import Path
import re

roots = [
    Path(".github"),
    Path("scripts"),
    Path("tests"),
    Path("src"),
    Path("crates"),
    Path("tools"),
    Path("editor-src"),
    Path("pdf-src"),
]
patterns = (
    re.compile(r"NATIVE_COMPILE_2B(?:13|14|15|16|17|18|19|20|21|24|25|26|27|28|29|30|31|32|33|34|35|36|37|38|39|40|41)_(?:PLAN|IMPLEMENTATION)\\.md"),
    re.compile(r"QA_NATIVE_COMPILE_2B(?:13|14|15|16|17|18|19|20|21|24|25|26|27|28|29|30|31|32|33|34|35|36|37|38|39|40|41)\\.md"),
)
allowed_suffixes = {".sh", ".py", ".js", ".mjs", ".rs", ".toml", ".yml", ".yaml"}
self_path = Path("scripts/validate-native-current.sh")

violations = []
for root in roots:
    if not root.exists():
        continue
    for path in root.rglob("*"):
        if not path.is_file() or path == self_path or path.suffix not in allowed_suffixes:
            continue
        text = path.read_text(errors="ignore")
        if any(pattern.search(text) for pattern in patterns):
            violations.append(path.as_posix())

if violations:
    raise SystemExit(
        "native-history Markdown dependency reintroduced in executable/source surfaces:\n- "
        + "\n- ".join(sorted(set(violations)))
    )

for path in Path(".").glob("NATIVE_COMPILE_2B*_PLAN.md"):
    raise SystemExit(f"deleted native plan fixture returned: {path}")
for path in Path(".").glob("NATIVE_COMPILE_2B*_IMPLEMENTATION.md"):
    raise SystemExit(f"deleted native implementation fixture returned: {path}")
PY

echo "[native-current] maintained current behavior/source contracts"
node scripts/run-browser-behavior-tests.mjs

echo "Current native source/invariant suite passed."
