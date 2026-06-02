# XyFlow Gap Review Design

## Boundary

This task produces a review report. It may add review documentation and task
evidence, but it must not change Jellyflow runtime behavior, public APIs,
schema, or adapter template code.

Source-of-truth order:

1. Accepted ADRs.
2. `CONTEXT.md`.
3. Current Jellyflow source and tests.
4. `repo-ref/xyflow` source.
5. Preserved history under `docs/history/`.

## Review Output

Primary output:

```text
docs/reviews/xyflow-gap-2026-06-02.md
```

The report should include:

- executive summary;
- source map;
- coverage matrix;
- detailed findings with file references;
- top gaps and suggested follow-up Trellis tasks;
- explicit adapter-owned and intentionally out-of-scope areas.

## Comparison Model

Classify each area with one status:

| Status | Meaning |
| --- | --- |
| `covered` | Jellyflow has a headless contract and meaningful tests/conformance. |
| `partial` | Jellyflow has part of the behavior, but misses important XyFlow semantics or evidence. |
| `missing` | Jellyflow lacks a headless equivalent that likely belongs in runtime/conformance. |
| `adapter-owned` | XyFlow behavior belongs in future adapter crates because it depends on DOM, renderer, platform input, or visual components. |
| `intentionally out of scope` | Behavior should not be copied into Jellyflow based on current ADRs/product intent. |

## Review Areas

1. Model, IDs, graph operations, and store commit semantics.
2. Node/edge changes, callback projection, controlled-mode behavior.
3. Connection and reconnection, handle lookup, connection radius, validation.
4. Selection, keyboard intent, delete, and async/pre-delete behavior.
5. Node drag, parent expansion, extents, snap-to-grid, keyboard movement.
6. Node resize, keep-aspect-ratio, parent/child clamps, resize event lifecycle.
7. Viewport, pan/zoom, fitView, translate extent, scroll, double-click,
   animation, inertia.
8. Auto-pan activation by node drag, connect, selection, and node focus.
9. Geometry, edge paths, edge hit testing, visible nodes, visible edges, render
   order, spatial indexing.
10. Adapter/UI surface: React wrappers, DOM measurement, minimap, controls,
    background, toolbar, portals, accessibility, SSR/provider behavior.

## Evidence Rules

- Every non-obvious claim should cite at least one XyFlow file and one
  Jellyflow file, or explain why the Jellyflow side is absent.
- Do not over-quote source. Use short identifiers, function/module names, and
  file references.
- Use `rg` and focused file reads. Avoid broad source dumping.
- Prefer facts that can become follow-up task acceptance criteria.

## Risk And Rollback

Risk is low because this task is documentation-only. The main risk is drawing
the wrong boundary and recommending renderer/UI work inside the headless crates.
Mitigation: check ADR 0001 and ADR 0003 before finalizing recommendations.
