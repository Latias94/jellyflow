# Jellyflow Product-Shaped Example Gallery

Jellyflow examples are contract tests for the headless node UI foundation. They should demonstrate
semantic node kits, adapter-local widget mapping, measured internals, connection behavior, resize
stability, and low-zoom degradation without adding backend workflow execution.

## Product Families

| Family | Semantic source | Adapter proof | What it proves |
| --- | --- | --- | --- |
| Dify-style workflow | `workflow.automation`, `demo.llm` | `jellyflow-egui` Automation builder, GPUI canvas proof, `jellyflow-proof` review card | Prompt/completion field rows, model/metric badges, config groups, status banners, actions, run chrome, and measured handles. |
| Shader or blueprint graph | `shader.blueprint`, `demo.shader.*` | `jellyflow-egui` Shader graph, GPUI shader fixture projection | Typed port rails, shader-card renderer mapping, compact config/preview regions, and source/target edge projection. |
| ERD table graph | `erd.table`, `demo.table` | `jellyflow-egui` ERD sample, runtime fixture tests | PK/FK field anchors, relation labels, resize-stable field geometry, and measured edge endpoints. |
| Knowledge canvas | `mind-map.knowledge-canvas`, `demo.topic` / `demo.source` | `jellyflow-egui` Knowledge board, proof/template surfaces | Source/title/preview regions, graph-shaped notes, low-zoom shell behavior, and framework-neutral semantic slots. |

## Verification

Use the focused gates when changing node-kit recipes or adapter mappings:

```sh
cargo test -p jellyflow-runtime fixture
cargo test -p jellyflow-egui all_sample_graphs_build_with_nodes_edges_and_default_layouts
cargo test -p jellyflow-egui product_samples_reuse_edge_metadata_and_port_descriptors
cargo test -p jellyflow-proof --example adapter_smoke
cargo test --manifest-path templates/headless-adapter/Cargo.toml
RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow
```

The examples are intentionally frontend/headless. They do not execute LLM calls, shader compilers,
database queries, sync engines, schedulers, or collaboration backends.
