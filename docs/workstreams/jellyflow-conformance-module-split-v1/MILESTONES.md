# Jellyflow Conformance Module Split v1 - Milestones

Status: Closed
Last updated: 2026-06-01

## M0 - Scope And Contract

- Workstream docs exist and agree.
- Refactor is explicitly behavior-preserving.
- ADR 0003 renderer boundary remains in force.

## M1 - Module Split

- Done: `runtime::conformance::mod` is a small facade.
- Done: scenario/schema vocabulary is separate from execution and IO.
- Done: runner and trace recorder code are local to runner ownership.
- Done: fixture file/directory IO is local to fixture ownership.
- Done: approval write-back behavior is local to approval ownership.
- Done: existing conformance tests, public-surface tests, and harness example pass.

## M2 - Closeout

- Done: fresh formatting, test, lint, JSON, and diff checks are recorded.
- Done: no follow-ons are needed for this behavior-preserving split.
