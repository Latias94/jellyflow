# Jellyflow Shared Guides

These guides are loaded for every Trellis task in this repository. They are
project-specific guardrails for a headless Rust graph engine, not generic
server-application rules.

## Source Of Truth Order

When facts conflict, use this order:

1. The current task artifacts define the immediate scope and acceptance criteria.
2. Accepted ADRs in `docs/adr/` define architecture decisions.
3. `CONTEXT.md` is the high-signal navigation summary.
4. Closed workstreams in `docs/workstreams/` and `docs/history/fret-workstreams/`
   provide evidence and prior tradeoffs.
5. Source code, tests, and crate READMEs show the current implementation.

If a task request conflicts with an accepted ADR, stop and plan the ADR change
instead of silently coding around it.

## Available Guides

| Guide | Use |
| --- | --- |
| [Code Reuse Thinking Guide](./code-reuse-thinking-guide.md) | Search for existing Jellyflow helpers, fixtures, and contracts before adding new ones. |
| [Cross-Layer Thinking Guide](./cross-layer-thinking-guide.md) | Check crate, schema, runtime, conformance, XyFlow, adapter, and workstream boundaries. |

## Pre-Modification Checklist

- Read the task `prd.md`; read `design.md` and `implement.md` when present.
- Read the relevant package index under `.trellis/spec/<package>/backend/`.
- Read ADRs referenced by the touched boundary.
- Use `rg` before adding a helper, enum variant, config field, conformance action,
  or public re-export.
- For code changes, choose the smallest meaningful validation gate from
  `CONTEXT.md`; prefer `cargo nextest` for Rust tests.

## Legacy Workstream Rule

Do not convert closed legacy workstreams into Trellis tasks. Treat them as
historical evidence and link them from new Trellis task artifacts when they
explain a boundary, gate, or follow-on.
