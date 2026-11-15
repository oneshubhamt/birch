#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

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
allowed_suffixes = {".sh", ".py", ".js", ".mjs", ".rs", ".toml", ".yml", ".yaml"}
self_path = Path("scripts/validate-qa-report-decoupling.sh")
pattern = re.compile(r"QA_[A-Z0-9_]+\.md")

violations = []
for root in roots:
    if not root.exists():
        continue
    for path in root.rglob("*"):
        if not path.is_file() or path == self_path or path.suffix not in allowed_suffixes:
            continue
        text = path.read_text(errors="ignore")
        refs = sorted(set(pattern.findall(text)))
        if refs:
            violations.append((path.as_posix(), refs))

if violations:
    lines = []
    for path, refs in violations:
        lines.append(f"{path}: {', '.join(refs)}")
    raise SystemExit(
        "QA report dependency remains in executable/source surfaces:\n- "
        + "\n- ".join(lines)
    )

print("QA report executable-dependency guard passed.")
PY
