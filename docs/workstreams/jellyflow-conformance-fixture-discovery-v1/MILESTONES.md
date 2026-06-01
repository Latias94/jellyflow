# Jellyflow Conformance Fixture Discovery v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- Discovery scope is separate from approval workflows and renderer smoke tests.

## M1 - Directory Discovery

- Fixture directory API discovers JSON suite files recursively.
- Discovered paths are deterministic.
- Loaded suites keep source paths for reports.
- Tests cover sorted recursive discovery, optional missing directories, and invalid JSON errors.
- Public-surface smoke coverage uses the directory loader.

## M2 - Closeout

- README/runtime README explain directory-backed fixture suites.
- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Follow-ons are split or explicitly deferred.
