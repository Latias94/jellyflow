use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, DefaultTypeCompatibility, EdgeId, Graph, GraphOp,
    GraphTransaction, NodeId, NodeKindKey, PortId,
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
    ActionTarget, MenuDescriptor, MenuSurface, NodeActionDescriptor, NodeControlBinding,
    NodeControlBindingSource, NodeControlDescriptor, NodeControlKind, NodeKindViewDescriptor,
    NodeRegistry, NodeRepeatableCollectionDescriptor, NodeSurfaceSlotVisibility,
    PortHandleVisibility, PortViewSide,
};
use jellyflow::runtime::{DispatchError, DispatchOutcome, NodeGraphStore};

use crate::handle_layout::{HandleAnchorRegion, HandleLayoutPort, handle_bounds_for_node};
use crate::renderer::{
    EguiNodeControlEdit, EguiNodeRepeatableAction, EguiNodeWidgetRenderOutcome,
    EguiRepeatableMoveDirection, NodeInteractiveRegion, NodeRenderInput, NodeRenderLayout,
    NodeRendererState, RendererCatalog,
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

    pub fn action_descriptor(&self, action_key: &str) -> Option<NodeActionDescriptor> {
        self.node_registry
            .view_descriptors()
            .into_iter()
            .flat_map(|descriptor| descriptor.actions)
            .find(|action| action.key == action_key)
    }

    pub fn dropped_wire_menu_for_handle(
        &self,
        from: ConnectionHandleRef,
    ) -> Option<MenuDescriptor> {
        let source_port_key = self
            .store
            .graph()
            .ports()
            .get(&from.port)
            .map(|port| port.key.0.as_str());
        let mut action_keys = Vec::new();
        for descriptor in self.node_registry.view_descriptors() {
            for menu in descriptor
                .menus
                .iter()
                .filter(|menu| menu.surface == MenuSurface::DroppedWire)
            {
                for action_key in &menu.action_keys {
                    let Some(action) = descriptor.action(action_key) else {
                        continue;
                    };
                    if dropped_wire_action_matches_source(action, source_port_key)
                        && !action_keys.iter().any(|existing| existing == action_key)
                    {
                        action_keys.push(action_key.clone());
                    }
                }
            }
        }

        (!action_keys.is_empty()).then(|| {
            MenuDescriptor::new("menu.dropped_wire", MenuSurface::DroppedWire)
                .with_label("Insert node")
                .with_action_keys(action_keys)
        })
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

    pub(crate) fn measured_node_rect_for(
        &self,
        node: NodeId,
        rect: CanvasRect,
        state: NodeRendererState,
    ) -> CanvasRect {
        self.measured_node_rect_and_layout_for(node, rect, state)
            .map(|(rect, _layout)| rect)
            .unwrap_or(rect)
    }

    fn handle_bounds_with_regions(
        &self,
        node: NodeId,
        regions: &[NodeInteractiveRegion],
    ) -> Vec<(ConnectionHandleRef, HandleBounds)> {
        let rect = self.node_rect(node).unwrap_or(CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            },
        });
        self.handle_bounds_for_rect(node, rect, regions)
    }

    pub(crate) fn handle_bounds_for_size(
        &self,
        node: NodeId,
        size: CanvasSize,
        regions: &[NodeInteractiveRegion],
    ) -> Vec<(ConnectionHandleRef, HandleBounds)> {
        self.handle_bounds_for_rect(
            node,
            CanvasRect {
                origin: CanvasPoint::default(),
                size,
            },
            regions,
        )
    }

    fn handle_bounds_for_rect(
        &self,
        node: NodeId,
        rect: CanvasRect,
        regions: &[NodeInteractiveRegion],
    ) -> Vec<(ConnectionHandleRef, HandleBounds)> {
        let Some(node_record) = self.store.graph().nodes().get(&node) else {
            return Vec::new();
        };
        let schema = self.node_registry.get(&node_record.kind);
        let descriptor = self.node_registry.view_descriptor(&node_record.kind);
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
        let anchor_regions = handle_anchor_regions(
            regions,
            descriptor.as_ref(),
            Some(&node_record.data),
            Some(&node_record.ports),
            self.store.graph(),
        );

        handle_bounds_for_node(node, ports, rect.size, &anchor_regions)
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

    pub fn apply_node_widget_outcome(
        &mut self,
        node: NodeId,
        outcome: EguiNodeWidgetRenderOutcome,
    ) -> Result<bool, String> {
        let mut data_changed = false;
        for edit in outcome.control_edits {
            data_changed |= self.apply_node_control_edit(node, edit)?.is_some();
        }
        for action in outcome.repeatable_actions {
            data_changed |= self.apply_node_repeatable_action(node, action)?.is_some();
        }
        if !data_changed && outcome.changed {
            self.notify_node_component_state_changed(node);
        }
        Ok(data_changed)
    }

    pub fn apply_node_control_edit(
        &mut self,
        node: NodeId,
        edit: EguiNodeControlEdit,
    ) -> Result<Option<DispatchOutcome>, String> {
        let from = self
            .store
            .graph()
            .nodes()
            .get(&node)
            .map(|node| node.data.clone())
            .ok_or_else(|| format!("missing node `{node:?}`"))?;
        let mut to = from.clone();
        set_bound_node_value(&mut to, &edit.binding, edit.value)?;
        let outcome =
            self.dispatch_node_data_if_changed(node, from, to, "Set node control value")?;
        if outcome.is_some() {
            self.notify_node_data_changed(node);
        }
        Ok(outcome)
    }

    pub fn apply_node_repeatable_action(
        &mut self,
        node: NodeId,
        action: EguiNodeRepeatableAction,
    ) -> Result<Option<DispatchOutcome>, String> {
        let collection_key = match &action {
            EguiNodeRepeatableAction::Add { collection_key }
            | EguiNodeRepeatableAction::Remove { collection_key, .. }
            | EguiNodeRepeatableAction::Move { collection_key, .. } => collection_key,
        };
        let descriptor = self
            .descriptor_for_node(node)
            .ok_or_else(|| format!("missing descriptor for node `{node:?}`"))?;
        let collection = descriptor
            .repeatable_collection(collection_key)
            .cloned()
            .ok_or_else(|| format!("missing repeatable collection `{collection_key}`"))?;
        let from = self
            .store
            .graph()
            .nodes()
            .get(&node)
            .map(|node| node.data.clone())
            .ok_or_else(|| format!("missing node `{node:?}`"))?;
        let mut to = from.clone();
        apply_repeatable_action_to_data(&mut to, &collection, action)?;
        let outcome =
            self.dispatch_node_data_if_changed(node, from, to, "Set repeatable collection")?;
        if outcome.is_some() {
            self.notify_node_data_changed(node);
        }
        Ok(outcome)
    }

    fn dispatch_node_data_if_changed(
        &mut self,
        node: NodeId,
        from: serde_json::Value,
        to: serde_json::Value,
        label: &str,
    ) -> Result<Option<DispatchOutcome>, String> {
        if from == to {
            return Ok(None);
        }
        self.store
            .dispatch_transaction(
                &GraphTransaction::from_ops([GraphOp::SetNodeData { id: node, from, to }])
                    .with_label(label),
            )
            .map(Some)
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
        let mut node_render_layouts = BTreeMap::new();
        for node in &visible_node_ids {
            if let Some((rect, layout)) = self.measured_node_rect_and_layout(*node) {
                node_rects.insert(*node, rect);
                node_render_layouts.insert(*node, layout);
            }
        }

        let mut handle_bounds = HashMap::new();
        for node in &visible_node_ids {
            let regions = node_render_layouts
                .get(node)
                .map(|layout| layout.interactive_regions.as_slice())
                .unwrap_or(&[]);
            let Some(rect) = node_rects.get(node).copied() else {
                continue;
            };
            for (handle, bounds) in self.handle_bounds_for_rect(*node, rect, regions) {
                handle_bounds.insert(handle, bounds);
            }
        }

        (node_rects, node_render_layouts, handle_bounds)
    }

    fn measured_node_rect_and_layout(
        &self,
        node: NodeId,
    ) -> Option<(CanvasRect, NodeRenderLayout)> {
        let rect = self.node_rect(node)?;
        self.measured_node_rect_and_layout_for(node, rect, NodeRendererState::default())
    }

    fn measured_node_rect_and_layout_for(
        &self,
        node: NodeId,
        mut rect: CanvasRect,
        state: NodeRendererState,
    ) -> Option<(CanvasRect, NodeRenderLayout)> {
        for _ in 0..3 {
            let layout = self.node_render_layout(node, rect, state)?;
            let min_size = minimum_render_size(layout.min_size);
            let next_size = CanvasSize {
                width: rect.size.width.max(min_size.width),
                height: rect.size.height.max(min_size.height),
            };
            if sizes_close(rect.size, next_size) {
                return Some((rect, layout));
            }
            rect.size = next_size;
        }
        let layout = self.node_render_layout(node, rect, state)?;
        Some((rect, layout))
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
                .and_then(|descriptor| {
                    self.store.graph().nodes().get(node).map(|node_record| {
                        measured_surface_anchors(
                            descriptor,
                            regions,
                            &node_record.data,
                            &node_record.ports,
                            self.store.graph(),
                        )
                    })
                })
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
    node_data: &serde_json::Value,
    node_ports: &[PortId],
    graph: &Graph,
) -> Vec<MeasuredSurfaceAnchor> {
    surface_anchor_rects(regions, descriptor, node_data)
        .into_iter()
        .flat_map(|(anchor, rect, port_key)| {
            let ports = if let Some(port_key) = port_key.as_deref() {
                descriptor
                    .port_decl(port_key)
                    .into_iter()
                    .filter(|_| node_has_port_key(node_ports, graph, port_key))
                    .collect::<Vec<_>>()
            } else {
                descriptor
                    .ports_by_anchor(&anchor)
                    .into_iter()
                    .collect::<Vec<_>>()
            };
            ports.into_iter().map(move |port| {
                MeasuredSurfaceAnchor::new(
                    anchor.clone(),
                    rect,
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

fn handle_anchor_regions<'a>(
    regions: &'a [NodeInteractiveRegion],
    descriptor: Option<&'a NodeKindViewDescriptor>,
    node_data: Option<&'a serde_json::Value>,
    node_ports: Option<&'a [PortId]>,
    graph: &'a Graph,
) -> Vec<HandleAnchorRegion<'a>> {
    let mut anchors = regions
        .iter()
        .filter(|region| region.rect.is_positive_finite())
        .map(|region| HandleAnchorRegion {
            key: Cow::Borrowed(region.key.as_str()),
            port_key: None,
            rect: region.rect,
        })
        .collect::<Vec<_>>();

    if let Some(descriptor) = descriptor {
        anchors.extend(regions.iter().filter_map(|region| {
            let slot = descriptor.surface_slot(&region.key)?;
            let anchor = slot.anchor.as_deref()?;
            (anchor != region.key && region.rect.is_positive_finite()).then_some(
                HandleAnchorRegion {
                    key: Cow::Borrowed(anchor),
                    port_key: None,
                    rect: region.rect,
                },
            )
        }));
        if let (Some(node_data), Some(node_ports)) = (node_data, node_ports) {
            anchors.extend(repeatable_item_anchor_regions(
                regions, descriptor, node_data, node_ports, graph,
            ));
        }
    }

    anchors
}

fn surface_anchor_rects(
    regions: &[NodeInteractiveRegion],
    descriptor: &NodeKindViewDescriptor,
    node_data: &serde_json::Value,
) -> Vec<(String, CanvasRect, Option<String>)> {
    let mut anchors = Vec::new();
    for region in regions
        .iter()
        .filter(|region| region.rect.is_positive_finite())
    {
        anchors.push((region.key.clone(), region.rect, None));
        if let Some(anchor) = descriptor
            .surface_slot(&region.key)
            .and_then(|slot| slot.anchor.as_ref())
            && anchor != &region.key
        {
            anchors.push((anchor.clone(), region.rect, None));
        }
    }
    anchors.extend(
        repeatable_item_port_anchors(regions, descriptor, node_data)
            .into_iter()
            .map(|(anchor, rect, port_key)| (anchor, rect, Some(port_key))),
    );
    anchors
}

fn repeatable_item_anchor_regions<'a>(
    regions: &'a [NodeInteractiveRegion],
    descriptor: &'a NodeKindViewDescriptor,
    node_data: &'a serde_json::Value,
    node_ports: &'a [PortId],
    graph: &'a Graph,
) -> Vec<HandleAnchorRegion<'a>> {
    repeatable_item_port_anchors(regions, descriptor, node_data)
        .into_iter()
        .filter(move |(_, _, port_key)| node_has_port_key(node_ports, graph, port_key))
        .map(|(anchor, rect, port_key)| HandleAnchorRegion {
            key: Cow::Owned(anchor),
            port_key: Some(Cow::Owned(port_key)),
            rect,
        })
        .collect()
}

fn repeatable_item_port_anchors(
    regions: &[NodeInteractiveRegion],
    descriptor: &NodeKindViewDescriptor,
    node_data: &serde_json::Value,
) -> Vec<(String, CanvasRect, String)> {
    let mut anchors = Vec::new();
    for collection in &descriptor.repeatable_collections {
        for item in collection.item_projections(node_data) {
            let Some(port_key) = item.port_key else {
                continue;
            };
            for slot in item.slots {
                let Some(region) = regions
                    .iter()
                    .find(|region| region.key == slot.key && region.rect.is_positive_finite())
                else {
                    continue;
                };
                let anchor = slot.anchor.unwrap_or(slot.key);
                anchors.push((
                    collection.anchor_rule.anchor_prefix.clone(),
                    region.rect,
                    port_key.clone(),
                ));
                anchors.push((item.anchor.clone(), region.rect, port_key.clone()));
                anchors.push((anchor, region.rect, port_key.clone()));
                anchors.push((port_key.clone(), region.rect, port_key.clone()));
                break;
            }
        }
    }
    anchors
}

fn node_has_port_key(node_ports: &[PortId], graph: &Graph, key: &str) -> bool {
    node_ports
        .iter()
        .filter_map(|port| graph.ports().get(port))
        .any(|port| port.key.0 == key)
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

fn sizes_close(a: CanvasSize, b: CanvasSize) -> bool {
    (a.width - b.width).abs() <= 0.5 && (a.height - b.height).abs() <= 0.5
}

fn set_bound_node_value(
    data: &mut serde_json::Value,
    binding: &NodeControlBinding,
    value: serde_json::Value,
) -> Result<(), String> {
    match binding.source {
        NodeControlBindingSource::DataPath | NodeControlBindingSource::Slot => {
            set_dot_path_value(data, &binding.path, value)
        }
        NodeControlBindingSource::JsonPointer => set_json_pointer_value(data, &binding.path, value),
        NodeControlBindingSource::GraphSymbol | NodeControlBindingSource::PortAnchor => {
            Err(format!(
                "binding source `{:?}` is not writable by the egui node adapter",
                binding.source
            ))
        }
    }
}

fn set_dot_path_value(
    value: &mut serde_json::Value,
    path: &str,
    new_value: serde_json::Value,
) -> Result<(), String> {
    let segments = path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        *value = new_value;
        return Ok(());
    }
    set_path_segments(value, &segments, new_value)
}

fn set_json_pointer_value(
    value: &mut serde_json::Value,
    pointer: &str,
    new_value: serde_json::Value,
) -> Result<(), String> {
    if pointer.is_empty() {
        *value = new_value;
        return Ok(());
    }
    let Some(pointer) = pointer.strip_prefix('/') else {
        return Err(format!("json pointer `{pointer}` must start with `/`"));
    };
    let segments = pointer
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let borrowed = segments.iter().map(String::as_str).collect::<Vec<_>>();
    set_path_segments(value, &borrowed, new_value)
}

fn set_path_segments(
    value: &mut serde_json::Value,
    segments: &[&str],
    new_value: serde_json::Value,
) -> Result<(), String> {
    let Some((segment, rest)) = segments.split_first() else {
        *value = new_value;
        return Ok(());
    };

    if rest.is_empty() {
        match value {
            serde_json::Value::Object(map) => {
                map.insert((*segment).to_owned(), new_value);
                Ok(())
            }
            serde_json::Value::Array(items) => {
                let index = segment
                    .parse::<usize>()
                    .map_err(|_| format!("array path segment `{segment}` is not an index"))?;
                let Some(slot) = items.get_mut(index) else {
                    return Err(format!("array index `{index}` is out of bounds"));
                };
                *slot = new_value;
                Ok(())
            }
            serde_json::Value::Null => {
                let mut map = serde_json::Map::new();
                map.insert((*segment).to_owned(), new_value);
                *value = serde_json::Value::Object(map);
                Ok(())
            }
            _ => Err(format!(
                "cannot set path segment `{segment}` on scalar value"
            )),
        }
    } else {
        match value {
            serde_json::Value::Object(map) => {
                let child = map
                    .entry((*segment).to_owned())
                    .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                set_path_segments(child, rest, new_value)
            }
            serde_json::Value::Array(items) => {
                let index = segment
                    .parse::<usize>()
                    .map_err(|_| format!("array path segment `{segment}` is not an index"))?;
                let Some(child) = items.get_mut(index) else {
                    return Err(format!("array index `{index}` is out of bounds"));
                };
                set_path_segments(child, rest, new_value)
            }
            serde_json::Value::Null => {
                *value = serde_json::Value::Object(serde_json::Map::new());
                set_path_segments(value, segments, new_value)
            }
            _ => Err(format!(
                "cannot traverse path segment `{segment}` on scalar value"
            )),
        }
    }
}

fn apply_repeatable_action_to_data(
    data: &mut serde_json::Value,
    collection: &NodeRepeatableCollectionDescriptor,
    action: EguiNodeRepeatableAction,
) -> Result<(), String> {
    match action {
        EguiNodeRepeatableAction::Add { .. } => add_repeatable_item(data, collection),
        EguiNodeRepeatableAction::Remove { item_id, .. } => {
            remove_repeatable_item(data, collection, &item_id)
        }
        EguiNodeRepeatableAction::Move {
            item_id, direction, ..
        } => move_repeatable_item(data, collection, &item_id, direction),
    }
}

fn add_repeatable_item(
    data: &mut serde_json::Value,
    collection: &NodeRepeatableCollectionDescriptor,
) -> Result<(), String> {
    if collection.add_disabled_reason(data).is_some() {
        return Ok(());
    }
    let next_id = next_repeatable_item_id(data, collection);
    let mut item = serde_json::json!({});
    set_dot_path_value(
        &mut item,
        &collection.item_id_path,
        serde_json::json!(next_id),
    )?;
    for slot in &collection.item_template_slots {
        for control in &slot.controls {
            if let Some(path) = repeatable_item_control_data_path(control) {
                set_dot_path_value(&mut item, &path, default_control_value(control))?;
            }
        }
    }

    let target = repeatable_collection_value_mut(data, &collection.item_source)?;
    match target {
        serde_json::Value::Array(items) => {
            items.push(item);
            Ok(())
        }
        serde_json::Value::Object(items) => {
            let key = item
                .pointer(&format!("/{}", collection.item_id_path.replace('.', "/")))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("item")
                .to_owned();
            items.insert(key, item);
            Ok(())
        }
        serde_json::Value::Null => {
            *target = serde_json::Value::Array(vec![item]);
            Ok(())
        }
        _ => Err(format!(
            "repeatable collection `{}` is not an array or object",
            collection.item_source
        )),
    }
}

fn remove_repeatable_item(
    data: &mut serde_json::Value,
    collection: &NodeRepeatableCollectionDescriptor,
    item_id: &str,
) -> Result<(), String> {
    if collection.remove_disabled_reason(data).is_some() {
        return Ok(());
    }
    let target = repeatable_collection_value_mut(data, &collection.item_source)?;
    match target {
        serde_json::Value::Array(items) => {
            items.retain(|item| repeatable_item_id(collection, item).as_deref() != Some(item_id));
            Ok(())
        }
        serde_json::Value::Object(items) => {
            items.retain(|key, item| {
                key != item_id && repeatable_item_id(collection, item).as_deref() != Some(item_id)
            });
            Ok(())
        }
        serde_json::Value::Null => Ok(()),
        _ => Err(format!(
            "repeatable collection `{}` is not an array or object",
            collection.item_source
        )),
    }
}

fn move_repeatable_item(
    data: &mut serde_json::Value,
    collection: &NodeRepeatableCollectionDescriptor,
    item_id: &str,
    direction: EguiRepeatableMoveDirection,
) -> Result<(), String> {
    if !collection.reorderable {
        return Ok(());
    }
    let target = repeatable_collection_value_mut(data, &collection.item_source)?;
    let serde_json::Value::Array(items) = target else {
        return Ok(());
    };
    let Some(index) = items
        .iter()
        .position(|item| repeatable_item_id(collection, item).as_deref() == Some(item_id))
    else {
        return Ok(());
    };
    match direction {
        EguiRepeatableMoveDirection::Up if index > 0 => items.swap(index, index - 1),
        EguiRepeatableMoveDirection::Down if index + 1 < items.len() => {
            items.swap(index, index + 1);
        }
        _ => {}
    }
    Ok(())
}

fn repeatable_collection_value_mut<'a>(
    data: &'a mut serde_json::Value,
    path: &str,
) -> Result<&'a mut serde_json::Value, String> {
    let segments = path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return Ok(data);
    }
    let mut current = data;
    for segment in segments {
        match current {
            serde_json::Value::Object(map) => {
                current = map
                    .entry(segment.to_owned())
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            }
            serde_json::Value::Null => {
                *current = serde_json::Value::Object(serde_json::Map::new());
                if let serde_json::Value::Object(map) = current {
                    current = map
                        .entry(segment.to_owned())
                        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                }
            }
            _ => {
                return Err(format!(
                    "cannot traverse repeatable path segment `{segment}`"
                ));
            }
        }
    }
    Ok(current)
}

fn repeatable_item_id(
    collection: &NodeRepeatableCollectionDescriptor,
    item: &serde_json::Value,
) -> Option<String> {
    semantic_json_lookup_bridge(item, &collection.item_id_path).and_then(|value| {
        value
            .as_str()
            .map(ToOwned::to_owned)
            .or_else(|| value.as_i64().map(|value| value.to_string()))
            .or_else(|| value.as_u64().map(|value| value.to_string()))
    })
}

fn next_repeatable_item_id(
    data: &serde_json::Value,
    collection: &NodeRepeatableCollectionDescriptor,
) -> String {
    let base = collection
        .key
        .rsplit('.')
        .next()
        .unwrap_or("item")
        .trim_end_matches('s')
        .replace('-', "_");
    let existing = collection
        .item_projections(data)
        .into_iter()
        .map(|item| item.item_id)
        .collect::<std::collections::BTreeSet<_>>();
    for index in 1..=999 {
        let candidate = format!("{base}_{index}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    format!("{base}_new")
}

fn repeatable_item_control_data_path(control: &NodeControlDescriptor) -> Option<String> {
    control
        .binding
        .as_ref()
        .and_then(|binding| match binding.source {
            NodeControlBindingSource::DataPath | NodeControlBindingSource::Slot => {
                Some(binding.path.clone())
            }
            NodeControlBindingSource::JsonPointer
            | NodeControlBindingSource::GraphSymbol
            | NodeControlBindingSource::PortAnchor => None,
        })
        .or_else(|| control.slot.clone())
}

fn default_control_value(control: &NodeControlDescriptor) -> serde_json::Value {
    match control.kind {
        NodeControlKind::NumberInput | NodeControlKind::Slider => serde_json::json!(0.0),
        NodeControlKind::Toggle => serde_json::json!(false),
        NodeControlKind::Select
        | NodeControlKind::Asset
        | NodeControlKind::VariablePicker
        | NodeControlKind::PortBinding => control
            .options
            .first()
            .map(|option| option.value.clone())
            .unwrap_or(serde_json::Value::Null),
        NodeControlKind::MultiSelect => serde_json::json!([]),
        NodeControlKind::TextInput
        | NodeControlKind::Code
        | NodeControlKind::Color
        | NodeControlKind::Expression
        | NodeControlKind::TextArea => serde_json::json!(""),
    }
}

fn semantic_json_lookup_bridge<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = current.get(segment)?;
    }
    Some(current)
}

fn dropped_wire_action_matches_source(
    action: &NodeActionDescriptor,
    source_port_key: Option<&str>,
) -> bool {
    match &action.target {
        ActionTarget::DroppedWire {
            source_port_key: None,
        } => true,
        ActionTarget::DroppedWire {
            source_port_key: Some(expected),
        } => source_port_key == Some(expected.as_str()),
        _ => false,
    }
}
