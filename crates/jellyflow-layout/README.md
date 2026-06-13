# Jellyflow Layout

Optional headless layout adapters for Jellyflow graphs.

The crate keeps layout engines outside `jellyflow-core` and returns normal Jellyflow
transactions so hosts can decide when to apply or animate layout results.

## Engine Boundary

`jellyflow-layout` exposes a small engine protocol:

- `LayoutEngine` is implemented by built-in or host-provided engines.
- `LayoutEngineRegistry` maps stable `LayoutEngineId` values to engines.
- `LayoutEngineRequest` selects one engine and carries a `LayoutRequest`.
- `LayoutContext` carries runtime-only facts such as measured node sizes and fallback node origin.
- `LayoutFamilyMetadata` and `LayoutEngineMetadata` expose discovery-only family/capability data
  without changing the stable engine-id dispatch contract.

The built-in `dugong` engine is registered under `DUGONG_LAYOUT_ENGINE_ID` (`"dugong"`).
The built-in radial mind-map engine is registered under `MIND_MAP_RADIAL_LAYOUT_ENGINE_ID`
(`"mind_map_radial"`).
The built-in freeform mind-map engine is registered under `MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID`
(`"mind_map_freeform"`).
It currently keeps the input canvas feel and only resolves overlaps while preserving pinned nodes.
Compatibility helpers such as `layout_graph_with_dugong` remain available, but new host code should
prefer the registry path when it needs runtime selection or custom engines.
The built-in `dugong` engine is grouped under the `layered_dag` family, while the radial and
freeform mind-map engines are grouped under the `mind_map` family.

## Presets

For common editor flows, start with `LayoutPresetBuilder`. Presets only choose an engine and
options; they still produce ordinary `LayoutEngineRequest` values, so hosts can pass them through
the registry/runtime path or customize them before execution.

```rust
use jellyflow_layout::{LayoutDirection, LayoutPresetBuilder};

let workflow = LayoutPresetBuilder::workflow()
    .with_direction(LayoutDirection::LeftToRight)
    .build();
let tree = LayoutPresetBuilder::tree().build();
let mind_map = LayoutPresetBuilder::mind_map().build();

assert_eq!(workflow.engine.as_str(), "dugong");
assert_eq!(tree.engine.as_str(), "dugong");
assert_eq!(mind_map.engine.as_str(), "mind_map_radial");
```

```rust
use jellyflow_core::Graph;
use jellyflow_layout::{
    LayoutContext, LayoutPresetBuilder, builtin_layout_engine_registry,
    layout_graph_to_transaction_with_engine,
};

fn plan_builtin_layout(graph: &Graph) -> Result<(), jellyflow_layout::LayoutError> {
    let registry = builtin_layout_engine_registry();
    let request = LayoutPresetBuilder::workflow().build();
    let context = LayoutContext::new();

    let tx = layout_graph_to_transaction_with_engine(graph, &request, &registry, &context)?;
    assert_eq!(tx.label(), Some("Layout graph"));
    Ok(())
}
```

For interactive editors, keep product-specific behavior in custom engines. A radial, freeform, or
mind-map engine can implement `LayoutEngine`, use `LayoutContext` for measured sizes or pinned
nodes, and still return the same `LayoutResult` / `GraphTransaction` shape as the built-in engine.
