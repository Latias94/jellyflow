# Jellyflow Conformance File Fixtures v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- File fixture scope is separate from renderer smoke tests.

## M1 - File Loader

- `ConformanceSuite::load_json`, `load_json_if_exists`, and `save_json` exist.
- Tests cover roundtrip save/load, optional missing files, and parse errors.
- Public-surface smoke coverage uses the file helpers.

## M2 - Closeout

- README/runtime README explain file-backed headless fixture suites.
- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Follow-ons are split or explicitly deferred.
