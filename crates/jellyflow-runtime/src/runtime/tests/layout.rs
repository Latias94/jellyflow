use jellyflow_core::core::{CanvasPoint, CanvasSize, Graph};

use crate::runtime::layout::{
    DugongLayoutApplyOutcome, LayoutContext, LayoutEngine, LayoutEngineId, LayoutEngineRegistry,
    LayoutEngineRequest, LayoutError, LayoutNodePosition, LayoutRequest, LayoutResult,
    builtin_layout_engine_registry,
};
use crate::runtime::measurement::NodeMeasurement;
use crate::runtime::tests::fixtures::{make_graph, make_store};

#[derive(Debug, Clone, Copy)]
struct ContextSizeEngine;

impl LayoutEngine for ContextSizeEngine {
    fn id(&self) -> LayoutEngineId {
        LayoutEngineId::new("context-size")
    }

    fn layout(
        &self,
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        let nodes = graph
            .nodes()
            .keys()
            .map(|node| {
                let size = context
                    .measured_node_sizes
                    .get(node)
                    .copied()
                    .unwrap_or(request.options.default_node_size);
                LayoutNodePosition {
                    node: *node,
                    pos: CanvasPoint {
                        x: size.width,
                        y: size.height,
                    },
                    center: CanvasPoint {
                        x: size.width + size.width * 0.5,
                        y: size.height + size.height * 0.5,
                    },
                    size,
                }
            })
            .collect();

        Ok(LayoutResult {
            nodes,
            edge_routes: Vec::new(),
            bounds: None,
        })
    }
}

#[test]
fn dugong_layout_transaction_dispatches_through_store() {
    let (graph, a, b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let request = LayoutRequest::all().with_measured_node_sizes([
        (
            a,
            CanvasSize {
                width: 160.0,
                height: 80.0,
            },
        ),
        (
            b,
            CanvasSize {
                width: 160.0,
                height: 80.0,
            },
        ),
    ]);

    let tx = store
        .dugong_layout_transaction(&request)
        .expect("layout transaction");
    let outcome = store.dispatch_transaction(&tx).expect("dispatch");

    assert_eq!(outcome.committed().label(), Some("Layout graph"));
    assert!(!outcome.committed().is_empty());
    assert!(
        store.graph().nodes()[&a].pos != (CanvasPoint { x: 0.0, y: 0.0 })
            || store.graph().nodes()[&b].pos != (CanvasPoint { x: 100.0, y: 0.0 })
    );
}

#[test]
fn apply_dugong_layout_returns_layout_and_dispatch_outcome() {
    let (graph, a, _b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let request = LayoutRequest::all();

    let outcome = store
        .apply_dugong_layout(&request)
        .expect("apply dugong layout");

    let DugongLayoutApplyOutcome { layout, dispatch } = outcome;
    assert!(layout.node_position(a).is_some());
    let dispatch = dispatch.expect("layout changed node positions");
    assert_eq!(dispatch.committed().label(), Some("Layout graph"));
}

#[test]
fn apply_freeform_layout_returns_layout_and_dispatch_outcome() {
    let (graph, a, _b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let request = LayoutEngineRequest::mind_map_freeform(LayoutRequest::all());

    let outcome = store
        .apply_layout(&request, builtin_layout_engine_registry())
        .expect("apply freeform layout");

    assert!(outcome.layout.node_position(a).is_some());
    assert_eq!(
        outcome
            .committed()
            .expect("layout changed node positions")
            .label(),
        Some("Layout graph")
    );
}

#[test]
fn custom_layout_engine_applies_through_store() {
    let (graph, a, _b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let mut registry = LayoutEngineRegistry::new();
    registry.insert(ContextSizeEngine).expect("register engine");
    let request = LayoutEngineRequest::new("context-size", LayoutRequest::all());

    let outcome = store
        .apply_layout(&request, &registry)
        .expect("apply layout");

    assert!(outcome.layout.node_position(a).is_some());
    assert_eq!(
        store.graph().nodes()[&a].pos,
        CanvasPoint { x: 172.0, y: 36.0 }
    );
    assert_eq!(outcome.committed().unwrap().label(), Some("Layout graph"));
}

#[test]
fn store_layout_context_uses_runtime_measurements_without_persisting_size() {
    let (graph, a, _b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let measured = CanvasSize {
        width: 240.0,
        height: 96.0,
    };
    store
        .report_node_measurement(NodeMeasurement::new(a).with_size(Some(measured)))
        .expect("report measurement");
    let mut registry = LayoutEngineRegistry::new();
    registry.insert(ContextSizeEngine).expect("register engine");
    let request = LayoutEngineRequest::new("context-size", LayoutRequest::all());

    let outcome = store
        .apply_layout(&request, &registry)
        .expect("apply layout");

    let node_position = outcome.layout.node_position(a).expect("node position");
    assert_eq!(node_position.size, measured);
    assert_eq!(
        store.graph().nodes()[&a].pos,
        CanvasPoint {
            x: measured.width,
            y: measured.height
        }
    );
    assert_eq!(store.graph().nodes()[&a].size, None);
    assert_eq!(
        store.layout_context().measured_node_sizes.get(&a),
        Some(&measured)
    );
}

#[test]
fn apply_layout_skips_empty_dirty_scope_transactions() {
    let (graph, _a, _b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let request = LayoutRequest::all().with_dirty_scope_from_footprint(
        store.graph(),
        &jellyflow_core::ops::GraphMutationFootprint::new(),
    );

    let outcome = store
        .apply_layout(
            &LayoutEngineRequest::dugong(request),
            builtin_layout_engine_registry(),
        )
        .expect("apply empty dirty-scope layout");

    assert!(outcome.layout.nodes.is_empty());
    assert!(outcome.committed().is_none());
    assert!(outcome.dispatch.is_none());
}
