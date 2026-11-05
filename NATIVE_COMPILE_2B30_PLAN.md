# Rustnote 2b.30 plan

Audit: 2b.29's macOS deployment-target source validator imports Python 3.11-only `tomllib`, so Monterey systems with older `python3` fail before preparation begins.

Objectives:
1. Remove the Python 3.11 dependency without weakening any deployment-target assertion.
2. Preserve macOS 12.0, AEGIS 0.9.15 native backend, bundle metadata, and Mach-O validation unchanged.
3. Extend inherited build-ID validators to admit 2b.30.
4. Re-run targeted and inherited static QA plus dependency-free behavioral QA.
