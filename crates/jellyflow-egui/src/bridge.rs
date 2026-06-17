use std::collections::{BTreeMap, HashMap};

use eframe::egui::{Color32, Stroke};
use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphId, NodeId, NodeKindKey, PortCapacity,
    PortDirection, PortId, PortKind,
};
use jellyflow::layout::{LayoutEngineRegistry, builtin_layout_engine_registry};
use jellyflow::runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow::runtime::runtime::connection::{ConnectEdgeRequest, ConnectionHandleRef};
use jellyflow::runtime::runtime::create_node::CreateNodeRequest;
use jellyflow::runtime::runtime::drag::{NodeNudgeDirection, NodeNudgeRequest};
use jellyflow::runtime::runtime::fit_view::{
    FitViewComputeOptions, FitViewNodeInfo, compute_fit_view_target,
};
use jellyflow::runtime::runtime::geometry::{
    BezierEdgeOptions, HandleBounds, HandlePosition, bezier_edge_path,
};
use jellyflow::runtime::runtime::keyboard::{
    KeyboardActionError, KeyboardActionOutcome, KeyboardIntent,
};
use jellyflow::runtime::runtime::layout::{LayoutApplyError, LayoutEngineRequest};
use jellyflow::runtime::runtime::measurement::{MeasuredHandle, NodeMeasurement};
use jellyflow::runtime::runtime::resize::{
    NodePointerResizeRequest, NodeResizeDirection, NodeResizeSession,
    NodeResizeSessionUpdateRequest,
};
use jellyflow::runtime::runtime::selection::{NodePointerDownInput, SelectionBoxInput};
use jellyflow::runtime::runtime::viewport::{
    ViewportPanRequest, ViewportTransform, ViewportZoomRequest,
};
use jellyflow::runtime::schema::{NodeKindViewDescriptor, NodeRegistry, NodeSchema, PortDecl};
use jellyflow::runtime::{DispatchError, DispatchOutcome, NodeGraphStore};
use serde_json::json;
use thiserror::Error;

use crate::state::{
    ActiveCanvasInteraction, CanvasSnapshot, LayoutPresetChoice, layout_scope_for_selection,
};

const DEFAULT_NODE_WIDTH: f32 = 180.0;
const DEFAULT_NODE_HEIGHT: f32 = 86.0;
const DEFAULT_HANDLE_SIZE: f32 = 10.0;

/// Visual style mapped from an adapter-owned renderer key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRendererStyle {
    pub fill: Color32,
    pub stroke: Color32,
    pub accent: Color32,
    pub text: Color32,
}

impl NodeRendererStyle {
    pub const fn new(fill: Color32, stroke: Color32, accent: Color32, text: Color32) -> Self {
        Self {
            fill,
            stroke,
            accent,
            text,
        }
    }

    pub const fn task() -> Self {
        Self::new(
            Color32::from_rgb(245, 248, 252),
            Color32::from_rgb(124, 139, 160),
            Color32::from_rgb(42, 106, 166),
            Color32::from_rgb(31, 41, 55),
        )
    }

    pub const fn decision() -> Self {
        Self::new(
            Color32::from_rgb(255, 248, 235),
            Color32::from_rgb(191, 129, 45),
            Color32::from_rgb(200, 83, 44),
            Color32::from_rgb(61, 46, 28),
        )
    }

    pub const fn data() -> Self {
        Self::new(
            Color32::from_rgb(238, 250, 246),
            Color32::from_rgb(79, 146, 121),
            Color32::from_rgb(18, 128, 96),
            Color32::from_rgb(27, 53, 48),
        )
    }

    pub const fn output() -> Self {
        Self::new(
            Color32::from_rgb(249, 244, 255),
            Color32::from_rgb(135, 107, 177),
            Color32::from_rgb(108, 81, 158),
            Color32::from_rgb(49, 38, 70),
        )
    }

    pub const fn fallback() -> Self {
        Self::new(
            Color32::from_rgb(247, 247, 246),
            Color32::from_rgb(142, 142, 135),
            Color32::from_rgb(82, 82, 74),
            Color32::from_rgb(36, 36, 32),
        )
    }

    pub fn selected_stroke(self) -> Stroke {
        Stroke::new(2.0, self.accent)
    }
}

/// Adapter-owned renderer catalog keyed by `NodeKindViewDescriptor::renderer_key`.
#[derive(Debug, Clone)]
pub struct RendererCatalog {
    fallback: NodeRendererStyle,
    by_renderer_key: BTreeMap<String, NodeRendererStyle>,
}

impl Default for RendererCatalog {
    fn default() -> Self {
        Self::with_builtin_styles()
    }
}

impl RendererCatalog {
    pub fn new() -> Self {
        Self {
            fallback: NodeRendererStyle::fallback(),
            by_renderer_key: BTreeMap::new(),
        }
    }

    pub fn with_builtin_styles() -> Self {
        let mut catalog = Self::new();
        catalog
            .register("task-card", NodeRendererStyle::task())
            .register("decision-card", NodeRendererStyle::decision())
            .register("data-card", NodeRendererStyle::data())
            .register("output-card", NodeRendererStyle::output());
        catalog
    }

    pub fn register(
        &mut self,
        renderer_key: impl Into<String>,
        style: NodeRendererStyle,
    ) -> &mut Self {
        self.by_renderer_key.insert(renderer_key.into(), style);
        self
    }

    pub fn style_for_descriptor(&self, descriptor: &NodeKindViewDescriptor) -> NodeRendererStyle {
        self.style_for_key(&descriptor.renderer_key)
    }

    pub fn style_for_key(&self, renderer_key: &str) -> NodeRendererStyle {
        self.by_renderer_key
            .get(renderer_key)
            .copied()
            .unwrap_or(self.fallback)
    }
}

#[derive(Debug, Error)]
pub enum DemoGraphError {
    #[error("demo graph create node failed: {0}")]
    Create(String),
    #[error("demo graph connect failed: {0}")]
    Connect(String),
}

/// Owns the Jellyflow store and exposes small adapter-facing commands for egui widgets.
pub struct JellyflowEguiBridge {
    store: NodeGraphStore,
    node_registry: NodeRegistry,
    layout_registry: &'static LayoutEngineRegistry,
    renderers: RendererCatalog,
}

impl JellyflowEguiBridge {
    pub fn new(
        store: NodeGraphStore,
        node_registry: NodeRegistry,
        layout_registry: &'static LayoutEngineRegistry,
        renderers: RendererCatalog,
    ) -> Self {
        Self {
            store,
            node_registry,
            layout_registry,
            renderers,
        }
    }

    pub fn demo() -> Result<Self, DemoGraphError> {
        let registry = demo_node_registry();
        let mut bridge = Self::new(
            NodeGraphStore::new(
                Graph::new(GraphId::new()),
                NodeGraphViewState::default(),
                NodeGraphEditorConfig::default().with_spatial_index_enabled(true),
            ),
            registry,
            builtin_layout_engine_registry(),
            RendererCatalog::default(),
        );
        bridge.populate_demo_graph()?;
        Ok(bridge)
    }

    pub fn store(&self) -> &NodeGraphStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut NodeGraphStore {
        &mut self.store
    }

    pub fn registry(&self) -> &NodeRegistry {
        &self.node_registry
    }

    pub fn renderers(&self) -> &RendererCatalog {
        &self.renderers
    }

    pub fn descriptors(&self) -> Vec<NodeKindViewDescriptor> {
        self.node_registry.view_descriptors()
    }

    pub fn descriptor_for_node(&self, node: NodeId) -> Option<NodeKindViewDescriptor> {
        self.store
            .graph()
            .nodes()
            .get(&node)
            .and_then(|node| self.node_registry.view_descriptor(&node.kind))
    }

    pub fn rebuild_snapshot(
        &mut self,
        previous: &CanvasSnapshot,
        viewport_rect: eframe::egui::Rect,
    ) -> CanvasSnapshot {
        self.report_snapshot_measurements(previous);
        let viewport_size = CanvasSize {
            width: viewport_rect.width().max(1.0),
            height: viewport_rect.height().max(1.0),
        };
        let transform =
            ViewportTransform::from_view_state(self.store.view_state()).unwrap_or_else(|| {
                ViewportTransform::new(CanvasPoint::default(), 1.0)
                    .expect("default viewport transform is valid")
            });
        let layout_facts = self.store.layout_facts_query(viewport_size);
        let rendering = layout_facts.rendering.clone();
        let visible_node_render_order =
            visible_or_full_nodes(&rendering.visible_node_render_order, &rendering.node_order);
        let visible_node_ids =
            visible_or_full_nodes(&rendering.visible_node_ids, &rendering.node_order);
        let visible_edge_render_order =
            visible_or_full_edges(&rendering.visible_edge_render_order, &rendering.edge_order);
        let visible_edge_ids =
            visible_or_full_edges(&rendering.visible_edge_ids, &rendering.edge_order);

        let mut node_rects = BTreeMap::new();
        for node in &visible_node_ids {
            if let Some(rect) = self.node_rect(*node) {
                node_rects.insert(*node, rect);
            }
        }

        let mut handle_bounds = HashMap::new();
        for node in &visible_node_ids {
            for (handle, bounds) in self.default_handle_bounds(*node) {
                handle_bounds.insert(handle, bounds);
            }
        }

        let edge_paths = layout_facts
            .visible_edge_positions
            .iter()
            .filter_map(|position| {
                bezier_edge_path(
                    position.position.source,
                    position.position.target,
                    BezierEdgeOptions::default(),
                )
                .map(|path| (position.edge, path))
            })
            .collect();

        CanvasSnapshot {
            viewport_rect,
            viewport_size,
            transform,
            node_rects,
            handle_bounds,
            edge_paths,
            rendering,
            layout_facts,
            visible_node_ids,
            visible_node_render_order,
            visible_edge_ids,
            visible_edge_render_order,
        }
    }

    pub fn node_rect(&self, node: NodeId) -> Option<CanvasRect> {
        let node_record = self.store.graph().nodes().get(&node)?;
        if node_record.hidden || !node_record.pos.is_finite() {
            return None;
        }
        let size = node_record
            .size
            .or_else(|| {
                self.node_registry
                    .view_descriptor(&node_record.kind)
                    .and_then(|descriptor| descriptor.default_size)
            })
            .unwrap_or(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            });
        Some(CanvasRect {
            origin: node_record.pos,
            size,
        })
    }

    pub fn default_handle_bounds(&self, node: NodeId) -> Vec<(ConnectionHandleRef, HandleBounds)> {
        let Some(node_record) = self.store.graph().nodes().get(&node) else {
            return Vec::new();
        };

        let inputs: Vec<_> = node_record
            .ports
            .iter()
            .copied()
            .filter(|port| {
                self.store
                    .graph()
                    .ports()
                    .get(port)
                    .is_some_and(|record| record.dir == PortDirection::In)
            })
            .collect();
        let outputs: Vec<_> = node_record
            .ports
            .iter()
            .copied()
            .filter(|port| {
                self.store
                    .graph()
                    .ports()
                    .get(port)
                    .is_some_and(|record| record.dir == PortDirection::Out)
            })
            .collect();
        let size = self
            .node_rect(node)
            .map(|rect| rect.size)
            .unwrap_or(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            });

        let mut handles = Vec::with_capacity(inputs.len() + outputs.len());
        handles.extend(side_handle_bounds(node, &inputs, PortDirection::In, size));
        handles.extend(side_handle_bounds(node, &outputs, PortDirection::Out, size));
        handles
    }

    pub fn create_node(
        &mut self,
        kind: NodeKindKey,
        pos: CanvasPoint,
    ) -> Result<DispatchOutcome, String> {
        self.store
            .apply_create_node_from_schema(&self.node_registry, CreateNodeRequest::new(kind, pos))
            .map(|outcome| outcome.dispatch)
            .map_err(|err| err.to_string())
    }

    pub fn connect_handles(
        &mut self,
        from: ConnectionHandleRef,
        to: ConnectionHandleRef,
    ) -> Result<Option<DispatchOutcome>, String> {
        let connection = jellyflow::runtime::runtime::connection::ConnectionHandleConnection::from_start_and_target(from, to);
        let mode = self.store.resolved_interaction_state().connection_mode;
        self.store
            .apply_connect_edge(ConnectEdgeRequest::new(
                connection.source.port,
                connection.target.port,
                mode,
            ))
            .map_err(|err| err.to_string())
    }

    pub fn apply_layout(
        &mut self,
        choice: LayoutPresetChoice,
    ) -> Result<Option<DispatchOutcome>, LayoutApplyError> {
        let request = layout_request_for_choice(&self.store, choice);
        self.store
            .apply_layout(&request, self.layout_registry)
            .map(|outcome| outcome.dispatch)
    }

    pub fn apply_layout_request(
        &mut self,
        request: &LayoutEngineRequest,
    ) -> Result<Option<DispatchOutcome>, LayoutApplyError> {
        self.store
            .apply_layout(request, self.layout_registry)
            .map(|outcome| outcome.dispatch)
    }

    pub fn fit_view(&mut self, viewport: CanvasSize) -> bool {
        let nodes = self
            .store
            .graph()
            .nodes()
            .iter()
            .filter_map(|(_id, node)| {
                if node.hidden {
                    return None;
                }
                let size = node.size.or_else(|| {
                    self.node_registry
                        .view_descriptor(&node.kind)
                        .and_then(|descriptor| descriptor.default_size)
                })?;
                Some(FitViewNodeInfo {
                    pos: node.pos,
                    origin: node.origin,
                    size_px: (size.width, size.height),
                })
            })
            .collect::<Vec<_>>();
        let Some((pan, zoom)) = compute_fit_view_target(
            &nodes,
            FitViewComputeOptions {
                viewport_width_px: viewport.width,
                viewport_height_px: viewport.height,
                node_origin: (0.0, 0.0),
                padding: 0.12,
                margin_px_fallback: 48.0,
                min_zoom: 0.2,
                max_zoom: 2.5,
            },
        ) else {
            return false;
        };
        self.store.set_viewport(pan, zoom);
        true
    }

    pub fn pan_by_screen_delta(&mut self, delta: CanvasPoint) -> bool {
        self.store
            .apply_viewport_pan(ViewportPanRequest::new(delta))
            .is_some()
    }

    pub fn zoom_at_screen(&mut self, anchor_screen: CanvasPoint, factor: f32) -> bool {
        let current_zoom = self.store.view_state().zoom;
        if !factor.is_finite() || factor <= 0.0 || !current_zoom.is_finite() || current_zoom <= 0.0
        {
            return false;
        }
        self.store
            .apply_viewport_zoom(ViewportZoomRequest::new(
                anchor_screen,
                current_zoom * factor,
                0.2,
                3.0,
            ))
            .is_some()
    }

    pub fn select_node(&mut self, node: NodeId, additive: bool) {
        let mut nodes = if additive {
            self.store.view_state().selected_nodes.clone()
        } else {
            Vec::new()
        };
        if nodes.contains(&node) && additive {
            nodes.retain(|id| *id != node);
        } else if !nodes.contains(&node) {
            nodes.push(node);
        }
        nodes.sort();
        nodes.dedup();
        self.store.set_selection(nodes, Vec::new(), Vec::new());
    }

    pub fn select_edge(&mut self, edge: EdgeId, additive: bool) {
        let mut edges = if additive {
            self.store.view_state().selected_edges.clone()
        } else {
            Vec::new()
        };
        if edges.contains(&edge) && additive {
            edges.retain(|id| *id != edge);
        } else if !edges.contains(&edge) {
            edges.push(edge);
        }
        edges.sort();
        edges.dedup();
        self.store.set_selection(Vec::new(), edges, Vec::new());
    }

    pub fn clear_selection(&mut self) {
        self.store.set_selection(Vec::new(), Vec::new(), Vec::new());
    }

    pub fn start_node_drag(&mut self, node: NodeId, additive: bool) {
        self.store
            .apply_node_pointer_down(NodePointerDownInput::new(
                node,
                additive,
                CanvasPoint::default(),
            ));
    }

    pub fn apply_selection_box(
        &mut self,
        start: CanvasPoint,
        current: CanvasPoint,
        additive: bool,
    ) {
        let input = if additive {
            SelectionBoxInput::additive_from_drag(start, current)
        } else {
            SelectionBoxInput::replace_from_drag(start, current)
        };
        self.store.apply_selection_box(input);
    }

    pub fn commit_interaction(
        &mut self,
        interaction: ActiveCanvasInteraction,
    ) -> Result<Option<DispatchOutcome>, String> {
        match interaction {
            ActiveCanvasInteraction::NodeDrag { preview, .. } => {
                let Some(plan) = preview else {
                    return Ok(None);
                };
                self.store
                    .dispatch_transaction(plan.transaction())
                    .map(Some)
                    .map_err(|err| err.to_string())
            }
            ActiveCanvasInteraction::NodeResize {
                node,
                direction,
                start_pointer,
                current_pointer,
                preview,
            } => {
                let Some(_preview) = preview else {
                    return Ok(None);
                };
                self.store
                    .apply_node_resize_session(
                        NodeResizeSession::new(node, start_pointer, direction),
                        NodeResizeSessionUpdateRequest::new(current_pointer),
                    )
                    .map(|outcome| outcome.update.map(|update| update.dispatch))
                    .map_err(|err| err.to_string())
            }
            ActiveCanvasInteraction::Connect { from, target, .. } => {
                let Some(target) = target.and_then(|target| target.target) else {
                    return Ok(None);
                };
                self.connect_handles(from, target.handle)
            }
            ActiveCanvasInteraction::SelectionBox {
                start_pointer,
                current_pointer,
                additive,
            } => {
                self.apply_selection_box(start_pointer, current_pointer, additive);
                Ok(None)
            }
            ActiveCanvasInteraction::Pan { .. } => Ok(None),
            ActiveCanvasInteraction::None => Ok(None),
        }
    }

    pub fn plan_node_drag(
        &self,
        node: NodeId,
        pointer_delta: CanvasPoint,
    ) -> Option<jellyflow::runtime::runtime::drag::NodeDragPlan> {
        let node_record = self.store.graph().nodes().get(&node)?;
        self.store
            .plan_node_drag(jellyflow::runtime::runtime::drag::NodeDragRequest {
                node,
                to: CanvasPoint {
                    x: node_record.pos.x + pointer_delta.x,
                    y: node_record.pos.y + pointer_delta.y,
                },
            })
    }

    pub fn plan_pointer_resize(
        &self,
        node: NodeId,
        start: CanvasPoint,
        current: CanvasPoint,
        direction: NodeResizeDirection,
    ) -> Option<jellyflow::runtime::runtime::resize::NodeResizePlan> {
        self.store
            .plan_node_pointer_resize(NodePointerResizeRequest::new(
                node, start, current, direction,
            ))
    }

    pub fn resolve_connection_target(
        &self,
        pointer: CanvasPoint,
        from: ConnectionHandleRef,
    ) -> jellyflow::runtime::runtime::connection::ResolvedConnectionTarget {
        self.store
            .resolve_connection_target_from_layout_facts(pointer, from)
    }

    pub fn undo(&mut self) -> Result<Option<DispatchOutcome>, DispatchError> {
        self.store.undo()
    }

    pub fn redo(&mut self) -> Result<Option<DispatchOutcome>, DispatchError> {
        self.store.redo()
    }

    pub fn delete_selection(&mut self) -> Result<Option<DispatchOutcome>, String> {
        self.store
            .apply_delete_selection()
            .map_err(|err| err.to_string())
    }

    pub fn nudge_selection(
        &mut self,
        direction: NodeNudgeDirection,
        fast: bool,
    ) -> Result<Option<KeyboardActionOutcome>, KeyboardActionError> {
        self.store
            .apply_keyboard_intent(KeyboardIntent::NudgeSelection(NodeNudgeRequest {
                direction,
                fast,
            }))
    }

    fn report_snapshot_measurements(&mut self, snapshot: &CanvasSnapshot) {
        for (node, rect) in &snapshot.node_rects {
            let handles = self
                .default_handle_bounds(*node)
                .into_iter()
                .map(|(handle, bounds)| MeasuredHandle::new(handle, bounds));
            let _ = self.store.report_node_measurement(
                NodeMeasurement::new(*node)
                    .with_size(Some(rect.size))
                    .with_handles(handles),
            );
        }
    }

    fn populate_demo_graph(&mut self) -> Result<(), DemoGraphError> {
        let start = self
            .create_node(
                NodeKindKey::from("demo.start"),
                CanvasPoint { x: -360.0, y: 20.0 },
            )
            .map_err(DemoGraphError::Create)?
            .committed()
            .clone();
        let task = self
            .create_node(
                NodeKindKey::from("demo.task"),
                CanvasPoint { x: -80.0, y: -40.0 },
            )
            .map_err(DemoGraphError::Create)?
            .committed()
            .clone();
        let decision = self
            .create_node(
                NodeKindKey::from("demo.decision"),
                CanvasPoint { x: 220.0, y: -46.0 },
            )
            .map_err(DemoGraphError::Create)?
            .committed()
            .clone();
        let output = self
            .create_node(
                NodeKindKey::from("demo.output"),
                CanvasPoint { x: 520.0, y: 46.0 },
            )
            .map_err(DemoGraphError::Create)?
            .committed()
            .clone();

        let _ = (start, task, decision, output);
        self.connect_first("demo.start", "demo.task")
            .map_err(DemoGraphError::Connect)?;
        self.connect_first("demo.task", "demo.decision")
            .map_err(DemoGraphError::Connect)?;
        self.connect_first("demo.decision", "demo.output")
            .map_err(DemoGraphError::Connect)?;
        let _ = self.fit_view(CanvasSize {
            width: 1100.0,
            height: 700.0,
        });
        Ok(())
    }

    fn connect_first(&mut self, source_kind: &str, target_kind: &str) -> Result<(), String> {
        let source = first_port(self.store.graph(), source_kind, PortDirection::Out)
            .ok_or_else(|| format!("missing output port for `{source_kind}`"))?;
        let target = first_port(self.store.graph(), target_kind, PortDirection::In)
            .ok_or_else(|| format!("missing input port for `{target_kind}`"))?;
        let mode = self.store.resolved_interaction_state().connection_mode;
        self.store
            .apply_connect_edge(ConnectEdgeRequest::new(source, target, mode))
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

fn layout_request_for_choice(
    store: &NodeGraphStore,
    choice: LayoutPresetChoice,
) -> LayoutEngineRequest {
    choice
        .builder()
        .with_scope(layout_scope_for_selection(store))
        .build()
}

fn visible_or_full_nodes(visible: &[NodeId], full: &[NodeId]) -> Vec<NodeId> {
    if visible.is_empty() {
        full.to_vec()
    } else {
        visible.to_vec()
    }
}

fn visible_or_full_edges(visible: &[EdgeId], full: &[EdgeId]) -> Vec<EdgeId> {
    if visible.is_empty() {
        full.to_vec()
    } else {
        visible.to_vec()
    }
}

fn side_handle_bounds(
    node: NodeId,
    ports: &[PortId],
    direction: PortDirection,
    node_size: CanvasSize,
) -> Vec<(ConnectionHandleRef, HandleBounds)> {
    let count = ports.len().max(1) as f32;
    ports
        .iter()
        .enumerate()
        .map(|(index, port)| {
            let y =
                ((index + 1) as f32 / (count + 1.0)) * node_size.height - DEFAULT_HANDLE_SIZE * 0.5;
            let x = match direction {
                PortDirection::In => -DEFAULT_HANDLE_SIZE * 0.5,
                PortDirection::Out => node_size.width - DEFAULT_HANDLE_SIZE * 0.5,
            };
            (
                ConnectionHandleRef::new(node, *port, direction),
                HandleBounds {
                    rect: CanvasRect {
                        origin: CanvasPoint { x, y },
                        size: CanvasSize {
                            width: DEFAULT_HANDLE_SIZE,
                            height: DEFAULT_HANDLE_SIZE,
                        },
                    },
                    position: match direction {
                        PortDirection::In => HandlePosition::Left,
                        PortDirection::Out => HandlePosition::Right,
                    },
                },
            )
        })
        .collect()
}

fn first_port(graph: &Graph, kind: &str, direction: PortDirection) -> Option<PortId> {
    graph.nodes().iter().find_map(|(node_id, node)| {
        (node.kind.0 == kind).then(|| {
            node.ports.iter().copied().find(|port| {
                graph
                    .ports()
                    .get(port)
                    .is_some_and(|record| record.node == *node_id && record.dir == direction)
            })
        })?
    })
}

fn demo_node_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::new();
    registry.register(
        NodeSchema::builder("demo.start", "Start")
            .category(["Workflow"])
            .renderer_key("data-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(PortDecl::data_output("out").with_label("out"))
            .default_data(json!({ "summary": "Entry point" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.task", "Task")
            .category(["Workflow"])
            .renderer_key("task-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(PortDecl::data_input("in").with_label("in"))
            .port(PortDecl::data_output("out").with_label("out"))
            .default_data(json!({ "summary": "Run a unit of work" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.decision", "Decision")
            .category(["Workflow"])
            .renderer_key("decision-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(
                PortDecl::new(
                    "in",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                )
                .with_label("in"),
            )
            .port(
                PortDecl::new(
                    "yes",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                )
                .with_label("yes"),
            )
            .default_data(json!({ "summary": "Branch the flow" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.output", "Output")
            .category(["Workflow"])
            .renderer_key("output-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(PortDecl::data_input("in").with_label("in"))
            .default_data(json!({ "summary": "Publish the result" }))
            .build(),
    );
    registry
}
