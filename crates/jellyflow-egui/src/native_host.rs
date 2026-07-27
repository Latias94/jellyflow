use std::any::TypeId;
use std::cell::RefCell;
use std::collections::{BTreeMap, btree_map::Entry};
use std::hash::Hash;
use std::rc::Rc;

use egui::{Context, Id, Pos2, Rect, Response, Ui, UiBuilder, Vec2};
use jellyflow_core::{NodeId, NodeKindKey, PortDirection, PortId};
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint, ViewRect, ViewSize, side_anchor};
use jellyflow_runtime::graph::{
    DocumentEpoch, GraphRevision, GraphSnapshot, GraphVersion, RuntimePatch, RuntimePatchKind,
};
use jellyflow_runtime::view::{
    AdapterFrame, FeedbackCollector, FeedbackCollectorError, FeedbackReceipt,
    FeedbackSubmissionError, FrameMaterializationError, FrameNode, FrameRequest, NodeInstanceKey,
    NodeLayout, NodeLayoutBuilderError, ViewCloseOutcome, ViewSession, ViewSessionError,
    ViewSessionEvent, ViewSubscriptionToken,
};

/// Egui-native frame bridge backed by one headless view capability.
///
/// For each node instance, the host retains only typed state-removal tokens needed for hard
/// retirement. Native widgets, render results, layout, and graph facts remain immediate and
/// frame-local.
pub struct EguiNativeFrameHost {
    view: ViewSession,
    lifecycle: Rc<RefCell<EguiNativeLifecycle>>,
    terminal_subscription: Option<ViewSubscriptionToken>,
}

impl EguiNativeFrameHost {
    #[must_use]
    pub fn new(view: ViewSession) -> Self {
        let initial_snapshot = view.graph_snapshot().ok();
        let lifecycle = Rc::new(RefCell::new(EguiNativeLifecycle {
            document_epoch: initial_snapshot.as_ref().map(GraphSnapshot::document_epoch),
            observed_revision: initial_snapshot.as_ref().map(GraphSnapshot::revision),
            ..EguiNativeLifecycle::default()
        }));
        let weak_lifecycle = Rc::downgrade(&lifecycle);
        let terminal_subscription = view
            .subscribe(move |event| match event {
                ViewSessionEvent::RuntimePatched { patch } => {
                    if let Some(lifecycle) = weak_lifecycle.upgrade() {
                        reconcile_lifecycle_patch(&lifecycle, &patch);
                    }
                }
                ViewSessionEvent::ViewportChanged { .. }
                | ViewSessionEvent::SelectionChanged { .. }
                | ViewSessionEvent::LayoutPublished { .. } => {}
                ViewSessionEvent::Closed { .. } => {
                    if let Some(lifecycle) = weak_lifecycle.upgrade() {
                        retire_lifecycle_all(&lifecycle);
                    }
                }
            })
            .ok();
        Self {
            view,
            lifecycle,
            terminal_subscription,
        }
    }

    #[must_use]
    pub fn view(&self) -> &ViewSession {
        &self.view
    }

    /// Closes the view and removes every egui state item registered by its node instances.
    pub fn close(&self) -> Result<ViewCloseOutcome, ViewSessionError> {
        let outcome = self.view.close();
        if outcome.is_ok() || self.view.close_reason().is_some() {
            self.retire_all();
        }
        outcome
    }

    /// Materializes, renders, measures, and submits one native egui frame.
    ///
    /// The closure runs once for every admitted node in runtime paint order. Its returned response
    /// is the node root measurement. Commands are recorded when [`EguiNativeNode::command`] is
    /// called; all node layouts are appended only after every closure has completed, then the host
    /// finishes and submits the feedback log exactly once.
    pub fn show(
        &self,
        ui: &mut Ui,
        request: FrameRequest,
        mut render_node: impl FnMut(&mut Ui, &EguiNativeNode<'_>) -> Response,
    ) -> Result<EguiNativeFrameOutcome, EguiNativeFrameError> {
        self.show_resolved(ui, request, |ui, node| Some(render_node(ui, node)))
    }

    /// Renders one frame while keeping unresolved-renderer handling inside the egui adapter.
    ///
    /// Return `None` when the requested [`RendererStamp`](jellyflow_runtime::view::RendererStamp)
    /// has no
    /// application renderer. The host then renders a diagnostic node and measures every real frame
    /// port, so unknown semantic kinds remain inspectable and connectable without interpreting or
    /// mutating their opaque graph payload.
    pub fn show_with_fallback(
        &self,
        ui: &mut Ui,
        request: FrameRequest,
        render_node: impl FnMut(&mut Ui, &EguiNativeNode<'_>) -> Option<Response>,
    ) -> Result<EguiNativeFrameOutcome, EguiNativeFrameError> {
        self.show_resolved(ui, request, render_node)
    }

    fn show_resolved(
        &self,
        ui: &mut Ui,
        request: FrameRequest,
        render_node: impl FnMut(&mut Ui, &EguiNativeNode<'_>) -> Option<Response>,
    ) -> Result<EguiNativeFrameOutcome, EguiNativeFrameError> {
        let outcome = self.render_frame(ui, request, render_node);
        self.audit_current_instances_if_stale();

        let outcome = outcome?;
        if outcome.receipt.request_next_frame() {
            ui.ctx().request_repaint();
        }
        Ok(outcome)
    }

    fn render_frame(
        &self,
        ui: &mut Ui,
        request: FrameRequest,
        mut render_node: impl FnMut(&mut Ui, &EguiNativeNode<'_>) -> Option<Response>,
    ) -> Result<EguiNativeFrameOutcome, EguiNativeFrameError> {
        let frame = self.view.frame(request)?;
        self.reconcile_frame_instances(&frame);
        let scene_origin = ui.max_rect().min;
        let collector = frame.collector();
        let mut layouts = Vec::with_capacity(frame.nodes().len());
        let mut rendered_instances = Vec::with_capacity(frame.nodes().len());
        let mut diagnostics = Vec::new();

        for frame_node in frame.nodes() {
            let node_rect = node_screen_rect(&frame, frame_node, scene_origin)?;
            let root_id = Id::new(("jellyflow-native-node", frame_node.instance()));
            let mut node_ui = ui.new_child(UiBuilder::new().id(root_id).max_rect(node_rect));
            node_ui.set_clip_rect(ui.clip_rect());
            let node = EguiNativeNode::new(frame_node, root_id, scene_origin, collector.clone());
            let root = match render_node(&mut node_ui, &node) {
                Some(root) => root,
                None => {
                    let diagnostic = EguiNativeNodeDiagnostic::missing_renderer(frame_node);
                    let root = render_missing_renderer(&mut node_ui, &node, &diagnostic)?;
                    diagnostics.push(diagnostic);
                    root
                }
            };
            let instance = frame_node.instance().clone();
            let (layout, states) = node.into_layout(root.rect)?;
            self.track_states(instance.clone(), ui.ctx(), states);
            layouts.push(layout);
            rendered_instances.push(instance);
        }

        for layout in layouts {
            collector.layout(layout)?;
        }
        let feedback = collector.finish()?;
        let receipt = self.view.submit_feedback(feedback)?;

        Ok(EguiNativeFrameOutcome {
            frame_graph_revision: frame.graph_revision(),
            frame_document_epoch: frame.document_epoch(),
            rendered_instances,
            diagnostics,
            receipt,
        })
    }

    fn track_states(
        &self,
        instance: NodeInstanceKey,
        context: &Context,
        states: Vec<EguiStateRegistration>,
    ) {
        if states.is_empty() {
            return;
        }
        if !self.instance_is_authoritative(&instance) {
            retire_egui_states(context, states);
            return;
        }
        let retired = {
            let mut lifecycle = self.lifecycle.borrow_mut();
            let replaced = lifecycle
                .instances_by_node
                .insert(instance.node(), instance.clone())
                .filter(|current| current != &instance)
                .and_then(|current| lifecycle.instances.remove(&current));
            let tracked = lifecycle.instances.entry(instance).or_default();
            tracked.track_states(context, states);
            replaced
        };
        if let Some(retired) = retired {
            retired.retire();
        }
    }

    fn instance_is_authoritative(&self, instance: &NodeInstanceKey) -> bool {
        if instance.view() != self.view.id()
            || self
                .lifecycle
                .borrow()
                .authoritative_instances
                .get(&instance.node())
                != Some(instance)
        {
            return false;
        }
        matches!(
            self.view.node_lifetime_key(instance.node()),
            Ok(Some(current)) if current == instance.lifetime()
        )
    }

    fn reconcile_frame_instances(&self, frame: &AdapterFrame) {
        let document_changed = self
            .lifecycle
            .borrow()
            .document_epoch
            .is_some_and(|current| current != frame.document_epoch());
        if document_changed {
            self.retire_all();
        }
        self.lifecycle.borrow_mut().document_epoch = Some(frame.document_epoch());

        let retired = {
            let mut lifecycle = self.lifecycle.borrow_mut();
            let mut retired = Vec::new();
            for node in frame.nodes() {
                let instance = node.instance();
                if let Some(current) = lifecycle.instances_by_node.get(&instance.node())
                    && current != instance
                {
                    retired.push(current.clone());
                }
                lifecycle
                    .authoritative_instances
                    .insert(instance.node(), instance.clone());
            }
            retired
        };
        self.retire_instances(retired);
    }

    fn audit_current_instances_if_stale(&self) {
        if self.view.close_reason().is_some() {
            self.retire_all();
            return;
        }
        let Ok(snapshot) = self.view.graph_snapshot() else {
            if self.view.close_reason().is_some() {
                self.retire_all();
            }
            return;
        };
        let lifecycle_is_current = {
            let lifecycle = self.lifecycle.borrow();
            lifecycle.observed_revision == Some(snapshot.revision())
                && lifecycle.document_epoch == Some(snapshot.document_epoch())
        };
        if lifecycle_is_current {
            return;
        }
        let document_changed = self
            .lifecycle
            .borrow()
            .document_epoch
            .is_some_and(|current| current != snapshot.document_epoch());
        if document_changed {
            self.retire_all();
            let mut lifecycle = self.lifecycle.borrow_mut();
            lifecycle.document_epoch = Some(snapshot.document_epoch());
            lifecycle.observed_revision = Some(snapshot.revision());
            return;
        }
        self.lifecycle.borrow_mut().document_epoch = Some(snapshot.document_epoch());

        let instances = self
            .lifecycle
            .borrow()
            .instances_by_node
            .values()
            .cloned()
            .collect::<Vec<_>>();
        #[cfg(test)]
        {
            let mut lifecycle = self.lifecycle.borrow_mut();
            lifecycle.full_audits = lifecycle.full_audits.saturating_add(1);
            lifecycle.full_audit_instance_visits = lifecycle
                .full_audit_instance_visits
                .saturating_add(instances.len() as u64);
        }
        let retired = instances
            .into_iter()
            .filter(|instance| {
                if instance.view() != self.view.id()
                    || snapshot.graph().node(&instance.node()).is_none()
                {
                    return true;
                }
                match self.view.node_lifetime_key(instance.node()) {
                    Ok(Some(current)) => current != instance.lifetime(),
                    Ok(None) => true,
                    Err(_) => false,
                }
            })
            .collect::<Vec<_>>();
        self.retire_instances(retired);
        {
            let mut lifecycle = self.lifecycle.borrow_mut();
            lifecycle.observed_revision = Some(snapshot.revision());
            lifecycle.document_epoch = Some(snapshot.document_epoch());
        }
        if self.view.close_reason().is_some() {
            self.retire_all();
        }
    }

    fn retire_instances(&self, instances: Vec<NodeInstanceKey>) {
        retire_lifecycle_instances(&self.lifecycle, instances);
    }

    fn retire_all(&self) {
        retire_lifecycle_all(&self.lifecycle);
    }
}

impl Drop for EguiNativeFrameHost {
    fn drop(&mut self) {
        if let Some(token) = self.terminal_subscription.take() {
            let _ = self.view.unsubscribe(token);
        }
        self.retire_all();
    }
}

#[derive(Default)]
struct EguiNativeLifecycle {
    document_epoch: Option<DocumentEpoch>,
    observed_revision: Option<GraphRevision>,
    authoritative_instances: BTreeMap<NodeId, NodeInstanceKey>,
    instances: BTreeMap<NodeInstanceKey, TrackedEguiInstance>,
    instances_by_node: BTreeMap<NodeId, NodeInstanceKey>,
    #[cfg(test)]
    full_audits: u64,
    #[cfg(test)]
    full_audit_instance_visits: u64,
}

#[derive(Default)]
struct TrackedEguiInstance {
    contexts: Vec<TrackedEguiContext>,
}

struct TrackedEguiContext {
    context: Context,
    states: Vec<EguiStateRegistration>,
}

impl TrackedEguiInstance {
    fn track_states(&mut self, context: &Context, states: Vec<EguiStateRegistration>) {
        if let Some(tracked) = self
            .contexts
            .iter_mut()
            .find(|tracked| tracked.context == *context)
        {
            tracked.extend(states);
            return;
        }

        let mut tracked = TrackedEguiContext {
            context: context.clone(),
            states: Vec::new(),
        };
        tracked.extend(states);
        self.contexts.push(tracked);
    }

    fn retire(self) {
        for tracked_context in self.contexts {
            for state in tracked_context.states {
                state.retire(&tracked_context.context);
            }
        }
    }
}

impl TrackedEguiContext {
    fn extend(&mut self, states: impl IntoIterator<Item = EguiStateRegistration>) {
        for state in states {
            if !self.states.contains(&state) {
                self.states.push(state);
            }
        }
    }
}

fn retire_lifecycle_all(lifecycle: &RefCell<EguiNativeLifecycle>) {
    let retired = {
        let mut lifecycle = lifecycle.borrow_mut();
        lifecycle.authoritative_instances.clear();
        lifecycle.instances_by_node.clear();
        std::mem::take(&mut lifecycle.instances)
    };
    for instance in retired.into_values() {
        instance.retire();
    }
}

fn retire_lifecycle_instances(
    lifecycle: &RefCell<EguiNativeLifecycle>,
    instances: impl IntoIterator<Item = NodeInstanceKey>,
) {
    let retired = {
        let mut lifecycle = lifecycle.borrow_mut();
        instances
            .into_iter()
            .filter_map(|instance| {
                let remove_authoritative = lifecycle
                    .authoritative_instances
                    .get(&instance.node())
                    .is_some_and(|current| current == &instance);
                if remove_authoritative {
                    lifecycle.authoritative_instances.remove(&instance.node());
                }
                let remove_index = lifecycle
                    .instances_by_node
                    .get(&instance.node())
                    .is_some_and(|current| current == &instance);
                if remove_index {
                    lifecycle.instances_by_node.remove(&instance.node());
                }
                lifecycle.instances.remove(&instance)
            })
            .collect::<Vec<_>>()
    };
    for instance in retired {
        instance.retire();
    }
}

fn reconcile_lifecycle_patch(lifecycle: &RefCell<EguiNativeLifecycle>, patch: &RuntimePatch) {
    let document_changed = lifecycle
        .borrow()
        .document_epoch
        .is_some_and(|current| current != patch.document_epoch());
    if document_changed || patch.kind() == RuntimePatchKind::DocumentReset {
        retire_lifecycle_all(lifecycle);
        let mut lifecycle = lifecycle.borrow_mut();
        lifecycle.document_epoch = Some(patch.document_epoch());
        lifecycle.observed_revision = Some(patch.to_revision());
        return;
    }
    let patch_is_contiguous = lifecycle.borrow().observed_revision == Some(patch.from_revision());
    lifecycle.borrow_mut().document_epoch = Some(patch.document_epoch());

    let retired = {
        let lifecycle = lifecycle.borrow();
        patch
            .node_lifetime_changes()
            .filter_map(|(node, current)| {
                let instance = lifecycle.authoritative_instances.get(&node)?;
                (current != Some(instance.lifetime())).then(|| instance.clone())
            })
            .collect::<Vec<_>>()
    };
    retire_lifecycle_instances(lifecycle, retired);
    lifecycle.borrow_mut().observed_revision = patch_is_contiguous.then_some(patch.to_revision());
}

#[derive(Clone, Copy)]
struct EguiStateRegistration {
    id: Id,
    state_type: TypeId,
    remove: fn(&Context, Id),
}

impl PartialEq for EguiStateRegistration {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.state_type == other.state_type
    }
}

impl Eq for EguiStateRegistration {}

impl EguiStateRegistration {
    fn new<T: 'static>(id: Id) -> Self {
        Self {
            id,
            state_type: TypeId::of::<T>(),
            remove: remove_egui_state::<T>,
        }
    }

    fn retire(self, context: &Context) {
        context.memory_mut(|memory| memory.surrender_focus(self.id));
        (self.remove)(context, self.id);
    }
}

fn remove_egui_state<T: 'static>(context: &Context, id: Id) {
    context.data_mut(|data| data.remove::<T>(id));
}

fn retire_egui_states(context: &Context, states: impl IntoIterator<Item = EguiStateRegistration>) {
    for state in states {
        state.retire(context);
    }
}

/// Per-node native egui rendering context for one materialized adapter frame.
pub struct EguiNativeNode<'frame> {
    frame_node: &'frame FrameNode,
    root_id: Id,
    scene_origin: Pos2,
    collector: FeedbackCollector,
    port_outcomes: RefCell<BTreeMap<PortId, EguiPortLayoutOutcome>>,
    states: RefCell<Vec<EguiStateRegistration>>,
}

impl<'frame> EguiNativeNode<'frame> {
    fn new(
        frame_node: &'frame FrameNode,
        root_id: Id,
        scene_origin: Pos2,
        collector: FeedbackCollector,
    ) -> Self {
        Self {
            frame_node,
            root_id,
            scene_origin,
            collector,
            port_outcomes: RefCell::new(BTreeMap::new()),
            states: RefCell::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn frame_node(&self) -> &FrameNode {
        self.frame_node
    }

    /// Stable root for native widget state, including the complete node instance identity.
    #[must_use]
    pub fn root_id(&self) -> Id {
        self.root_id
    }

    /// Derives a stable native widget ID beneath this node instance.
    #[must_use]
    pub fn id(&self, salt: impl Hash) -> Id {
        self.root_id.with(salt)
    }

    /// Returns a stable widget ID and registers state type `T` for hard-retirement cleanup.
    ///
    /// Use the returned ID for framework widgets that persist `T` in egui's `IdTypeMap`. The host
    /// removes only this exact `(Id, T)` pair when the node instance is replaced or retired.
    #[must_use]
    pub fn tracked_state_id<T: 'static>(&self, salt: impl Hash) -> Id {
        let id = self.id(salt);
        let registration = EguiStateRegistration::new::<T>(id);
        let mut states = self.states.borrow_mut();
        if !states.contains(&registration) {
            states.push(registration);
        }
        id
    }

    /// Numeric graph revision captured by this frame for display and diagnostics.
    #[must_use]
    pub fn graph_revision(&self) -> GraphRevision {
        self.frame_node.graph_snapshot().revision()
    }

    /// Graph version captured by this frame, including runtime provenance.
    #[must_use]
    pub fn graph_version(&self) -> GraphVersion {
        self.frame_node.graph_snapshot().version()
    }

    /// Appends a user command at the exact point its egui event is handled.
    pub fn command(&self, command: GraphCommand) -> Result<(), FeedbackCollectorError> {
        self.collector.command(command)
    }

    /// Measures a real graph port from one native response.
    ///
    /// The response rectangle supplies both visual and declared hit bounds. Its egui-clipped
    /// interaction rectangle supplies the effective hit bounds. The route anchor is derived from
    /// the requested outward side.
    pub fn measure_port(
        &self,
        port: PortId,
        response: &Response,
        outward: HandlePosition,
    ) -> Result<(), EguiNativePortError> {
        self.measure_port_with_hit(port, response, response, outward)
    }

    /// Measures a real graph port with distinct native visual and hit responses.
    pub fn measure_port_with_hit(
        &self,
        port: PortId,
        visual: &Response,
        hit: &Response,
        outward: HandlePosition,
    ) -> Result<(), EguiNativePortError> {
        let visual_bounds = egui_rect_to_view(visual.rect, self.scene_origin);
        let hit_bounds = egui_rect_to_view(hit.rect, self.scene_origin);
        let interaction_bounds = hit
            .interact_rect
            .is_positive()
            .then(|| egui_rect_to_view(hit.interact_rect, self.scene_origin));
        let route_anchor =
            side_anchor(visual_bounds, outward).ok_or(EguiNativePortError::InvalidBounds {
                node: self.frame_node.id(),
                port,
            })?;
        self.record_port_outcome(
            port,
            EguiPortLayoutOutcome::Rendered {
                visual_bounds,
                hit_bounds,
                interaction_bounds,
                route_anchor,
                outward,
            },
        )
    }

    /// Reports that product UI intentionally hides a real graph port for this frame.
    pub fn mark_port_hidden(&self, port: PortId) -> Result<(), EguiNativePortError> {
        self.record_port_outcome(port, EguiPortLayoutOutcome::Hidden)
    }

    /// Reports that immediate-mode clipping intentionally culled a real graph port this frame.
    pub fn mark_port_culled(&self, port: PortId) -> Result<(), EguiNativePortError> {
        self.record_port_outcome(port, EguiPortLayoutOutcome::Culled)
    }

    fn record_port_outcome(
        &self,
        port: PortId,
        outcome: EguiPortLayoutOutcome,
    ) -> Result<(), EguiNativePortError> {
        if !self
            .frame_node
            .ports()
            .iter()
            .any(|frame_port| frame_port.id() == port)
        {
            return Err(EguiNativePortError::UnknownPort {
                node: self.frame_node.id(),
                port,
            });
        }

        match self.port_outcomes.borrow_mut().entry(port) {
            Entry::Vacant(entry) => {
                entry.insert(outcome);
                Ok(())
            }
            Entry::Occupied(_) => Err(EguiNativePortError::DuplicatePort {
                node: self.frame_node.id(),
                port,
            }),
        }
    }

    fn into_layout(
        self,
        root: Rect,
    ) -> Result<(NodeLayout, Vec<EguiStateRegistration>), NodeLayoutBuilderError> {
        let mut layout = self
            .frame_node
            .begin_layout(egui_rect_to_view(root, self.scene_origin));
        let mut outcomes = self.port_outcomes.into_inner();
        for port in self.frame_node.ports() {
            match outcomes.remove(&port.id()) {
                Some(EguiPortLayoutOutcome::Rendered {
                    visual_bounds,
                    hit_bounds,
                    interaction_bounds,
                    route_anchor,
                    outward,
                }) => {
                    layout.port_rendered_with_interaction_bounds(
                        port.layout_target(),
                        visual_bounds,
                        hit_bounds,
                        interaction_bounds,
                        route_anchor,
                        outward,
                    )?;
                }
                Some(EguiPortLayoutOutcome::Hidden) => {
                    layout.port_hidden(port.layout_target())?;
                }
                Some(EguiPortLayoutOutcome::Culled) => {
                    layout.port_culled(port.layout_target())?;
                }
                None => {
                    layout.port_unavailable(port.layout_target())?;
                }
            }
        }

        Ok((layout.finish()?, self.states.into_inner()))
    }
}

enum EguiPortLayoutOutcome {
    Rendered {
        visual_bounds: ViewRect,
        hit_bounds: ViewRect,
        interaction_bounds: Option<ViewRect>,
        route_anchor: ViewPoint,
        outward: HandlePosition,
    },
    Hidden,
    Culled,
}

/// Result of one complete materialize-render-submit cycle.
#[derive(Debug)]
pub struct EguiNativeFrameOutcome {
    frame_graph_revision: GraphRevision,
    frame_document_epoch: DocumentEpoch,
    rendered_instances: Vec<NodeInstanceKey>,
    diagnostics: Vec<EguiNativeNodeDiagnostic>,
    receipt: FeedbackReceipt,
}

impl EguiNativeFrameOutcome {
    /// Graph revision captured before this frame's feedback commands were submitted.
    #[must_use]
    pub fn frame_graph_revision(&self) -> GraphRevision {
        self.frame_graph_revision
    }

    /// Document epoch captured when this frame was materialized.
    #[must_use]
    pub fn frame_document_epoch(&self) -> DocumentEpoch {
        self.frame_document_epoch
    }

    #[must_use]
    pub fn rendered_instances(&self) -> &[NodeInstanceKey] {
        &self.rendered_instances
    }

    /// Adapter-owned fallback diagnostics produced during this frame.
    #[must_use]
    pub fn diagnostics(&self) -> &[EguiNativeNodeDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn receipt(&self) -> &FeedbackReceipt {
        &self.receipt
    }

    #[must_use]
    pub fn into_receipt(self) -> FeedbackReceipt {
        self.receipt
    }
}

/// Recoverable node-rendering problem presented by the egui fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EguiNativeNodeDiagnostic {
    node: NodeId,
    node_kind: Option<NodeKindKey>,
    renderer_key: String,
    message: String,
}

impl EguiNativeNodeDiagnostic {
    fn missing_renderer(node: &FrameNode) -> Self {
        let node_kind = node.node().map(|record| record.kind.clone());
        let renderer_key = node.renderer().key().to_owned();
        let message = match &node_kind {
            Some(kind) => format!(
                "no egui renderer `{renderer_key}` is registered for node kind `{}`",
                kind.0
            ),
            None => format!("no egui renderer `{renderer_key}` is registered"),
        };
        Self {
            node: node.id(),
            node_kind,
            renderer_key,
            message,
        }
    }

    #[must_use]
    pub fn node(&self) -> NodeId {
        self.node
    }

    #[must_use]
    pub fn node_kind(&self) -> Option<&NodeKindKey> {
        self.node_kind.as_ref()
    }

    #[must_use]
    pub fn renderer_key(&self) -> &str {
        &self.renderer_key
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EguiNativeFrameError {
    #[error(transparent)]
    Materialization(#[from] FrameMaterializationError),
    #[error(transparent)]
    Collection(#[from] FeedbackCollectorError),
    #[error(transparent)]
    Layout(#[from] NodeLayoutBuilderError),
    #[error(transparent)]
    Submission(#[from] FeedbackSubmissionError),
    #[error(transparent)]
    Port(#[from] EguiNativePortError),
    #[error("node {node:?} cannot be projected into finite egui coordinates")]
    InvalidNodePlacement { node: NodeId },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EguiNativePortError {
    #[error("node {node:?} has no real frame port {port:?}")]
    UnknownPort { node: NodeId, port: PortId },
    #[error("node {node:?} already reported a layout outcome for real frame port {port:?}")]
    DuplicatePort { node: NodeId, port: PortId },
    #[error("node {node:?} reported invalid native bounds for real frame port {port:?}")]
    InvalidBounds { node: NodeId, port: PortId },
}

fn render_missing_renderer(
    ui: &mut Ui,
    node: &EguiNativeNode<'_>,
    diagnostic: &EguiNativeNodeDiagnostic,
) -> Result<Response, EguiNativePortError> {
    let mut measured_ports = Vec::with_capacity(node.frame_node().ports().len());
    let root = ui.group(|ui| {
        ui.strong("Renderer unavailable");
        ui.monospace(diagnostic.renderer_key());
        for frame_port in node.frame_node().ports() {
            let direction = frame_port.port().map(|port| port.dir);
            let port_label = frame_port
                .port()
                .map(|port| port.key.0.as_str())
                .unwrap_or("unknown port");
            ui.horizontal(|ui| match direction {
                Some(PortDirection::In) => {
                    let (_, response) =
                        ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::drag());
                    ui.monospace(port_label);
                    measured_ports.push((frame_port.id(), response, HandlePosition::Left));
                }
                Some(PortDirection::Out) | None => {
                    ui.monospace(port_label);
                    let (_, response) =
                        ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::drag());
                    measured_ports.push((frame_port.id(), response, HandlePosition::Right));
                }
            });
        }
    });
    for (port, response, outward) in measured_ports {
        node.measure_port(port, &response, outward)?;
    }
    Ok(root.response)
}

fn node_screen_rect(
    frame: &AdapterFrame,
    node: &FrameNode,
    scene_origin: Pos2,
) -> Result<Rect, EguiNativeFrameError> {
    let bounds = node.bounds();
    let Some(origin) = frame.transform().screen_point_for_canvas(bounds.origin) else {
        return Err(EguiNativeFrameError::InvalidNodePlacement { node: node.id() });
    };
    let size = Vec2::new(
        bounds.size.width * frame.transform().zoom,
        bounds.size.height * frame.transform().zoom,
    );
    let min = scene_origin + Vec2::new(origin.x, origin.y);
    if !min.is_finite() || !size.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return Err(EguiNativeFrameError::InvalidNodePlacement { node: node.id() });
    }
    Ok(Rect::from_min_size(min, size))
}

fn egui_rect_to_view(rect: Rect, scene_origin: Pos2) -> ViewRect {
    ViewRect::new(
        ViewPoint::new(rect.min.x - scene_origin.x, rect.min.y - scene_origin.y),
        ViewSize::new(rect.width(), rect.height()),
    )
}

#[cfg(test)]
mod tests {
    use jellyflow_core::{
        CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId,
    };
    use jellyflow_runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
    use jellyflow_runtime::graph::GraphRuntime;
    use jellyflow_runtime::view::{FrameMaterializationBudget, RendererStamp, ViewSessionOptions};

    use super::*;

    #[test]
    fn stable_ten_thousand_node_lifecycle_uses_revision_watermark() {
        const NODE_COUNT: usize = 10_000;
        let mut builder = GraphBuilder::new(GraphId::from_u128(0xe601));
        let mut nodes = Vec::with_capacity(NODE_COUNT);
        for index in 0..NODE_COUNT {
            let node = NodeId::from_u128(index as u128 + 1);
            nodes.push(node);
            builder.insert_node(
                node,
                Node {
                    kind: NodeKindKey::new("test.egui-lifecycle-scale"),
                    kind_version: 1,
                    pos: CanvasPoint {
                        x: (index % 100) as f32 * 20.0,
                        y: (index / 100) as f32 * 20.0,
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
                        width: 16.0,
                        height: 16.0,
                    }),
                    hidden: false,
                    collapsed: false,
                    ports: Vec::new(),
                    data: serde_json::Value::Null,
                },
            );
        }
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("valid scale graph"))
            .expect("scale runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("scale view");
        let host = EguiNativeFrameHost::new(view);
        let frame = host
            .view()
            .frame(
                FrameRequest::new(
                    CanvasRect {
                        origin: CanvasPoint::default(),
                        size: CanvasSize {
                            width: 2_100.0,
                            height: 2_100.0,
                        },
                    },
                    RendererStamp::new("test.egui-lifecycle-scale", 1),
                )
                .with_materialization_budget(
                    FrameMaterializationBudget::new(NODE_COUNT, 0, 0)
                        .with_visible_node_scan_limit(NODE_COUNT * 2),
                ),
            )
            .expect("scale frame");
        assert_eq!(frame.nodes().len(), NODE_COUNT);
        {
            let mut lifecycle = host.lifecycle.borrow_mut();
            for node in frame.nodes() {
                let instance = node.instance().clone();
                lifecycle
                    .authoritative_instances
                    .insert(node.id(), instance.clone());
                lifecycle
                    .instances_by_node
                    .insert(node.id(), instance.clone());
                lifecycle.instances.insert(instance, Default::default());
            }
        }

        for _ in 0..120 {
            host.audit_current_instances_if_stale();
        }
        assert_eq!(host.lifecycle.borrow().full_audits, 0);
        assert_eq!(host.lifecycle.borrow().full_audit_instance_visits, 0);

        runtime
            .execute(GraphCommand::topology(
                runtime.snapshot().expect("retirement snapshot").version(),
                TopologyEdit::new().remove_node(nodes[0], IncidentEdgePolicy::Reject),
            ))
            .expect("retirement executes")
            .applied()
            .expect("retirement applies");
        assert!(
            !host
                .lifecycle
                .borrow()
                .instances_by_node
                .contains_key(&nodes[0])
        );
        host.audit_current_instances_if_stale();
        assert_eq!(host.lifecycle.borrow().full_audits, 0);

        host.lifecycle.borrow_mut().observed_revision = None;
        host.audit_current_instances_if_stale();
        let lifecycle = host.lifecycle.borrow();
        assert_eq!(lifecycle.full_audits, 1);
        assert_eq!(
            lifecycle.full_audit_instance_visits,
            (NODE_COUNT - 1) as u64
        );
    }
}
