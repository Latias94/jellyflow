# Jellyflow Layout

Optional headless layout adapters for Jellyflow graphs.

The crate keeps layout engines outside `jellyflow-core` and returns immutable `LayoutResult`
values. Applying those results belongs to a revision-aware runtime or host.

## Engine Boundary

`jellyflow-layout` exposes a small engine protocol:

- `PreparedLayoutInput` is the registry-owned, validated projection shared by every engine.
- `LayoutEngine` consumes that projection and returns only center placements plus optional routes.
- `LayoutEngineDescriptor` registers an engine together with its stable id and discovery metadata.
- `LayoutEngineRegistry` owns preparation, dispatch, and the single result finalizer.
- `LayoutEngineRequest` selects one engine and carries a `LayoutRequest`.
- `LayoutContext` carries runtime-only facts such as measured node sizes and fallback node origin.
- `LayoutResult` is the finalized immutable output: node positions, edge routes, and bounds.
- `LayoutFamilyMetadata` and `LayoutEngineMetadata` expose discovery-only family/capability data
  without changing the stable engine-id dispatch contract.

The built-in `dugong` engine is registered under `DUGONG_LAYOUT_ENGINE_ID` (`"dugong"`).
The built-in tidy tree engine is registered under `TIDY_TREE_LAYOUT_ENGINE_ID`
(`"tidy_tree"`).
The built-in radial mind-map engine is registered under `MIND_MAP_RADIAL_LAYOUT_ENGINE_ID`
(`"mind_map_radial"`).
The built-in freeform mind-map engine is registered under `MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID`
(`"mind_map_freeform"`).
It currently keeps the input canvas feel and only resolves overlaps while preserving pinned nodes.
Hosts select built-in or custom engines through `LayoutEngine` and `LayoutEngineRegistry`.
The built-in `dugong` and tidy tree engines are grouped under the `layered_dag` family, while the
radial and freeform mind-map engines are grouped under the `mind_map` family.

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
assert_eq!(tree.engine.as_str(), "tidy_tree");
assert_eq!(mind_map.engine.as_str(), "mind_map_radial");
```

```rust
use jellyflow_core::Graph;
use jellyflow_layout::{
    LayoutContext, LayoutPresetBuilder, LayoutResult, builtin_layout_engine_registry,
};

fn plan_builtin_layout(graph: &Graph) -> Result<LayoutResult, jellyflow_layout::LayoutError> {
    let registry = builtin_layout_engine_registry();
    let request = LayoutPresetBuilder::workflow().build();
    let context = LayoutContext::new();

    registry.layout(graph, &request, &context)
}
```

Applications using `jellyflow-runtime` map `result.nodes` to
`NodeGeometryEdit::new(position.node).with_position(position.pos)`, then build one
`GraphCommand::set_node_geometry` with the opaque graph version from the snapshot used to plan the
layout. `LayoutEngineRegistry` rejects duplicate, out-of-scope, non-finite, and unrepresentable
custom-engine geometry before it returns a result. This keeps every layout write on the same
explicit command path as other document mutations without coupling the runtime to a layout engine.

Custom engines do not receive an editable graph or construct `LayoutResult` directly. The registry
resolves scope, hidden nodes, size precedence, origins, pinning, and edge endpoints once, then asks
the engine for the smallest useful output:

```rust
use jellyflow_core::CanvasPoint;
use jellyflow_layout::{
    LayoutEngine, LayoutEngineDescriptor, LayoutEngineId, LayoutEngineMetadata, LayoutError,
    LayoutFamilyMetadata, LayoutNodePlacement, LayoutPlacements, PreparedLayoutInput,
};

struct PreserveCurrentCenters;

impl LayoutEngine for PreserveCurrentCenters {
    fn place(&self, input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        Ok(LayoutPlacements::preserve_canvas_coordinates(
            input.nodes().map(|node| LayoutNodePlacement {
                node: node.id(),
                center: node.current_center(),
            }),
        ))
    }
}

fn custom_descriptor() -> LayoutEngineDescriptor {
    let family = LayoutFamilyMetadata::new("custom", "Custom");
    let engine = LayoutEngineId::new("custom.preserve");
    let metadata = LayoutEngineMetadata::new(engine, family.id.clone(), "Preserve centers");

    LayoutEngineDescriptor::new(family, metadata, PreserveCurrentCenters)
}
```

This boundary keeps product-specific algorithms extensible without creating a portable layout DSL
or allowing engines to duplicate graph validation and coordinate-frame finalization.
