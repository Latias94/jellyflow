use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, PortId,
};
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{GraphRuntime, ViewSessionOptions};
use jellyflow_open_gpui::{
    OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiNodeBinding,
    OpenGpuiNodeRegistry, OpenGpuiSceneHost, RevisionedDraft,
};
use open_gpui::{AppContext, Context, Entity, ParentElement, Render, Styled, Window, div};
use open_gpui_canvas_jellyflow::nodes::{
    NativeNodeResources, WORKFLOW_LLM_KIND, native_node_catalog, register_native_nodes,
    workflow_prompt_selector,
};
use open_gpui_canvas_jellyflow::nodes::{WORKFLOW_PROMPT_FIELD, WorkflowDraftStore};
use open_gpui_ui_components::gpui_adapter::init_text_input;

const KIND: &str = "test.workflow-overflow";
const RENDERER: &str = "test.native.workflow-overflow";
const INITIAL_NODES: usize = OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 1;
const CHURN_NODES: usize = OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY + 1;
const NODE_SPACING: f32 = 240.0;

struct OverflowNode {
    _binding: Entity<OpenGpuiNodeBinding>,
    _draft: RevisionedDraft<String>,
    _lease: Option<OpenGpuiActivityLease>,
}

struct ProtectedNode {
    _lease: OpenGpuiActivityLease,
}

impl Render for ProtectedNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().child("protected workflow state")
    }
}

impl Render for OverflowNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().child("workflow draft")
    }
}

#[open_gpui::test]
fn sixty_fifth_dirty_draft_survives_real_host_capacity_and_lru_churn(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(overflow_graph())
        .expect("valid overflow graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let store = WorkflowDraftStore::new();
    let overflow_node = Rc::new(RefCell::new(None::<NodeId>));
    let overflow_values = Rc::new(RefCell::new(Vec::<String>::new()));
    let initial_mounts = Rc::new(RefCell::new(BTreeMap::<NodeId, u32>::new()));
    let mut registry = OpenGpuiNodeRegistry::builder();
    let factory_store = store.clone();
    let factory_overflow_node = overflow_node.clone();
    let factory_overflow_values = overflow_values.clone();
    let factory_initial_mounts = initial_mounts.clone();
    registry.register(RENDERER, move |binding, _window, cx| {
        let snapshot = binding.read(cx).snapshot();
        let node = snapshot.node_id();
        let instance = snapshot.instance().clone();
        let revision = snapshot
            .read()
            .map(|facts| facts.graph_revision())
            .unwrap_or_default();
        let is_initial = node_index(node) < INITIAL_NODES;
        let mount = factory_initial_mounts
            .borrow_mut()
            .entry(node)
            .and_modify(|mount| *mount += 1)
            .or_insert(1)
            .to_owned();
        let mut draft = factory_store
            .restore(
                &instance,
                WORKFLOW_PROMPT_FIELD,
                revision,
                String::from("authoritative"),
            )
            .into_draft();
        if is_initial && mount == 1 {
            draft.edit_local(format!("unsaved-{}", node_index(node)));
        }

        let lease = if is_initial {
            match binding
                .read(cx)
                .activity()
                .acquire(OpenGpuiActivityKind::Draft)
            {
                Ok(lease) => Some(lease),
                Err(OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { capacity }) => {
                    assert_eq!(capacity, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);
                    factory_store.externalize_overflow(&instance, WORKFLOW_PROMPT_FIELD, &draft);
                    let mut current = factory_overflow_node.borrow_mut();
                    assert!(current.is_none() || *current == Some(node));
                    *current = Some(node);
                    factory_overflow_values
                        .borrow_mut()
                        .push(draft.local().clone());
                    None
                }
                Err(error) => panic!("unexpected activity error: {error}"),
            }
        } else {
            None
        };

        cx.new(|_| OverflowNode {
            _binding: binding,
            _draft: draft,
            _lease: lease,
        })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            initial_bounds(),
        )
    });
    draw(cx);

    let overflow = overflow_node
        .borrow()
        .expect("the sixty-fifth initial instance must overflow");
    assert_eq!(overflow_values.borrow().len(), 1);
    assert_eq!(store.len(), 1);

    for index in 0..CHURN_NODES {
        cx.update_entity(&host, |host, cx| {
            host.set_frame_bounds(churn_bounds(index));
            cx.notify();
        });
        draw(cx);
    }
    cx.read_entity(&host, |host, _| {
        assert!(host.stats().evictions() >= 1);
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
        assert!(host.stats().eligible_parked_nodes() <= OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(initial_bounds());
        cx.notify();
    });
    draw(cx);

    assert_eq!(
        initial_mounts.borrow().get(&overflow),
        Some(&2),
        "the overflow instance must mount fresh after eligible LRU eviction"
    );
    assert_eq!(
        overflow_values.borrow().as_slice(),
        [
            format!("unsaved-{}", node_index(overflow)),
            format!("unsaved-{}", node_index(overflow))
        ],
        "the same unsaved draft must be restored before the remounted entity is reclaimed again"
    );
}

#[open_gpui::test]
fn sixty_fifth_workflow_focus_is_rejected_before_the_node_can_park(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let catalog = focus_overflow_catalog();
    let runtime = GraphRuntime::builder(catalog)
        .build(focus_overflow_graph())
        .expect("valid workflow focus overflow graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach workflow focus overflow view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    let resources = cx.update(NativeNodeResources::ready_default);
    register_native_nodes(&mut registry, resources);
    registry.register(RENDERER, |binding, _window, cx| {
        let lease = binding
            .read(cx)
            .activity()
            .acquire(OpenGpuiActivityKind::Draft)
            .expect("the first 64 instances fit the protected budget");
        cx.new(|_| ProtectedNode { _lease: lease })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid focus overflow registry"),
            initial_bounds(),
        )
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
    });

    let prompt_selector = workflow_prompt_selector(focus_overflow_llm_id());
    let prompt = cx
        .debug_bounds(&prompt_selector)
        .expect("sixty-fifth workflow prompt");
    cx.simulate_click(prompt.center(), Default::default());
    draw(cx);

    assert!(
        !cx.debug_selector_is_focused(&prompt_selector),
        "capacity rejection must synchronously move focus off the unprotected workflow node"
    );
    cx.update(|window, cx| assert!(window.focused(cx).is_none()));
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
        assert!(host.stats().protected_overflows() >= 1);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(churn_bounds(0));
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(host.stats().parked_nodes() >= 1);
        assert!(host.stats().eligible_parked_nodes() >= 1);
    });
    cx.update(|window, cx| assert!(window.focused(cx).is_none()));
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn overflow_graph() -> jellyflow::core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xd1af_7000));
    for index in 0..INITIAL_NODES + CHURN_NODES {
        graph.insert_node(node_id(index), overflow_node_record(index));
    }
    graph.build().expect("overflow graph")
}

fn overflow_node_record(index: usize) -> Node {
    let x = if index < INITIAL_NODES {
        0.0
    } else {
        churn_x(index - INITIAL_NODES)
    };
    Node {
        kind: NodeKindKey::new(KIND),
        kind_version: 1,
        pos: CanvasPoint { x, y: 0.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 120.0,
            height: 80.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Workflow overflow", RENDERER)
                .default_size(CanvasSize {
                    width: 120.0,
                    height: 80.0,
                })
                .build(),
        )
        .expect("register overflow kind");
    catalog
}

fn focus_overflow_catalog() -> NodeCatalog {
    let mut catalog = native_node_catalog().expect("native workflow catalog");
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Workflow focus protector", RENDERER)
                .default_size(CanvasSize {
                    width: 120.0,
                    height: 80.0,
                })
                .build(),
        )
        .expect("register focus protector kind");
    catalog
}

fn focus_overflow_graph() -> jellyflow::core::Graph {
    let catalog = focus_overflow_catalog();
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xd1af_9000));
    for index in 0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
        graph.insert_node(node_id(index), overflow_node_record(index));
    }
    let creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_LLM_KIND),
            focus_overflow_llm_id(),
            CanvasPoint::default(),
            [
                PortId::from_u128(0xd1af_f101),
                PortId::from_u128(0xd1af_f102),
            ],
        )
        .expect("instantiate overflow workflow LLM");
    let (node_id, node, ports) = creation.into_parts();
    graph.insert_node(node_id, node);
    for (port_id, port) in ports {
        graph.insert_port(port_id, port);
    }
    graph.build().expect("workflow focus overflow graph")
}

fn focus_overflow_llm_id() -> NodeId {
    NodeId::from_u128(0xd1af_f001)
}

fn node_id(index: usize) -> NodeId {
    NodeId::from_u128(0xd1af_8000 + index as u128)
}

fn node_index(node: NodeId) -> usize {
    (node.0.as_u128() - 0xd1af_8000) as usize
}

fn churn_x(index: usize) -> f32 {
    1_000.0 + index as f32 * NODE_SPACING
}

fn initial_bounds() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint { x: -10.0, y: -10.0 },
        size: CanvasSize {
            width: 160.0,
            height: 120.0,
        },
    }
}

fn churn_bounds(index: usize) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: churn_x(index) - 10.0,
            y: -10.0,
        },
        size: CanvasSize {
            width: 160.0,
            height: 120.0,
        },
    }
}
