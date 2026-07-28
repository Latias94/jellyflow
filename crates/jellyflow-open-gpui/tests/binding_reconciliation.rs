use std::cell::RefCell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::{
    OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
    OpenGpuiSceneHost, OpenGpuiSceneHostStats,
};
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{commands::GraphCommand, graph::GraphRuntime, view::ViewSessionOptions};
use open_gpui::{AppContext, Context, Entity, ParentElement, Render, Styled, Window, div};

const NODE_COUNT: usize = 10_000;
const VISIBLE_NODE_COUNT: usize = 100;
const VIEW_COUNT: usize = 4;
const QUIESCENT_FRAME_COUNT: usize = 120;
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

struct FourViewScene {
    hosts: Vec<Entity<OpenGpuiSceneHost>>,
}

impl Render for FourViewScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().children(self.hosts.clone())
    }
}

#[open_gpui::test]
fn one_node_change_updates_only_its_binding_across_four_views_at_scale(
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
            hosts.push(cx.new(|_| OpenGpuiSceneHost::new(view, registry, visible_viewport())));
        }
        *scene_hosts.borrow_mut() = hosts.clone();
        let _ = window;
        FourViewScene { hosts }
    });
    draw(cx);

    let hosts = captured_hosts.borrow().clone();
    assert_eq!(hosts.len(), VIEW_COUNT);
    for host in &hosts {
        cx.read_entity(host, |host, _| {
            assert_binding_stats(host.stats(), VISIBLE_NODE_COUNT as u64, 0, 0);
            assert_presentation_work(host.stats(), 1, VISIBLE_NODE_COUNT as u64);
        });
    }
    for host in &hosts {
        cx.update_entity(host, |_, cx| cx.notify());
    }
    draw(cx);
    for host in &hosts {
        cx.read_entity(host, |host, _| {
            assert_binding_stats(host.stats(), VISIBLE_NODE_COUNT as u64, 0, 0);
            assert_presentation_work(host.stats(), 1, VISIBLE_NODE_COUNT as u64);
        });
    }
    let changed = NodeId::from_u128(43);
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
            assert_binding_stats(host.stats(), VISIBLE_NODE_COUNT as u64, 1, 1);
            assert_presentation_work(host.stats(), 2, VISIBLE_NODE_COUNT as u64 + 1);
            assert_eq!(host.stats().mounts(), VISIBLE_NODE_COUNT as u64);
        });
    }

    let changed_value = captured_bindings
        .borrow()
        .iter()
        .find_map(|binding| {
            cx.read_entity(binding, |binding, _| {
                let authoring = binding.authoring().expect("visible binding stays live");
                (authoring.node_id() == changed).then(|| {
                    assert_eq!(authoring.version().revision(), 1);
                    authoring.node().data.clone()
                })
            })
        })
        .expect("changed visible binding");
    assert_eq!(changed_value, serde_json::json!({ "value": 42 }));

    for _ in 0..QUIESCENT_FRAME_COUNT {
        for host in &hosts {
            cx.update_entity(host, |_, cx| cx.notify());
        }
        draw(cx);
    }
    for host in &hosts {
        cx.read_entity(host, |host, _| {
            assert_binding_stats(host.stats(), VISIBLE_NODE_COUNT as u64, 1, 1);
            assert_presentation_work(host.stats(), 2, VISIBLE_NODE_COUNT as u64 + 1);
        });
    }
}

#[open_gpui::test]
fn ten_thousand_node_host_sweep_keeps_strong_instances_within_fixed_budgets(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(view, uncaptured_registry(), visible_viewport())
    });
    draw(cx);

    for first_visible in (VISIBLE_NODE_COUNT..NODE_COUNT).step_by(VISIBLE_NODE_COUNT) {
        cx.update_entity(&host, |host, cx| {
            host.set_frame_bounds(viewport_at(first_visible));
            cx.notify();
        });
        draw(cx);
    }

    cx.read_entity(&host, |host, _| {
        let stats = host.stats();
        let fixed_budget = stats
            .active_nodes()
            .saturating_add(OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY)
            .saturating_add(OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);
        assert_eq!(stats.mounts(), NODE_COUNT as u64);
        assert_eq!(stats.active_nodes(), VISIBLE_NODE_COUNT);
        assert!(stats.parked_nodes() <= OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY);
        assert!(stats.protected_instances() <= OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);
        assert!(stats.strong_instances() <= fixed_budget);
        assert!(stats.strong_instance_high_water() <= fixed_budget);
    });
}

fn assert_binding_stats(
    stats: OpenGpuiSceneHostStats,
    snapshot_builds: u64,
    updates: u64,
    reconciliation_visits: u64,
) {
    assert_eq!(stats.binding_mount_initializations(), snapshot_builds);
    assert_eq!(stats.binding_updates(), updates);
    assert_eq!(stats.binding_notifications(), updates);
    assert_eq!(stats.binding_reconciliation_visits(), reconciliation_visits,);
}

fn assert_presentation_work(stats: OpenGpuiSceneHostStats, queries: u64, materializations: u64) {
    assert_eq!(stats.binding_presentation_queries(), queries);
    assert_eq!(
        stats.binding_presentation_materializations(),
        materializations
    );
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn registry(bindings: Rc<RefCell<Vec<Entity<OpenGpuiNodeBinding>>>>) -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        bindings.borrow_mut().push(binding.clone());
        cx.new(|_| BindingProbeNode { binding })
    });
    registry.seal().expect("valid test registry")
}

fn uncaptured_registry() -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |binding, _window, cx| {
        cx.new(|_| BindingProbeNode { binding })
    });
    registry.seal().expect("valid test registry")
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
        .register(
            NodeKindSpec::builder(KIND, "Binding node", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                })
                .build(),
        )
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
