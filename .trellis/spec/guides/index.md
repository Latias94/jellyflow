# Jellyflow Shared Guides

These guides are loaded for every Trellis task in this repository. They are
project-specific guardrails for a headless Rust graph engine, not generic
server-application rules.

## Source Of Truth Order

When facts conflict, use this order:

1. The current task artifacts define the immediate scope and acceptance criteria.
2. Accepted ADRs in `docs/adr/` define architecture decisions.
3. `CONTEXT.md` is the high-signal navigation summary.
4. Preserved history under `docs/history/` and ADR companion notes provide
   prior evidence and tradeoffs.
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
- For repository-level CI, package metadata, or release documentation, read
  `.trellis/spec/repository/backend/release-readiness.md`.
- Read ADRs referenced by the touched boundary.
- Use `rg` before adding a helper, enum variant, config field, conformance action,
  or public re-export.
- For code changes, choose the smallest meaningful validation gate from
  `CONTEXT.md`; prefer `cargo nextest` for Rust tests.

## Legacy Workstream Rule

Do not recreate the old `WORKSTREAM.json` lane system. New work uses Trellis
tasks under `.trellis/tasks/`; long-lived conventions go into `.trellis/spec/`;
architecture decisions go into `docs/adr/`.
