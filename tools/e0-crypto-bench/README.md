# Rustnote E0 crypto benchmark

This is a research-only benchmark. It is intentionally not part of Rustnote's production Cargo workspace.

Run from the Rustnote source root:

```sh
./scripts/run-security-e0-bench.sh --quick
./scripts/run-security-e0-bench.sh --full
```

The runner copies this tool into `target/`, resolves the exact direct dependency pins there, creates a temporary lockfile, and runs the benchmark in release mode. The shipping `Cargo.toml`, `Cargo.lock`, and `src/` are not modified.

Use `--quick` as a smoke/performance sanity check. Use `--full` when collecting E0 decision data on a real target machine.

Do not compare numbers obtained on different build modes or unrelated machines as though they were controlled benchmark results. E0 uses this harness primarily to compare candidate algorithms **on the same Rustnote target device**.
