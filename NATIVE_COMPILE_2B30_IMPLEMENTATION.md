# Rustnote 2b.30 implementation

- Replaced `tomllib` parsing in `scripts/validate-macos-deployment-target.sh` with a narrow `re` + `pathlib` validator compatible with older Python 3 versions.
- The validator still requires the exact Cargo environment contract `MACOSX_DEPLOYMENT_TARGET = { value = "12.0", force = true }` inside `[env]`.
- AEGIS 0.9.15 native backend, Info.plist minimum version, and binary deployment-target validation are unchanged.
- No Rust source or runtime behavior changed.
