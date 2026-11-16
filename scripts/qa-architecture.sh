#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "[qa-architecture] shell syntax"
while IFS= read -r -d '' script; do
  bash -n "$script"
done < <(find scripts -type f -name '*.sh' -print0)

echo "[qa-architecture] JavaScript syntax"
while IFS= read -r -d '' file; do
  node --check "$file" >/dev/null
done < <(find . -type f \( -name '*.js' -o -name '*.mjs' \) \
  -not -path './target/*' -not -path './node_modules/*' \
  -not -path './editor-build/node_modules/*' -print0)

python3 - <<'PY'
from pathlib import Path
import re

root = Path(".")

# Documentation is provenance/specification, never an executable oracle.
exec_roots = (
    Path(".github"), Path("scripts"), Path("tests"), Path("src"), Path("crates"),
    Path("tools"), Path("editor-src"), Path("pdf-src"),
)
allowed = {".sh", ".py", ".js", ".mjs", ".rs", ".toml", ".yml", ".yaml"}
banned_docs = (
    re.compile(r"QA_[A-Z0-9_]+\.md"),
    re.compile(r"NATIVE_COMPILE_2B\d+_(?:PLAN|IMPLEMENTATION)\.md"),
    re.compile(r"SECURITY_ENCRYPTION_E[0-3]_CONTRACT\.md"),
    re.compile(r"(?<!QA_)SECURITY_E0_E3_(?:REMEDIATION_2A\d+|CLOSURE_2A19)\.md"),
    re.compile(r"(?<!QA_)VAULT_2B[A-Z0-9_]+\.md"),
)
violations = []
for base in exec_roots:
    if not base.exists():
        continue
    for path in base.rglob("*"):
        if not path.is_file() or path.suffix not in allowed:
            continue
        text = path.read_text(errors="ignore")
        if any(pattern.search(text) for pattern in banned_docs):
            violations.append(path.as_posix())
if violations:
    raise SystemExit("executable documentation dependency returned:\n- " + "\n- ".join(sorted(set(violations))))

# Only qa-release composes concern suites.
suite_names = (
    "qa-toolchain.sh",
    "qa-editor-browser.sh",
    "qa-security-privacy.sh",
    "qa-backup-format.sh",
    "qa-architecture.sh",
)
suite_call = re.compile(r"(?:^|[;&|]\s*|\n\s*)bash\s+scripts/(qa-[a-z-]+\.sh)")
for name in suite_names:
    text = (Path("scripts") / name).read_text()
    invoked = set(suite_call.findall(text))
    if invoked:
        raise SystemExit(f"{name} invokes peer concern suite(s): {sorted(invoked)}")
    if re.search(r"(?:bash\s+)?scripts/validate-(?:architecture|security|vault|backup|version)-", text):
        raise SystemExit(f"{name} depends on a pass-numbered historical validator")

release = Path("scripts/qa-release.sh").read_text()
for name in suite_names:
    if release.count(name) != 1:
        raise SystemExit(f"qa-release must invoke {name} exactly once")

# Layering: UI cannot reach storage/native implementation; application cannot depend on UI.
for path in Path("src/ui").rglob("*.rs"):
    text = path.read_text(errors="ignore")
    for forbidden in ("crate::storage::", "crate::native::", "crate::media_server::"):
        if forbidden in text:
            raise SystemExit(f"UI implementation boundary leak: {path}: {forbidden}")
for path in Path("src/application").rglob("*.rs"):
    text = path.read_text(errors="ignore")
    if "crate::ui::" in text:
        raise SystemExit(f"application layer depends on UI: {path}")

# SQLite implementation authority stays in storage.
for path in Path("src").rglob("*.rs"):
    if path.is_relative_to(Path("src/storage")):
        continue
    if "rusqlite::" in path.read_text(errors="ignore"):
        raise SystemExit(f"direct SQLite authority escaped storage: {path}")

# One executor owns blocking application work.
for path in Path("src").rglob("*.rs"):
    if path == Path("src/application/executor.rs"):
        continue
    if "spawn_blocking" in path.read_text(errors="ignore"):
        raise SystemExit(f"spawn_blocking escaped ApplicationExecutor: {path}")

# Legacy monoliths and hidden active-library architecture must stay gone.
for path in (
    Path("src/models.rs"),
    Path("src/native.rs"),
    Path("assets/pdfjs/rustnote-pdf-viewer.mjs"),
):
    if path.exists():
        raise SystemExit(f"legacy architecture returned: {path}")

# Search parsing remains storage-neutral; SQL/FTS translation stays in storage.
search = Path("src/search.rs").read_text()
for forbidden in ("notes_fts", " MATCH ", "candidate_sql_predicate", "preview_fts_expression", "attachment_names :"):
    if forbidden in search:
        raise SystemExit(f"search AST leaked storage knowledge: {forbidden}")
storage_search = Path("src/storage/search_query.rs").read_text()
for marker in ("candidate_sql_predicate", "preview_fts_expression", "notes_fts MATCH", "all_negative_branch_uses_indexed_not_in_predicate"):
    if marker not in storage_search:
        raise SystemExit(f"storage search authority missing: {marker}")
if "mod search_query;" not in Path("src/storage/mod.rs").read_text():
    raise SystemExit("storage search module is not registered")
for path in (Path("src/storage/notes.rs"), Path("src/storage/note_candidates.rs")):
    if "super::search_query::" not in path.read_text():
        raise SystemExit(f"storage search consumer bypasses canonical translator: {path}")

# Typed editor bridge remains singular and source-derived.
command = Path("src/editor_bridge/command.rs").read_text()
event = Path("src/editor_bridge/event.rs").read_text()
transport = Path("src/editor_bridge/transport.rs").read_text()
entry = Path("editor-src/prosemirror-editor.js").read_text()
for marker in (
    "pub enum EditorCommand",
    "ReadState {",
    "descriptor: Box<EditorAttachmentDescriptor>",
):
    if marker not in command:
        raise SystemExit(f"typed editor command boundary missing: {marker}")
for marker in ("pub enum EditorEvent", "pub enum EditorBridgeDecodeError", "EDITOR_BRIDGE_PROTOCOL_VERSION: u16 = 1"):
    if marker not in event:
        raise SystemExit(f"typed editor event boundary missing: {marker}")
if 'EDITOR_BRIDGE_GLOBAL: &str = "__rustnoteEditorBridge"' not in transport:
    raise SystemExit("typed editor transport lost its single bridge namespace")
if entry.strip() != "// Rustnote ProseMirror runtime entry. Source is modular; esbuild emits one local browser bundle.\nimport './bridge/host.js';":
    raise SystemExit("ProseMirror entry regressed from the tiny modular bridge host")

# Modular local source graphs must be acyclic.
def module_graph(directory: Path, suffix: str):
    files = {path.resolve(): path for path in directory.rglob(f"*{suffix}") if path.is_file()}
    graph = {path: set() for path in files}
    import_re = re.compile(r"(?:from\s+|import\s*)['\"](\.[^'\"]+)['\"]")
    for resolved, path in files.items():
        text = path.read_text(errors="ignore")
        for match in import_re.finditer(text):
            target = (path.parent / match.group(1)).resolve()
            if target.suffix == "":
                target = target.with_suffix(suffix)
            if target in files:
                graph[resolved].add(target)
    visiting, visited = set(), set()
    def visit(node):
        if node in visiting:
            raise SystemExit(f"local module cycle detected: {files[node]}")
        if node in visited:
            return
        visiting.add(node)
        for child in graph[node]:
            visit(child)
        visiting.remove(node)
        visited.add(node)
    for node in graph:
        visit(node)

module_graph(Path("editor-src"), ".js")
module_graph(Path("pdf-src"), ".mjs")
if len(Path("pdf-src/viewer.mjs").read_text().splitlines()) > 4700:
    raise SystemExit("PDF viewer regressed toward the former monolith")

# Direct Result<_, String> APIs are forbidden; preserve typed errors across boundaries.
def direct_string_results(source: str):
    found = []
    position = 0
    while True:
        position = source.find("Result", position)
        if position < 0:
            return found
        cursor = position + len("Result")
        while cursor < len(source) and source[cursor].isspace():
            cursor += 1
        if cursor >= len(source) or source[cursor] != "<":
            position = cursor
            continue
        depth = 0
        comma = None
        end = None
        for index in range(cursor, len(source)):
            char = source[index]
            if char == "<":
                depth += 1
            elif char == ">":
                depth -= 1
                if depth == 0:
                    end = index
                    break
            elif char == "," and depth == 1 and comma is None:
                comma = index
        if end is not None and comma is not None:
            error_type = re.sub(r"\s+", "", source[comma + 1:end])
            if error_type in {"String", "std::string::String", "alloc::string::String"}:
                found.append(source[position:end + 1])
        position = (end + 1) if end is not None else (cursor + 1)

bad = []
for base in (Path("src"), Path("crates")):
    for path in base.rglob("*.rs"):
        for signature in direct_string_results(path.read_text(errors="ignore")):
            bad.append(f"{path}: {signature[:160]}")
if bad:
    raise SystemExit("direct Result<_, String> API returned:\n- " + "\n- ".join(bad))

print("qa-architecture: layering, modularity, typed boundaries and suite topology passed.")
PY
