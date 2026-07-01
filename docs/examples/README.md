# Jellyflow Product-Shaped Example Gallery

Jellyflow examples are contract tests for the headless node UI foundation. They should demonstrate
semantic node kits, adapter-local widget mapping, measured internals, connection behavior, resize
stability, and low-zoom degradation without adding backend workflow execution.

## Product Families

| Family | Semantic source | Adapter proof | What it proves |
| --- | --- | --- | --- |
| Dify-style workflow | `workflow.automation`, `demo.llm` | `jellyflow-egui` Automation builder, Open GPUI `workflow.review`, `jellyflow-proof` review card | Prompt/model/temperature controls, action menu dispatch, status/progress chrome, inspector paths, and measured handles. |
| Shader or blueprint graph | `shader.blueprint`, `demo.shader.*` | `jellyflow-egui` Shader graph, Open GPUI `shader.material_mix` | Typed port rails, shader-card renderer mapping, dynamic input rows, missing-port diagnostics, compact config/preview regions, and source/target edge projection. |
| ERD table graph | `erd.table`, `demo.table` | `jellyflow-egui` ERD sample, Open GPUI `erd.customer_orders` | PK/FK field anchors, editable repeatable rows, relation labels, resize-stable field geometry, explicit port downgrades, and measured edge endpoints. |
| Knowledge canvas | `mind-map.knowledge-canvas`, `demo.topic` / `demo.source` | `jellyflow-egui` Knowledge board, Open GPUI `mind-map.strategy`, proof/template surfaces | Source/title/preview regions, graph-shaped notes, low-zoom shell behavior, topic/source custom renderers, and framework-neutral semantic slots. |

## Open GPUI Product Gallery

The Open GPUI gallery is the first mature adapter surface. It is intentionally host-local:
`jellyflow-open-gpui` owns widget-free renderer plans, authoring plans, ids, measurement facts, and
structured reports; `repo-ref/open-gpui/examples/canvas-jellyflow` owns concrete Open GPUI
components and product renderers.

Run the gallery:

```sh
cargo run --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml
```

The local component kit lives at
`repo-ref/open-gpui/examples/canvas-jellyflow/src/node_component_kit.rs`. Product renderers live at
`repo-ref/open-gpui/examples/canvas-jellyflow/src/product_renderers.rs`, and the gallery fixture
catalog lives at `repo-ref/open-gpui/examples/canvas-jellyflow/src/product_gallery.rs`.

## Verification

Use the focused gates when changing node-kit recipes or adapter mappings:

```sh
cargo test -p jellyflow-runtime fixture
cargo test -p jellyflow-egui all_sample_graphs_build_with_nodes_edges_and_default_layouts
cargo test -p jellyflow-egui product_samples_reuse_edge_metadata_and_port_descriptors
cargo test -p jellyflow-proof --example adapter_smoke
cargo test --manifest-path templates/headless-adapter/Cargo.toml
RUSTFLAGS='-Awarnings' cargo test --quiet --manifest-path repo-ref/open-gpui/examples/canvas-jellyflow/Cargo.toml --bin open-gpui-canvas-jellyflow -- --nocapture --test-threads=1
```

The examples are intentionally frontend/headless. They do not execute LLM calls, shader compilers,
database queries, sync engines, schedulers, or collaboration backends.

When the local Open GPUI headless renderer supports capture, the `canvas-jellyflow` bin tests also
export review screenshots to `repo-ref/open-gpui/target/open-gpui-jellyflow-gallery/`. Those PNGs
are smoke artifacts only; geometry/capability/interaction reports are the hard regression gate.
