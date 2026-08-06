use egui::{Response, Ui, Vec2};
use egui_kittest::{Harness, kittest::Queryable as _};
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_egui::{EguiNativeFrameHost, EguiNativeFrameOutcome};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::view::{FrameBoundsSource, FrameRequest, RendererStamp, ViewSessionOptions};

const UNKNOWN_KIND: &str = "plugin.future.unknown";
const FALLBACK_RENDERER: &str = "plugin.missing.renderer";

struct MissingRendererState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    ports: [PortId; 2],
    observed_source: Option<FrameBoundsSource>,
    outcome: Option<EguiNativeFrameOutcome>,
}

impl MissingRendererState {
    fn new() -> Self {
        let node = NodeId::from_u128(0xf001);
        let ports = [PortId::from_u128(0xf011), PortId::from_u128(0xf012)];
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph(node, ports))
            .expect("unknown-kind runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("unknown-kind view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            ports,
            observed_source: None,
            outcome: None,
        }
    }
}

fn render_missing_renderer(ui: &mut Ui, state: &mut MissingRendererState) {
    let source = &mut state.observed_source;
    state.outcome = Some(
        state
            .host
            .show_with_fallback(ui, frame_request(), |_ui, native| {
                source.get_or_insert(native.frame_node().bounds_source());
                None::<Response>
            })
            .expect("diagnostic fallback submits complete geometry"),
    );
}

#[test]
fn unknown_node_kind_renders_a_diagnostic_and_publishes_every_real_port() {
    let harness = Harness::builder()
        .with_size(Vec2::new(640.0, 480.0))
        .build_ui_state(render_missing_renderer, MissingRendererState::new());
    let state = harness.state();
    let outcome = state.outcome.as_ref().expect("fallback outcome");
    let diagnostic = outcome
        .diagnostics()
        .first()
        .expect("missing renderer diagnostic");

    assert_eq!(outcome.diagnostics().len(), 1);
    assert_eq!(diagnostic.node(), state.node);
    assert_eq!(
        diagnostic.node_kind(),
        Some(&NodeKindKey::new(UNKNOWN_KIND)),
    );
    assert_eq!(diagnostic.renderer_key(), FALLBACK_RENDERER);
    assert!(diagnostic.message().contains(UNKNOWN_KIND));
    assert_eq!(
        state.observed_source,
        Some(FrameBoundsSource::ProtocolFallback),
    );

    let geometry = state
        .host
        .view()
        .geometry_snapshot()
        .expect("fallback geometry");
    let node_geometry = geometry.node(state.node).expect("fallback node geometry");
    assert!(node_geometry.is_fresh());
    assert_eq!(
        node_geometry.ports().keys().copied().collect::<Vec<_>>(),
        state.ports,
    );
    harness.get_by_label("Renderer unavailable");
}

fn graph(node: NodeId, ports: [PortId; 2]) -> jellyflow_core::Graph {
    let mut builder = GraphBuilder::new(GraphId::from_u128(0xf000));
    builder.insert_node(
        node,
        Node {
            kind: NodeKindKey::new(UNKNOWN_KIND),
            kind_version: 99,
            pos: CanvasPoint { x: 20.0, y: 20.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports: ports.to_vec(),
            data: serde_json::json!({ "future_plugin": { "version": 99 } }),
        },
    );
    builder.insert_port(ports[0], port(node, "input", PortDirection::In));
    builder.insert_port(ports[1], port(node, "output", PortDirection::Out));
    builder.build().expect("unknown-kind graph")
}

fn port(node: NodeId, key: &str, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn frame_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 640.0,
                height: 480.0,
            },
        },
        RendererStamp::new(FALLBACK_RENDERER, 1),
    )
    .with_fallback_size(CanvasSize {
        width: 260.0,
        height: 180.0,
    })
}
