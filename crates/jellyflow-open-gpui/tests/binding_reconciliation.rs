use std::cell::RefCell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand,
    graph::GraphRuntime,
    view::{RendererStamp, ViewSessionOptions},
};
use open_gpui::{AppContext, Context, Entity, ParentElement, Render, Styled, Window, div};

const NODE_COUNT: usize = 200;
const VISIBLE_NODE_COUNT: usize = 20;
const VIEW_COUNT: usize = 2;
const QUIESCENT_FRAME_COUNT: usize = 2;
const KIND: &str = "test.binding-node";
const RENDERER: &str = "test.native.binding-node";

struct BindingProbeNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

impl Render for BindingProbeNode {
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
        div().child("binding probe")
    }
}

struct MultiViewScene {
    hosts: Vec<Entity<OpenGpuiSceneHost>>,
}

impl Render for MultiViewScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().children(self.hosts.clone())
    }
}

#[open_gpui::test]
fn one_node_change_updates_only_its_binding_across_independent_views(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(GraphRuntime::builder(catalog()).build(graph()).unwrap());
    let captured_hosts = Rc::new(RefCell::new(Vec::new()));
    let captured_bindings = Rc::new(RefCell::new(Vec::new()));
    let scene_hosts = captured_hosts.clone();
    let scene_bindings = captured_bindings.clone();
    let scene_runtime = runtime.clone();

    let (_scene, cx) = cx.add_window_view(move |window, cx| {
        let mut hosts = Vec::with_capacity(VIEW_COUNT);
        for _ in 0..VIEW_COUNT {
            let view = scene_runtime
                .attach_view(ViewSessionOptions::default())
                .unwrap();
            let registry = registry(scene_bindings.clone());
            hosts.push(cx.new(|_| {
                OpenGpuiSceneHost::new(view, registry, visible_viewport())
                    .expect("valid Open GPUI scene host")
            }));
        }
        *scene_hosts.borrow_mut() = hosts.clone();
        let _ = window;
        MultiViewScene { hosts }
    });
    draw(cx);

    let hosts = captured_hosts.borrow().clone();
    assert_eq!(hosts.len(), VIEW_COUNT);
    for host in &hosts {
        cx.read_entity(host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), VISIBLE_NODE_COUNT);
        });
    }
    for host in &hosts {
        cx.update_entity(host, |_, cx| cx.notify());
    }
    draw(cx);
    for host in &hosts {
        cx.read_entity(host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), VISIBLE_NODE_COUNT);
        });
    }
    let changed = NodeId::from_u128((VISIBLE_NODE_COUNT / 2 + 1) as u128);
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            changed,
            serde_json::json!({ "value": 42 }),
        ))
        .into_result()
        .unwrap();
    cx.run_until_parked();

    for host in &hosts {
        cx.read_entity(host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), VISIBLE_NODE_COUNT);
        });
    }

    let changed_values = captured_bindings
        .borrow()
        .iter()
        .filter_map(|binding| {
            cx.read_entity(binding, |binding, _| {
                let authoring = binding.authoring().expect("visible binding stays live");
                (authoring.node_id() == changed).then(|| {
                    assert_eq!(authoring.version().revision(), 1);
                    authoring.node().data.clone()
                })
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(changed_values.len(), VIEW_COUNT);
    assert!(
        changed_values
            .iter()
            .all(|value| value == &serde_json::json!({ "value": 42 }))
    );

    for _ in 0..QUIESCENT_FRAME_COUNT {
        for host in &hosts {
            cx.update_entity(host, |_, cx| cx.notify());
        }
        draw(cx);
    }
    for host in &hosts {
        cx.read_entity(host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), VISIBLE_NODE_COUNT);
        });
    }
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn registry(bindings: Rc<RefCell<Vec<Entity<OpenGpuiNodeBinding>>>>) -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new(RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("Binding node"),
            move |binding, _window, cx| {
                bindings.borrow_mut().push(binding.clone());
                Ok(cx.new(|_| BindingProbeNode { binding }))
            },
        ))
        .expect("register captured binding renderer");
    registry.seal(&catalog()).expect("valid test registry")
}

fn graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x51_000));
    for index in 0..NODE_COUNT {
        graph.insert_node(
            NodeId::from_u128(index as u128 + 1),
            Node {
                kind: NodeKindKey::new(KIND),
                kind_version: 1,
                pos: CanvasPoint {
                    x: index as f32 * 200.0,
                    y: 0.0,
                },
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
                ports: Vec::new(),
                data: serde_json::Value::Null,
            },
        );
    }
    graph.build().unwrap()
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND).build())
        .unwrap();
    catalog
}

fn visible_viewport() -> CanvasRect {
    viewport_at(0)
}

fn viewport_at(first_visible: usize) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: first_visible as f32 * 200.0,
            y: 0.0,
        },
        size: CanvasSize {
            width: VISIBLE_NODE_COUNT as f32 * 200.0 - 1.0,
            height: 200.0,
        },
    }
}
