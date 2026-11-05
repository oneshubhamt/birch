#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:---quick}"
if [[ "$MODE" != "--quick" && "$MODE" != "--full" ]]; then
  echo "usage: $0 [--quick|--full]" >&2
  exit 2
fi

# E0 is intentionally outside Rustnote's production workspace. Resolve and lock
# its exact direct dependency pins in target/ so running the research benchmark
# never edits the shipping Cargo.toml/Cargo.lock or the source tree.
BENCH_WORK_DIR="$ROOT_DIR/target/rustnote-e0-crypto-bench"
rm -rf "$BENCH_WORK_DIR"
mkdir -p "$BENCH_WORK_DIR/src"
cp "$ROOT_DIR/tools/e0-crypto-bench/Cargo.toml" "$BENCH_WORK_DIR/Cargo.toml"
cp "$ROOT_DIR/tools/e0-crypto-bench/src/main.rs" "$BENCH_WORK_DIR/src/main.rs"

cargo generate-lockfile --manifest-path "$BENCH_WORK_DIR/Cargo.toml"
cargo run \
  --release \
  --locked \
  --manifest-path "$BENCH_WORK_DIR/Cargo.toml" \
  -- "$MODE"
