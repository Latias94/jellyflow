use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, PortId,
};
use jellyflow_open_gpui::activity::{OpenGpuiActivityKind, OpenGpuiActivityLease};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::{
    OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
    OpenGpuiSceneHost,
};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRegistryBuilder,
    OpenGpuiNodeRendererDescriptor,
};
use jellyflow_open_gpui::retained_field::{
    RetainedField, RetainedFieldError, RetainedFieldKey, RetainedFieldPhase,
    RetainedFieldRetention, RetainedFieldStoreError,
};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::view::{RendererStamp, ViewSessionOptions};
use open_gpui::{App, AppContext, Context, Entity, ParentElement, Render, Styled, Window, div};
use open_gpui_canvas_jellyflow::nodes::{
    NativeNodeResources, WORKFLOW_LLM_DEFAULT_SIZE, WORKFLOW_LLM_KIND, native_node_catalog,
    register_native_nodes, workflow_prompt_selector,
};
use open_gpui_canvas_jellyflow::nodes::{WORKFLOW_PROMPT_FIELD, WorkflowFieldStore};
use open_gpui_ui_components::gpui_adapter::init_text_input;

const KIND: &str = "test.workflow-overflow";
const RENDERER: &str = "test.native.workflow-overflow";
const INITIAL_NODES: usize = OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 1;
const CHURN_NODES: usize = OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY + 1;
const NODE_SPACING: f32 = 240.0;

struct OverflowNode {
    _field: RetainedField<String>,
}

fn register_overflow_renderer<F, V>(registry: &mut OpenGpuiNodeRegistryBuilder, factory: F)
where
    F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
    V: Render + 'static,
{
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new(RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("Workflow overflow"),
            move |binding, window, cx| Ok(factory(binding, window, cx)),
        ))
        .expect("unique workflow overflow renderer descriptor");
}

#[open_gpui::test]
fn edit_is_rejected_before_publish_when_host_and_external_field_budgets_are_full(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(atomic_capacity_graph())
        .expect("valid atomic capacity graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach atomic capacity view");
    let store = WorkflowFieldStore::with_capacity(NonZeroUsize::new(1).unwrap());
    let targets = Rc::new(RefCell::new(BTreeMap::<NodeId, Entity<OverflowNode>>::new()));
    let factory_targets = targets.clone();
    let factory_store = store.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_overflow_renderer(&mut registry, move |binding, _window, cx| {
        let authoring = binding.read(cx).authoring().expect("live atomic binding");
        let node = authoring.node_id();
        let index = node_index(node);
        let mut field = RetainedField::mount(
            authoring.observe_field(|_| "authoritative".to_owned()),
            RetainedFieldKey::new(WORKFLOW_PROMPT_FIELD).unwrap(),
            factory_store.clone(),
        )
        .expect("mount atomic retained field");
        if index < OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
            field
                .edit(format!("protected-{index}"))
                .expect("fill host protected budget");
        }
        let mounted = cx.new(|_| OverflowNode { _field: field });
        if index >= OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
            factory_targets.borrow_mut().insert(node, mounted.clone());
        }
        mounted
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal(&node_catalog)
                .expect("valid atomic capacity registry"),
            initial_bounds(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
    });

    let first_overflow = targets
        .borrow()
        .get(&node_id(OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY))
        .expect("first overflow target")
        .clone();
    cx.update_entity(&first_overflow, |node, _| {
        node._field
            .edit("first externalized".to_owned())
            .expect("fill the external field budget");
        assert_eq!(
            node._field.retention(),
            RetainedFieldRetention::Externalized
        );
    });
    assert_eq!(store.len(), 1);

    let rejected = targets
        .borrow()
        .get(&node_id(OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 1))
        .expect("rejected overflow target")
        .clone();
    let result = cx.update_entity(&rejected, |node, _| {
        let result = node._field.edit("must not publish".to_owned());
        assert_eq!(node._field.local(), "authoritative");
        assert_eq!(node._field.phase(), RetainedFieldPhase::Clean);
        assert_eq!(node._field.retention(), RetainedFieldRetention::Idle);
        result
    });
    assert!(matches!(
        result,
        Err(RetainedFieldError::Store(
            RetainedFieldStoreError::CapacityExceeded { capacity: 1 }
        ))
    ));
    assert_eq!(store.len(), 1);
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
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(overflow_graph())
        .expect("valid overflow graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let store = WorkflowFieldStore::new();
    let overflow_node = Rc::new(RefCell::new(None::<NodeId>));
    let mounted_overflow = Rc::new(RefCell::new(None::<Entity<OverflowNode>>));
    let overflow_values = Rc::new(RefCell::new(Vec::<String>::new()));
    let initial_mounts = Rc::new(RefCell::new(BTreeMap::<NodeId, u32>::new()));
    let mut registry = OpenGpuiNodeRegistry::builder();
    let factory_store = store.clone();
    let factory_overflow_node = overflow_node.clone();
    let factory_mounted_overflow = mounted_overflow.clone();
    let factory_overflow_values = overflow_values.clone();
    let factory_initial_mounts = initial_mounts.clone();
    register_overflow_renderer(&mut registry, move |binding, _window, cx| {
        let authoring = binding
            .read(cx)
            .authoring()
            .expect("overflow factory receives a live authoring binding");
        let node = authoring.node_id();
        let index = node_index(node);
        let is_initial = index < INITIAL_NODES;
        let is_overflow_target = index + 1 == INITIAL_NODES;
        let mount = factory_initial_mounts
            .borrow_mut()
            .entry(node)
            .and_modify(|mount| *mount += 1)
            .or_insert(1)
            .to_owned();
        let key = RetainedFieldKey::new(WORKFLOW_PROMPT_FIELD).expect("constant field key");
        let mut field = RetainedField::mount(
            authoring.observe_field(|_| String::from("authoritative")),
            key,
            factory_store.clone(),
        )
        .expect("retained field mounts while binding is live");
        if is_initial && !is_overflow_target && mount == 1 {
            field
                .edit(format!("unsaved-{index}"))
                .expect("atomically retain unsaved overflow value");
        }

        if is_overflow_target {
            let mut current = factory_overflow_node.borrow_mut();
            assert!(current.is_none() || *current == Some(node));
            *current = Some(node);
            if mount > 1 {
                assert_eq!(field.retention(), RetainedFieldRetention::Externalized);
                factory_overflow_values
                    .borrow_mut()
                    .push(field.local().clone());
            }
        }

        let mounted = cx.new(|_| OverflowNode { _field: field });
        if is_overflow_target {
            *factory_mounted_overflow.borrow_mut() = Some(mounted.clone());
        }
        mounted
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&node_catalog).expect("valid test registry"),
            initial_bounds(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let overflow = overflow_node
        .borrow()
        .expect("the sixty-fifth initial instance must overflow");
    let first_mount = mounted_overflow
        .borrow_mut()
        .take()
        .expect("the sixty-fifth node is mounted before culling");
    cx.update_entity(&first_mount, |node, _| {
        node._field
            .edit(format!("unsaved-{}", node_index(overflow)))
            .expect("the full protected budget falls back to external storage");
        assert_eq!(
            node._field.retention(),
            RetainedFieldRetention::Externalized
        );
        overflow_values
            .borrow_mut()
            .push(node._field.local().clone());
    });
    drop(first_mount);
    assert_eq!(overflow_values.borrow().len(), 1);
    assert_eq!(store.len(), 1);

    // Do not draw after the edit. Cull immediately, then churn the eligible LRU until this exact
    // mount is evicted; the edit must already be protected or externalized when `edit` returns.
    for index in 0..CHURN_NODES {
        cx.update_entity(&host, |host, cx| {
            host.set_frame_bounds(churn_bounds(index));
            cx.notify();
        });
        draw(cx);
    }
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
        assert!(
            host.resource_snapshot().eligible_parked_nodes()
                <= OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY
        );
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

    let mounted = mounted_overflow
        .borrow_mut()
        .take()
        .expect("the remounted overflow field remains observable");
    cx.update_entity(&mounted, |node, _| {
        node._field
            .edit("authoritative".to_owned())
            .expect("atomically publish clean field");
        assert_eq!(node._field.retention(), RetainedFieldRetention::Idle,);
    });
    assert!(
        store.is_empty(),
        "a clean overflow field must delete its stale externalized snapshot"
    );
}

#[open_gpui::test]
fn sixty_fifth_workflow_focus_is_rejected_before_the_node_can_park(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let catalog = focus_overflow_catalog();
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(focus_overflow_graph())
        .expect("valid workflow focus overflow graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach workflow focus overflow view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    let resources = cx.update(NativeNodeResources::ready_default);
    register_native_nodes(&mut registry, resources);
    register_overflow_renderer(&mut registry, |binding, _window, cx| {
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
            registry
                .seal(&catalog)
                .expect("valid focus overflow registry"),
            initial_bounds(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().protected_instances(),
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
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(churn_bounds(0));
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(host.resource_snapshot().parked_nodes() >= 1);
        assert!(host.resource_snapshot().eligible_parked_nodes() >= 1);
    });
    cx.update(|window, cx| assert!(window.focused(cx).is_none()));
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn overflow_graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xd1af_7000));
    for index in 0..INITIAL_NODES + CHURN_NODES {
        graph.insert_node(node_id(index), overflow_node_record(index));
    }
    graph.build().expect("overflow graph")
}

fn atomic_capacity_graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xd1af_7001));
    for index in 0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 2 {
        let mut node = overflow_node_record(0);
        node.pos = CanvasPoint::default();
        graph.insert_node(node_id(index), node);
    }
    graph.build().expect("atomic capacity graph")
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
        .register(NodeKindSpec::builder(KIND).build())
        .expect("register overflow kind");
    catalog
}

fn focus_overflow_catalog() -> NodeCatalog {
    let mut catalog = native_node_catalog().expect("native workflow catalog");
    catalog
        .register(NodeKindSpec::builder(KIND).build())
        .expect("register focus protector kind");
    catalog
}

fn focus_overflow_graph() -> jellyflow_core::Graph {
    let catalog = focus_overflow_catalog();
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xd1af_9000));
    for index in 0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
        graph.insert_node(node_id(index), overflow_node_record(index));
    }
    let creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_LLM_KIND),
            focus_overflow_llm_id(),
            CanvasRect {
                origin: CanvasPoint::default(),
                size: WORKFLOW_LLM_DEFAULT_SIZE,
            },
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
