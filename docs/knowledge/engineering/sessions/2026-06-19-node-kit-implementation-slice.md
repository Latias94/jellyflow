---
type: "Session Handoff"
title: "Node-kit implementation slice"
description: "Handoff for the runtime kit reuse, egui sample cleanup, proof/template reuse, and registry fix."
tags: ["engineering-memory", "jellyflow", "session", "node-kit", "implementation"]
timestamp: 2026-06-19T17:18:40Z
status: "active"
---

# Summary

The node-kit boundary is now materially implemented. `jellyflow-runtime` owns built-in node-kit
manifests, fixtures, and a reusable kit registry; `jellyflow-egui` sample graphs now start from the
same built-in kit registry instead of copying the runtime schemas; `jellyflow-proof` and the
headless adapter template both reuse the builtin kit registry; and a registry precedence bug was
fixed so canonical kinds win over alias matches.

# Verified State

- `jellyflow-runtime` now exposes `NodeKitRegistry::builtin()` with workflow, ERD, and mind-map
  manifests and fixture graphs.
- The runtime kit tests cover all three builtin fixtures.
- `jellyflow-egui` sample graphs now reuse the builtin kit registry and still pass the full egui
  test suite.
- `jellyflow-proof` continues to pass its tests while reusing `NodeKitRegistry::builtin()`.
- `templates/headless-adapter` now starts from the builtin kit registry, layers its own template
  node on top, and passes its conformance suite.
- `NodeRegistry::resolve_kind` now prefers an explicitly registered canonical kind before alias
  matches, and a regression test was added to lock that behavior.

# Next Action

Update the plan and engineering memory summaries, then decide whether the remaining goal work is
just documentation/memory sync or whether more egui cleanup and adapter coverage still need to be
executed.

# Citations

- [Node kit plan](../../../plans/2026-06-19-003-feat-adapter-node-kit-boundary-plan.md)
- [Runtime kit tests](../../../../crates/jellyflow-runtime/src/schema/tests/kit.rs)
- [Registry precedence test](../../../../crates/jellyflow-runtime/src/schema/tests/instantiation.rs)
- [egui samples](../../../../crates/jellyflow-egui/src/samples.rs)
- [proof crate](../../../../crates/jellyflow-proof/src/lib.rs)
- [headless adapter template](../../../../templates/headless-adapter/src/lib.rs)
