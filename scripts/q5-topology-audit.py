#!/usr/bin/env python3
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
VALIDATOR = re.compile(r"^validate-.*\.sh$")
PASS_NUMBERED = re.compile(
    r"^validate-(?:architecture-[ab]\d|security-e0-e3-.*2a\d+|vault-2b\d|backup-2b\d|version-v\d)"
)
CALL = re.compile(r"(?:^|[\s;])(?:bash\s+|\.\s+)?(?:\./)?(scripts/validate-[A-Za-z0-9._-]+\.sh)")
SCRIPT_READ = re.compile(
    r"(?:read_text|read\(|cat\s+|grep[^\n]*|Path\([^\n]*)[^\n]*scripts/validate-[A-Za-z0-9._-]+\.sh"
)
CARGO = re.compile(r"\bcargo\s+(?:check|clippy|test|build)\b")
BROWSER = re.compile(r"run-browser-behavior-tests\.mjs")

validators = sorted(path for path in SCRIPTS.glob("validate-*.sh") if VALIDATOR.match(path.name))
names = {f"scripts/{path.name}" for path in validators}
edges: dict[str, set[str]] = {}
script_readers: dict[str, int] = {}
cargo_occurrences = 0
browser_occurrences = 0
total_lines = 0
total_bytes = 0

for path in validators:
    rel = f"scripts/{path.name}"
    text = path.read_text(errors="ignore")
    total_lines += len(text.splitlines())
    total_bytes += len(text.encode())
    called = {match.group(1) for match in CALL.finditer(text)}
    edges[rel] = {target for target in called if target in names and target != rel}
    count = len(SCRIPT_READ.findall(text))
    if count:
        script_readers[rel] = count
    cargo_occurrences += len(CARGO.findall(text))
    browser_occurrences += len(BROWSER.findall(text))

memo: dict[str, int] = {}
visiting: set[str] = set()
def depth(node: str) -> int:
    if node in memo:
        return memo[node]
    if node in visiting:
        return 0
    visiting.add(node)
    value = 1 + max((depth(child) for child in edges.get(node, ())), default=0)
    visiting.remove(node)
    memo[node] = value
    return value

metrics = {
    "validator_scripts": len(validators),
    "pass_numbered_validators": sum(1 for path in validators if PASS_NUMBERED.match(path.name)),
    "validator_lines": total_lines,
    "validator_bytes": total_bytes,
    "inter_validator_edges": sum(len(targets) for targets in edges.values()),
    "max_validator_chain_depth": max((depth(node) for node in edges), default=0),
    "validators_reading_validator_source": len(script_readers),
    "validator_source_read_occurrences": sum(script_readers.values()),
    "cargo_command_occurrences_inside_validators": cargo_occurrences,
    "browser_runner_occurrences_inside_validators": browser_occurrences,
}
print(json.dumps(metrics, sort_keys=True))
