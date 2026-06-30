use std::collections::{BTreeMap, HashMap};

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, DefaultTypeCompatibility, EdgeId, NodeId, NodeKindKey,
};
use jellyflow::layout::{LayoutEngineRegistry, builtin_layout_engine_registry};
use jellyflow::runtime::rules::plan_connect_typed_with_mode_and_policy;
use jellyflow::runtime::runtime::connection::{
    ConnectEdgeError, ConnectEdgeRequest, ConnectionHandleRef, ConnectionTargetHandle,
    ConnectionTargetInput, ResolvedConnectionTarget, resolve_connection_target,
};
use jellyflow::runtime::runtime::create_node::CreateNodeRequest;
use jellyflow::runtime::runtime::drag::{NodeNudgeDirection, NodeNudgeRequest};
use jellyflow::runtime::runtime::fit_view::{
    FitViewComputeOptions, FitViewNodeInfo, compute_fit_view_target,
};
use jellyflow::runtime::runtime::geometry::{HandleBounds, HandlePosition};
use jellyflow::runtime::runtime::keyboard::{
    KeyboardActionError, KeyboardActionOutcome, KeyboardIntent,
};
use jellyflow::runtime::runtime::layout::{LayoutApplyError, LayoutEngineRequest};
use jellyflow::runtime::runtime::measurement::{
    MeasuredHandle, MeasuredSurfaceAnchor, MeasuredSurfaceSlot, NodeInternalsInvalidation,
    NodeInternalsInvalidationReason, NodeMeasurement, NodeMeasurementOutcome,
    NodeMeasurementStatus,
};
use jellyflow::runtime::runtime::resize::{
    NodePointerResizeRequest, NodeResizeConstraints, NodeResizeDirection,
};
use jellyflow::runtime::runtime::selection::{NodePointerDownInput, SelectionBoxInput};
use jellyflow::runtime::runtime::viewport::{
    ViewportPanRequest, ViewportTransform, ViewportZoomRequest,
};
use jellyflow::runtime::schema::{
    NodeKindViewDescriptor, NodeRegistry, NodeSurfaceSlotVisibility, PortHandleVisibility,
    PortViewSide,
};
use jellyflow::runtime::{DispatchError, DispatchOutcome, NodeGraphStore};

use crate::handle_layout::{HandleAnchorRegion, HandleLayoutPort, handle_bounds_for_node};
use crate::renderer::{
    NodeInteractiveRegion, NodeRenderInput, NodeRenderLayout, NodeRendererState, RendererCatalog,
};
use crate::samples::{SampleGraphError, SampleGraphKind, sample_graph};
use crate::state::{
    ActiveCanvasInteraction, CanvasSnapshot, LayoutPresetChoice, layout_scope_for_selection,
};

pub(crate) const DEFAULT_NODE_WIDTH: f32 = 180.0;
pub(crate) const DEFAULT_NODE_HEIGHT: f32 = 86.0;
pub(crate) const MIN_NODE_WIDTH: f32 = 96.0;
pub(crate) const MIN_NODE_HEIGHT: f32 = 56.0;
pub(crate) const DEFAULT_HANDLE_SIZE: f32 = 10.0;

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

    pub fn sample(kind: SampleGraphKind) -> Result<(Self, LayoutPresetChoice), SampleGraphError> {
        let sample = sample_graph(kind)?;
        Ok((
            Self::new(
                sample.store,
                sample.registry,
                builtin_layout_engine_registry(),
                RendererCatalog::default(),
            ),
            sample.default_layout,
        ))
    }

    pub fn demo() -> Result<Self, SampleGraphError> {
        Self::sample(SampleGraphKind::Workflow).map(|(bridge, _layout)| bridge)
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
        _previous: &CanvasSnapshot,
        viewport_rect: eframe::egui::Rect,
    ) -> CanvasSnapshot {
        let viewport_size = CanvasSize {
            width: viewport_rect.width().max(1.0),
            height: viewport_rect.height().max(1.0),
        };
        let transform =
            ViewportTransform::from_view_state(self.store.view_state()).unwrap_or_else(|| {
                ViewportTransform::new(CanvasPoint::default(), 1.0)
                    .expect("default viewport transform is valid")
            });
        let initial_layout_facts = self.store.layout_facts_query(viewport_size);
        let (node_rects, node_render_layouts, handle_bounds) =
            self.snapshot_geometry(&initial_layout_facts.rendering);
        self.report_current_measurements(&node_rects, &node_render_layouts, &handle_bounds);
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

        let edge_paths = layout_facts
            .visible_edge_route_facts
            .iter()
            .map(|facts| (facts.edge, facts.facts.path.clone()))
            .collect();

        CanvasSnapshot {
            viewport_rect,
            viewport_size,
            transform,
            node_rects,
            node_render_layouts,
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
            .filter(|size| size.is_positive_finite())
            .or_else(|| {
                self.node_registry
                    .get(&node_record.kind)
                    .and_then(|schema| schema.default_size)
            })
            .unwrap_or(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            });
        Some(CanvasRect {
            origin: node_record.pos,
            size: minimum_render_size(size),
        })
    }

    pub fn default_handle_bounds(&self, node: NodeId) -> Vec<(ConnectionHandleRef, HandleBounds)> {
        self.handle_bounds_with_regions(node, &[])
    }

    pub fn node_render_layout(
        &self,
        node: NodeId,
        canvas_rect: CanvasRect,
        state: NodeRendererState,
    ) -> Option<NodeRenderLayout> {
        let node_record = self.store.graph().nodes().get(&node)?;
        let descriptor = self.node_registry.view_descriptor(&node_record.kind)?;
        let style = self.renderers.style_for_descriptor(&descriptor);
        Some(self.renderers.render_node(
            &NodeRenderInput {
                id: node,
                node: node_record,
                descriptor: &descriptor,
                state,
                style,
            },
            canvas_rect,
        ))
    }

    fn handle_bounds_with_regions(
        &self,
        node: NodeId,
        regions: &[NodeInteractiveRegion],
    ) -> Vec<(ConnectionHandleRef, HandleBounds)> {
        let size = self
            .node_rect(node)
            .map(|rect| rect.size)
            .unwrap_or(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            });
        self.handle_bounds_for_size(node, size, regions)
    }

    pub(crate) fn handle_bounds_for_size(
        &self,
        node: NodeId,
        size: CanvasSize,
        regions: &[NodeInteractiveRegion],
    ) -> Vec<(ConnectionHandleRef, HandleBounds)> {
        let Some(node_record) = self.store.graph().nodes().get(&node) else {
            return Vec::new();
        };
        let schema = self.node_registry.get(&node_record.kind);
        let ports = node_record.ports.iter().copied().filter_map(|port| {
            let port_record = self.store.graph().ports().get(&port)?;
            let decl = schema
                .and_then(|schema| schema.ports.iter().find(|decl| decl.key == port_record.key));
            Some(HandleLayoutPort {
                id: port,
                direction: port_record.dir,
                decl,
            })
        });
        let anchor_regions = regions
            .iter()
            .map(|region| HandleAnchorRegion {
                key: region.key.as_str(),
                rect: region.rect,
            })
            .collect::<Vec<_>>();

        handle_bounds_for_node(node, ports, size, &anchor_regions)
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
        let request = ConnectEdgeRequest::new(connection.source.port, connection.target.port, mode);
        let plan = self.plan_typed_connect(request);
        if plan.is_reject() {
            return Err(connect_plan_error_message(&plan));
        }
        self.store
            .apply_connect_edge(request)
            .map_err(|err| match err {
                ConnectEdgeError::Rejected { ref diagnostics } => diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.message.clone())
                    .unwrap_or_else(|| "connection rejected".to_owned()),
                ConnectEdgeError::Dispatch(err) => err.to_string(),
            })
    }

    pub fn invalidate_node_internals(
        &mut self,
        node: NodeId,
        reason: NodeInternalsInvalidationReason,
    ) -> NodeMeasurementOutcome {
        self.store.node_internals().invalidate_one(node, reason)
    }

    pub fn invalidate_visible_node_internals(
        &mut self,
        snapshot: &CanvasSnapshot,
        reason: NodeInternalsInvalidationReason,
    ) -> NodeMeasurementOutcome {
        self.store
            .node_internals()
            .invalidate(NodeInternalsInvalidation::new(
                snapshot.visible_node_ids.iter().copied(),
                reason,
            ))
    }

    pub fn notify_node_data_changed(&mut self, node: NodeId) -> NodeMeasurementOutcome {
        self.invalidate_node_internals(node, NodeInternalsInvalidationReason::DataChanged)
    }

    pub fn notify_node_component_state_changed(&mut self, node: NodeId) -> NodeMeasurementOutcome {
        self.invalidate_node_internals(node, NodeInternalsInvalidationReason::ComponentStateChanged)
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
                        .get(&node.kind)
                        .and_then(|schema| schema.default_size)
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
        let changed = self
            .store
            .apply_viewport_zoom(ViewportZoomRequest::new(
                anchor_screen,
                current_zoom * factor,
                0.2,
                3.0,
            ))
            .is_some();
        if changed {
            let nodes = self
                .store
                .graph()
                .nodes()
                .keys()
                .copied()
                .collect::<Vec<_>>();
            self.store
                .node_internals()
                .invalidate(NodeInternalsInvalidation::new(
                    nodes,
                    NodeInternalsInvalidationReason::ZoomChanged,
                ));
        }
        changed
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
            ActiveCanvasInteraction::NodeResize { preview, .. } => {
                let Some(plan) = preview else {
                    return Ok(None);
                };
                let node = plan.node;
                let outcome = self
                    .store
                    .dispatch_transaction(plan.transaction())
                    .map_err(|err| err.to_string())?;
                self.invalidate_node_internals(node, NodeInternalsInvalidationReason::SizeChanged);
                Ok(Some(outcome))
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
        self.store.plan_node_pointer_resize(
            NodePointerResizeRequest::new(node, start, current, direction)
                .with_constraints(self.resize_constraints_for_node(node)),
        )
    }

    pub fn resolve_connection_target(
        &self,
        pointer: CanvasPoint,
        from: ConnectionHandleRef,
    ) -> jellyflow::runtime::runtime::connection::ResolvedConnectionTarget {
        let base = self
            .store
            .resolve_connection_target_from_layout_facts(pointer, from);
        self.resolve_typed_connection_target(pointer, from, base)
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

    fn snapshot_geometry(
        &self,
        rendering: &jellyflow::runtime::runtime::rendering::RenderingQueryResult,
    ) -> (
        BTreeMap<NodeId, CanvasRect>,
        BTreeMap<NodeId, NodeRenderLayout>,
        HashMap<ConnectionHandleRef, HandleBounds>,
    ) {
        let visible_node_ids =
            visible_or_full_nodes(&rendering.visible_node_ids, &rendering.node_order);
        let mut node_rects = BTreeMap::new();
        for node in &visible_node_ids {
            if let Some(rect) = self.node_rect(*node) {
                node_rects.insert(*node, rect);
            }
        }

        let mut node_render_layouts = BTreeMap::new();
        for (node, rect) in &node_rects {
            if let Some(layout) =
                self.node_render_layout(*node, *rect, NodeRendererState::default())
            {
                node_render_layouts.insert(*node, layout);
            }
        }

        let mut handle_bounds = HashMap::new();
        for node in &visible_node_ids {
            let regions = node_render_layouts
                .get(node)
                .map(|layout| layout.interactive_regions.as_slice())
                .unwrap_or(&[]);
            for (handle, bounds) in self.handle_bounds_with_regions(*node, regions) {
                handle_bounds.insert(handle, bounds);
            }
        }

        (node_rects, node_render_layouts, handle_bounds)
    }

    fn report_current_measurements(
        &mut self,
        node_rects: &BTreeMap<NodeId, CanvasRect>,
        node_render_layouts: &BTreeMap<NodeId, NodeRenderLayout>,
        handle_bounds: &HashMap<ConnectionHandleRef, HandleBounds>,
    ) {
        for (node, rect) in node_rects {
            let descriptor = self.descriptor_for_node(*node);
            let regions = node_render_layouts
                .get(node)
                .map(|layout| layout.interactive_regions.as_slice())
                .unwrap_or(&[]);
            let handles = handle_bounds.iter().filter_map(|(handle, bounds)| {
                (handle.node == *node).then_some(MeasuredHandle::new(*handle, *bounds))
            });
            let slots = measured_surface_slots(regions);
            let anchors = descriptor
                .as_ref()
                .map(|descriptor| measured_surface_anchors(descriptor, regions))
                .unwrap_or_default();
            let _ = self.store.report_node_measurement(
                NodeMeasurement::new(*node)
                    .with_revision(self.next_measurement_revision(*node))
                    .with_size(Some(rect.size))
                    .with_handles(handles)
                    .with_slots(slots)
                    .with_anchors(anchors),
            );
        }
    }

    fn next_measurement_revision(&self, node: NodeId) -> u64 {
        match self.store.node_measurement_status(node) {
            NodeMeasurementStatus::Fresh { revision }
            | NodeMeasurementStatus::Dirty { revision, .. } => revision.saturating_add(1),
            NodeMeasurementStatus::Missing => 1,
        }
    }

    fn resolve_typed_connection_target(
        &self,
        _pointer: CanvasPoint,
        from: ConnectionHandleRef,
        base: ResolvedConnectionTarget,
    ) -> ResolvedConnectionTarget {
        let Some(target) = base.target else {
            return base;
        };
        let connection =
            jellyflow::runtime::runtime::connection::ConnectionHandleConnection::from_start_and_target(
                from,
                target.handle,
            );
        let request = ConnectEdgeRequest::new(
            connection.source.port,
            connection.target.port,
            self.store.resolved_interaction_state().connection_mode,
        );
        let is_valid_connection = self.plan_typed_connect(request).is_accept();
        resolve_connection_target(
            ConnectionTargetInput::new(
                from,
                Some(ConnectionTargetHandle::new(
                    target.handle,
                    target.connectable,
                    target.connectable_end,
                )),
                self.store.resolved_interaction_state().connection_mode,
                true,
            )
            .with_connection_validity(is_valid_connection),
        )
    }

    fn plan_typed_connect(
        &self,
        request: ConnectEdgeRequest,
    ) -> jellyflow::runtime::rules::ConnectPlan {
        let mut compat = DefaultTypeCompatibility::default();
        let interaction = self.store.resolved_interaction_state();
        plan_connect_typed_with_mode_and_policy(
            self.store.graph(),
            request.from,
            request.to,
            request.mode,
            &interaction,
            |graph, port| graph.ports().get(&port).and_then(|port| port.ty.clone()),
            &mut compat,
        )
    }

    pub(crate) fn resize_constraints_for_node(&self, node: NodeId) -> NodeResizeConstraints {
        NodeResizeConstraints::new(Some(self.minimum_node_size(node)), None)
    }

    fn minimum_node_size(&self, node: NodeId) -> CanvasSize {
        let fallback = CanvasSize {
            width: MIN_NODE_WIDTH,
            height: MIN_NODE_HEIGHT,
        };
        let Some(rect) = self.node_rect(node) else {
            return fallback;
        };
        self.node_render_layout(node, rect, NodeRendererState::default())
            .map(|layout| minimum_render_size(layout.min_size))
            .unwrap_or(fallback)
    }
}

fn measured_surface_slots(
    regions: &[NodeInteractiveRegion],
) -> impl Iterator<Item = MeasuredSurfaceSlot> + '_ {
    regions
        .iter()
        .filter(|region| region.rect.is_positive_finite())
        .map(|region| MeasuredSurfaceSlot::new(region.key.clone(), region.rect))
}

fn measured_surface_anchors(
    descriptor: &NodeKindViewDescriptor,
    regions: &[NodeInteractiveRegion],
) -> Vec<MeasuredSurfaceAnchor> {
    regions
        .iter()
        .filter(|region| region.rect.is_positive_finite())
        .flat_map(|region| {
            descriptor
                .ports_by_anchor(&region.key)
                .into_iter()
                .map(move |port| {
                    MeasuredSurfaceAnchor::new(
                        region.key.clone(),
                        region.rect,
                        port_side_to_handle_position(port.view.resolved_side(port.dir)),
                    )
                    .with_port_key(port.key.clone())
                    .with_visibility(
                        port.view
                            .visibility
                            .map(port_visibility_to_slot_visibility)
                            .unwrap_or(NodeSurfaceSlotVisibility::Visible),
                    )
                })
        })
        .collect()
}

fn port_visibility_to_slot_visibility(
    visibility: PortHandleVisibility,
) -> NodeSurfaceSlotVisibility {
    match visibility {
        PortHandleVisibility::Visible => NodeSurfaceSlotVisibility::Visible,
        PortHandleVisibility::Hidden => NodeSurfaceSlotVisibility::Hidden,
        PortHandleVisibility::Collapsed => NodeSurfaceSlotVisibility::Collapsed,
    }
}

fn port_side_to_handle_position(side: PortViewSide) -> HandlePosition {
    match side {
        PortViewSide::Top => HandlePosition::Top,
        PortViewSide::Right => HandlePosition::Right,
        PortViewSide::Bottom => HandlePosition::Bottom,
        PortViewSide::Left => HandlePosition::Left,
    }
}

fn connect_plan_error_message(plan: &jellyflow::runtime::rules::ConnectPlan) -> String {
    plan.diagnostics()
        .first()
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| "connect edge was rejected".to_owned())
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

fn minimum_render_size(size: CanvasSize) -> CanvasSize {
    if !size.is_finite() {
        return CanvasSize {
            width: MIN_NODE_WIDTH,
            height: MIN_NODE_HEIGHT,
        };
    }
    CanvasSize {
        width: size.width.max(MIN_NODE_WIDTH),
        height: size.height.max(MIN_NODE_HEIGHT),
    }
}
