# Jellyflow Conformance Golden Approval v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- Approval/update scope is separate from CLI and renderer smoke tests.

## M1 - Approval Update API

- Suite-level API can approve actual traces into a new suite.
- File-level API can explicitly write approved traces to JSON.
- Directory-level API computes approvals before writing files.
- Write-back refuses scenario execution errors.
- Reports are serde-friendly.
- Tests cover suite approval, file write-back, directory write-back, and error refusal.

## M2 - Closeout

- README/runtime README explain explicit approval write-back.
- Fresh formatting, test, lint, JSON, and diff checks are recorded.
- Follow-ons are split or explicitly deferred.
