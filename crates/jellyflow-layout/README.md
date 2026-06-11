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

The built-in `dugong` engine is registered under `DUGONG_LAYOUT_ENGINE_ID` (`"dugong"`).
The built-in radial mind-map engine is registered under `MIND_MAP_RADIAL_LAYOUT_ENGINE_ID`
(`"mind_map_radial"`).
Compatibility helpers such as `layout_graph_with_dugong` remain available, but new host code should
prefer the registry path when it needs runtime selection or custom engines.

```rust
use jellyflow_core::Graph;
use jellyflow_layout::{
    LayoutContext, LayoutEngineRequest, LayoutRequest, builtin_layout_engine_registry,
    layout_graph_to_transaction_with_engine,
};

fn plan_builtin_layout(graph: &Graph) -> Result<(), jellyflow_layout::LayoutError> {
    let registry = builtin_layout_engine_registry();
    let request = LayoutEngineRequest::dugong(LayoutRequest::all());
    let context = LayoutContext::new();

    let tx = layout_graph_to_transaction_with_engine(graph, &request, &registry, &context)?;
    assert_eq!(tx.label(), Some("Layout graph"));
    Ok(())
}
```

For interactive editors, keep product-specific behavior in custom engines. A radial, freeform, or
mind-map engine can implement `LayoutEngine`, use `LayoutContext` for measured sizes or pinned
nodes, and still return the same `LayoutResult` / `GraphTransaction` shape as the built-in engine.
