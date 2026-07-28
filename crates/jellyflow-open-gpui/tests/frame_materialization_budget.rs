use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    graph::GraphRuntime,
    view::{
        DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET, FrameMaterializationOverflow, ViewSessionOptions,
    },
};
use open_gpui::{AppContext, Context, Entity, ParentElement, Render, Window, div};

const NODE_COUNT: usize = 10_000;
const KIND: &str = "test.frame-budget-node";
const RENDERER: &str = "test.native.frame-budget-node";

struct FrameBudgetNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

impl Render for FrameBudgetNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let _ = self
            .binding
            .read(cx)
            .authoring()
            .expect("mounted binding opens authoring")
            .node_id();
        div().child("frame budget probe")
    }
}

#[open_gpui::test]
fn high_fanout_host_bounds_active_instances_and_exposes_overflow(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(star_graph())
            .unwrap(),
    );
    let scene_runtime = Rc::clone(&runtime);
    let (host, cx) = cx.add_window_view(move |_, _| {
        let view = scene_runtime
            .attach_view(ViewSessionOptions::default())
            .unwrap();
        OpenGpuiSceneHost::new(view, registry(), hub_viewport())
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        let stats = host.stats();
        let overflow = host
            .last_frame_materialization_overflow()
            .expect("a rendered frame must publish overflow diagnostics");
        assert_eq!(
            stats.active_nodes(),
            DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET + 1,
        );
        assert_eq!(stats.strong_instances(), stats.active_nodes());
        assert_eq!(stats.mounts(), stats.active_nodes() as u64);
        assert!(overflow.incident_scan_truncated());
        assert_eq!(overflow.visible_nodes(), 0);
        assert_eq!(overflow.required_nodes(), 0);
        assert_eq!(overflow.routing_dependency_nodes(), 0);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(CanvasRect {
            origin: CanvasPoint {
                x: -10_000.0,
                y: -10_000.0,
            },
            size: hub_viewport().size,
        });
        cx.notify();
    });
    draw(cx);

    cx.update_entity(&host, |host, _| {
        assert!(
            host.last_frame_materialization_overflow()
                .is_some_and(FrameMaterializationOverflow::is_empty),
            "the latest offscreen frame fits within the materialization budget",
        );
        let overflow = host
            .take_unobserved_frame_materialization_overflow()
            .expect("an unobserved overflow survives a later empty frame");
        assert!(overflow.incident_scan_truncated());
        assert_eq!(
            host.take_unobserved_frame_materialization_overflow(),
            None,
            "one-shot overflow evidence drains exactly once",
        );
    });
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn registry() -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |binding, _window, cx| {
        cx.new(|_| FrameBudgetNode { binding })
    });
    registry.seal().expect("valid test registry")
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Frame budget node", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                })
                .build(),
        )
        .unwrap();
    catalog
}

fn star_graph() -> jellyflow_core::Graph {
    let hub = NodeId::from_u128(1);
    let hub_port = PortId::from_u128(100_000);
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x52_000));
    graph.insert_node(hub, graph_node(CanvasPoint::default(), vec![hub_port]));
    graph.insert_port(hub_port, graph_port(hub, PortDirection::Out));

    for index in 1..NODE_COUNT {
        let target = NodeId::from_u128(index as u128 + 1);
        let target_port = PortId::from_u128(100_000 + index as u128);
        graph.insert_node(
            target,
            graph_node(
                CanvasPoint {
                    x: 10_000.0 + index as f32 * 200.0,
                    y: 0.0,
                },
                vec![target_port],
            ),
        );
        graph.insert_port(target_port, graph_port(target, PortDirection::In));
        graph.insert_edge(
            EdgeId::from_u128(200_000 + index as u128),
            Edge::new(EdgeKind::Data, hub_port, target_port),
        );
    }
    graph.build().unwrap()
}

fn graph_node(position: CanvasPoint, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(KIND),
        kind_version: 1,
        pos: position,
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 180.0,
            height: 100.0,
        }),
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn graph_port(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(match direction {
            PortDirection::In => "in",
            PortDirection::Out => "out",
        }),
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

fn hub_viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 200.0,
            height: 120.0,
        },
    }
}
