mod connection;
mod diagnostics;
mod frame_feedback;
mod lifecycle;
mod node_lifecycle;
mod retained_mount_store;
mod scene_gesture;
mod scene_render;

pub use self::connection::{
    OpenGpuiConnectionCommitEvent, OpenGpuiConnectionCommitFailure,
    OpenGpuiConnectionCommitOutcome, OpenGpuiConnectionCommitStage,
    OpenGpuiCreateFromWireCandidates, OpenGpuiCreateFromWireError,
    OpenGpuiCreateFromWireSubmission,
};
pub use self::diagnostics::{
    OpenGpuiEdgeLayerOverflow, OpenGpuiSceneHostDiagnostic, OpenGpuiSceneHostDiagnosticKind,
    OpenGpuiSceneHostStats,
};
pub use self::lifecycle::OpenGpuiSceneHostCloseError;

use self::connection::{MountedConnectionOwner, OpenGpuiConnectionPreview};
use self::frame_feedback::DeferredFrameFeedback;
#[cfg(test)]
use self::frame_feedback::{LayoutReceiptAction, layout_receipt_action};
#[cfg(test)]
use self::lifecycle::HostLifecycleSignal;
use self::node_lifecycle::MountedNode;
#[cfg(test)]
use self::scene_render::{
    OPEN_GPUI_SCENE_LAYER_ORDER, OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR,
    OpenGpuiConnectionPreviewEvidence, OpenGpuiSceneEvidence, OpenGpuiSceneLayer,
};

use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::schema::NodeCatalog;
use jellyflow::runtime::{
    AdapterEnvironment, FrameMaterializationBudget, FrameMaterializationError,
    FrameMaterializationOverflow, FrameRequest, RendererStamp, ViewCloseReason, ViewPoint,
    ViewRect, ViewSession, ViewSessionError, ViewSize, WeakViewSession,
};
use open_gpui::{Context, FocusHandle, IntoElement, Pixels, Render, Window};

use self::lifecycle::HostLifecycleBridge;
use self::retained_mount_store::RetainedMountStore;
use self::scene_gesture::SceneGestureController;

use crate::activity::OpenGpuiActivityCoordinator;
use crate::command_journal::{
    OpenGpuiCommandJournal, OpenGpuiCommandTicketError, resolve_command_tickets,
};
use crate::connection_controller::{OpenGpuiConnectionController, OpenGpuiCreateFromWireRequest};
use crate::interaction::{OpenGpuiInteractionCoordinator, OpenGpuiSceneInteractionCoordinator};
use crate::port_handle::OpenGpuiPendingMeasurements;
use crate::registry::OpenGpuiNodeRegistry;

#[cfg(test)]
use crate::binding::OpenGpuiNodeBinding;
#[cfg(test)]
use crate::command_journal::OpenGpuiCommandOrigin;
#[cfg(test)]
use crate::connection_controller::{
    OpenGpuiConnectionCancelReason, OpenGpuiConnectionCommitRejection, OpenGpuiConnectionIntent,
    OpenGpuiConnectionPhase,
};
#[cfg(test)]
use crate::interaction::{OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGesturePhase};

/// Maximum number of inactive, unprotected retained nodes kept by a scene host.
pub const OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY: usize = 256;
/// Maximum number of distinct retained nodes that may own retention-protecting activity per view.
pub const OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY: usize = 64;
/// Maximum number of published edge paths materialized by one native paint pass.
pub const OPEN_GPUI_DEFAULT_EDGE_LAYER_BUDGET: usize = 4_096;
/// Maximum selected IDs plus published route entries inspected by one edge-layer pass.
pub const OPEN_GPUI_DEFAULT_EDGE_ROUTE_SCAN_LIMIT: usize = 16_384;

const DIAGNOSTIC_RENDERER_KEY: &str = "jellyflow.open-gpui.diagnostic";
const RENDERER_INCARNATION: u64 = 1;

/// Retained Open GPUI scene host backed by one headless [`ViewSession`].
pub struct OpenGpuiSceneHost {
    view: ViewSession,
    commands: Rc<OpenGpuiCommandJournal>,
    catalog: Arc<NodeCatalog>,
    registry: OpenGpuiNodeRegistry,
    frame_request_template: FrameRequest,
    viewport: CanvasRect,
    surface_size: Option<ViewSize>,
    tracks_surface_viewport: bool,
    frame_materialization_budget: FrameMaterializationBudget,
    edge_layer_budget: usize,
    edge_route_scan_limit: usize,
    mounts: RetainedMountStore<MountedNode>,
    activity_coordinator: OpenGpuiActivityCoordinator,
    interactions: OpenGpuiInteractionCoordinator,
    scene_interactions: OpenGpuiSceneInteractionCoordinator,
    scene_gesture: SceneGestureController,
    connections: OpenGpuiConnectionController,
    connection_owner: Option<MountedConnectionOwner>,
    connection_preview: Option<OpenGpuiConnectionPreview>,
    last_create_from_wire: Option<OpenGpuiCreateFromWireRequest>,
    last_connection_commit: Option<OpenGpuiConnectionCommitEvent>,
    scene_origin: Option<open_gpui::Point<Pixels>>,
    scene_transform: Option<ViewportTransform>,
    interaction_focus: Option<FocusHandle>,
    document_epoch: Option<jellyflow::runtime::DocumentEpoch>,
    terminal_reason: Option<ViewCloseReason>,
    lifecycle_bridge: Option<HostLifecycleBridge>,
    #[cfg(test)]
    scene_evidence: Rc<OpenGpuiSceneEvidence>,
    #[cfg(test)]
    scene_wires_fault: Option<ViewSessionError>,
    stats: OpenGpuiSceneHostStats,
    last_diagnostic: Option<OpenGpuiSceneHostDiagnostic>,
    last_frame_materialization_overflow: FrameMaterializationOverflow,
    last_edge_layer_overflow: OpenGpuiEdgeLayerOverflow,
}

fn catalog_frame_request(catalog: &NodeCatalog, viewport: CanvasRect) -> FrameRequest {
    let mut request = FrameRequest::new(
        viewport,
        RendererStamp::new(DIAGNOSTIC_RENDERER_KEY, RENDERER_INCARNATION),
    );
    for spec in catalog.kinds() {
        request = request.with_kind_renderer(
            spec.kind().clone(),
            RendererStamp::new(spec.renderer_key(), RENDERER_INCARNATION),
        );
        if let Some(size) = spec.default_size() {
            request = request.with_kind_default(spec.kind().clone(), size);
        }
    }
    request.with_incident_routing_dependencies()
}

fn effective_frame_materialization_budget(
    configured: FrameMaterializationBudget,
    protected_required_nodes: usize,
) -> FrameMaterializationBudget {
    FrameMaterializationBudget::new(
        configured.visible_nodes(),
        protected_required_nodes,
        configured.routing_dependency_nodes(),
    )
    .with_visible_node_scan_limit(configured.visible_node_scan_limit())
}

impl OpenGpuiSceneHost {
    #[must_use]
    pub fn new(view: ViewSession, registry: OpenGpuiNodeRegistry, viewport: CanvasRect) -> Self {
        let catalog = view.catalog_owned();
        let frame_request_template = catalog_frame_request(&catalog, viewport);
        Self {
            view,
            commands: OpenGpuiCommandJournal::new(),
            catalog,
            registry,
            frame_request_template,
            viewport,
            surface_size: None,
            tracks_surface_viewport: false,
            frame_materialization_budget: FrameMaterializationBudget::default(),
            edge_layer_budget: OPEN_GPUI_DEFAULT_EDGE_LAYER_BUDGET,
            edge_route_scan_limit: OPEN_GPUI_DEFAULT_EDGE_ROUTE_SCAN_LIMIT,
            mounts: RetainedMountStore::new(OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY),
            activity_coordinator: OpenGpuiActivityCoordinator::new(
                OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
            ),
            interactions: OpenGpuiInteractionCoordinator::new(),
            scene_interactions: OpenGpuiSceneInteractionCoordinator::new(),
            scene_gesture: SceneGestureController::default(),
            connections: OpenGpuiConnectionController::new(),
            connection_owner: None,
            connection_preview: None,
            last_create_from_wire: None,
            last_connection_commit: None,
            scene_origin: None,
            scene_transform: None,
            interaction_focus: None,
            document_epoch: None,
            terminal_reason: None,
            lifecycle_bridge: None,
            #[cfg(test)]
            scene_evidence: Rc::new(OpenGpuiSceneEvidence::new()),
            #[cfg(test)]
            scene_wires_fault: None,
            stats: OpenGpuiSceneHostStats::default(),
            last_diagnostic: None,
            last_frame_materialization_overflow: FrameMaterializationOverflow::default(),
            last_edge_layer_overflow: OpenGpuiEdgeLayerOverflow::default(),
        }
    }

    /// Creates a host whose first frame only bootstraps native surface measurement.
    #[must_use]
    pub fn new_measured_surface(view: ViewSession, registry: OpenGpuiNodeRegistry) -> Self {
        let mut host = Self::new(
            view,
            registry,
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 1.0,
                    height: 1.0,
                },
            },
        );
        host.tracks_surface_viewport = true;
        host
    }

    /// Overrides native frame materialization limits before the host is mounted.
    ///
    /// The host replaces the required-node limit per frame with the exact number of protected
    /// owners. Those owners are independently bounded by [`OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY`].
    #[must_use]
    pub fn with_frame_materialization_budget(mut self, budget: FrameMaterializationBudget) -> Self {
        self.frame_materialization_budget = budget;
        self
    }

    /// Replaces native frame materialization limits and schedules a fresh frame when they change.
    ///
    /// The host replaces the required-node limit per frame with the exact number of protected
    /// owners. Those owners are independently bounded by [`OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY`].
    pub fn set_frame_materialization_budget(
        &mut self,
        budget: FrameMaterializationBudget,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.frame_materialization_budget == budget {
            return false;
        }
        self.frame_materialization_budget = budget;
        cx.notify();
        true
    }

    #[must_use]
    pub const fn frame_materialization_budget(&self) -> FrameMaterializationBudget {
        self.frame_materialization_budget
    }

    /// Overrides the independent published edge-path limit before the host is mounted.
    #[must_use]
    pub fn with_edge_layer_budget(mut self, budget: usize) -> Self {
        self.edge_layer_budget = budget;
        self
    }

    /// Replaces the published edge-path limit and schedules a fresh native paint pass.
    pub fn set_edge_layer_budget(&mut self, budget: usize, cx: &mut Context<Self>) -> bool {
        if self.edge_layer_budget == budget {
            return false;
        }
        self.edge_layer_budget = budget;
        cx.notify();
        true
    }

    #[must_use]
    pub const fn edge_layer_budget(&self) -> usize {
        self.edge_layer_budget
    }

    /// Overrides the selected-ID plus published-route scan limit before the host is mounted.
    #[must_use]
    pub fn with_edge_route_scan_limit(mut self, limit: usize) -> Self {
        self.edge_route_scan_limit = limit;
        self
    }

    /// Replaces the route scan limit and schedules a fresh native paint pass.
    pub fn set_edge_route_scan_limit(&mut self, limit: usize, cx: &mut Context<Self>) -> bool {
        if self.edge_route_scan_limit == limit {
            return false;
        }
        self.edge_route_scan_limit = limit;
        cx.notify();
        true
    }

    #[must_use]
    pub const fn edge_route_scan_limit(&self) -> usize {
        self.edge_route_scan_limit
    }

    /// Returns a non-owning capability for controls that operate on this exact view session.
    #[must_use]
    pub fn view_handle(&self) -> WeakViewSession {
        self.view.downgrade()
    }

    /// Advances geometry-affecting native environment generations for this view only.
    ///
    /// A changed environment makes previously issued feedback stale and schedules a fresh native
    /// measurement pass. Reapplying the same environment is a benign no-op.
    pub fn set_adapter_environment(
        &mut self,
        environment: AdapterEnvironment,
        cx: &mut Context<Self>,
    ) -> Result<bool, ViewSessionError> {
        let changed = self.view.set_adapter_environment(environment)?;
        if changed {
            cx.notify();
        }
        Ok(changed)
    }

    pub fn set_frame_bounds(&mut self, frame_bounds: CanvasRect) {
        self.tracks_surface_viewport = false;
        self.viewport = frame_bounds;
    }

    /// Resumes deriving frame bounds from the measured native surface and current pan/zoom.
    ///
    /// Calling [`Self::set_frame_bounds`] switches to manual frame bounds; this method restores the
    /// default measured-surface behavior.
    pub fn track_surface_viewport(&mut self) {
        self.tracks_surface_viewport = true;
    }

    #[must_use]
    pub fn frame_bounds(&self) -> CanvasRect {
        self.viewport
    }

    /// Returns the latest measured surface size in local logical pixels.
    #[must_use]
    pub fn surface_size(&self) -> Option<ViewSize> {
        self.surface_size
    }

    fn frame_request(&self) -> FrameRequest {
        let viewport = if self.tracks_surface_viewport {
            self.surface_size
                .and_then(|size| {
                    self.view
                        .viewport_transform()
                        .ok()
                        .and_then(|transform| canvas_viewport_for_surface(size, transform))
                })
                .unwrap_or(self.viewport)
        } else {
            self.viewport
        };
        let protected_required_nodes = self
            .mounts
            .iter()
            .filter(|(_, mounted)| mounted.activity.protected_count() > 0)
            .map(|(instance, _)| instance.node())
            .collect::<BTreeSet<_>>();
        let materialization_budget = effective_frame_materialization_budget(
            self.frame_materialization_budget,
            protected_required_nodes.len(),
        );
        self.frame_request_template
            .clone()
            .with_viewport(viewport)
            .with_materialization_budget(materialization_budget)
            .with_required_nodes(protected_required_nodes)
    }

    fn update_surface_viewport(
        &mut self,
        surface_size: ViewSize,
        transform: ViewportTransform,
    ) -> bool {
        self.surface_size = Some(surface_size);
        if !self.tracks_surface_viewport {
            return false;
        }
        if let Some(viewport) = canvas_viewport_for_surface(surface_size, transform) {
            let changed = self.viewport != viewport;
            self.viewport = viewport;
            return changed;
        }
        false
    }
}

fn canvas_viewport_for_surface(
    surface_size: ViewSize,
    transform: ViewportTransform,
) -> Option<CanvasRect> {
    ViewRect::new(ViewPoint::default(), surface_size).to_canvas_rect(transform)
}

impl Render for OpenGpuiSceneHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host_id = cx.entity_id();
        let focus_handle = self
            .interaction_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        self.interactions.set_focus_handle(focus_handle.clone());
        self.scene_interactions
            .set_focus_handle(focus_handle.clone());
        self.interactions.schedule_pending_drain(window, cx);
        self.scene_interactions.schedule_pending_drain(window, cx);
        self.ensure_lifecycle_bridge(cx);
        if let Some(reason) = self.close_reason() {
            let completed = self.commands.terminate(reason);
            self.enter_terminal(reason);
            let resolutions = self.handle_completed_commands(completed);
            cx.defer(move |_| resolve_command_tickets(resolutions));
            self.interactions.schedule_pending_drain(window, cx);
            return self.terminal_scene(host_id, focus_handle);
        }
        let frame = match self.view.frame(self.frame_request()) {
            Ok(frame) => frame,
            Err(FrameMaterializationError::Session(ViewSessionError::Closed { reason })) => {
                let completed = self.commands.terminate(reason);
                self.enter_terminal(reason);
                let resolutions = self.handle_completed_commands(completed);
                cx.defer(move |_| resolve_command_tickets(resolutions));
                self.interactions.schedule_pending_drain(window, cx);
                self.scene_interactions.schedule_pending_drain(window, cx);
                return self.terminal_scene(host_id, focus_handle);
            }
            Err(error) => return self.frame_failure(error.to_string(), cx),
        };
        self.record_frame_materialization_overflow(frame.materialization_overflow());
        let collector = frame.collector();
        let command_batch = match self.commands.collect_next_batch(&collector) {
            Ok(batch) => batch,
            Err(error) => {
                return self.frame_failure(
                    format!("failed to collect native command batch: {error}"),
                    cx,
                );
            }
        };
        let measurements = Rc::new(OpenGpuiPendingMeasurements::new(
            frame.nodes(),
            frame.transform(),
        ));
        self.reconcile_frame(
            frame.nodes(),
            frame.document_epoch(),
            &measurements,
            window,
            cx,
        );
        self.interactions.schedule_pending_drain(window, cx);
        let wires = match self.scene_wires() {
            Ok(wires) => wires,
            Err(ViewSessionError::Closed { reason }) => {
                let completed = self.commands.terminate(reason);
                self.enter_terminal(reason);
                let resolutions = self.handle_completed_commands(completed);
                cx.defer(move |_| resolve_command_tickets(resolutions));
                self.interactions.schedule_pending_drain(window, cx);
                self.scene_interactions.schedule_pending_drain(window, cx);
                return self.terminal_scene(host_id, focus_handle);
            }
            Err(error) => {
                let message = error.to_string();
                let mut resolutions = Vec::new();
                let mut batch_failure = None;
                if let Some(batch) = command_batch {
                    match self.commands.fail_batch(batch, || {
                        OpenGpuiCommandTicketError::FramePreparation {
                            message: message.clone(),
                        }
                    }) {
                        Ok(completed) => {
                            resolutions.extend(self.handle_completed_commands(completed));
                        }
                        Err(error) => batch_failure = Some(error),
                    }
                }
                cx.defer(move |_| resolve_command_tickets(resolutions));
                let message = batch_failure.map_or(message.clone(), |error| {
                    format!("{message}; failed to settle native command batch: {error}")
                });
                return self.frame_failure(message, cx);
            }
        };
        self.render_scene(
            Rc::new(DeferredFrameFeedback::new(
                collector,
                measurements,
                Rc::clone(&self.commands),
                command_batch,
                !self.scene_gesture.has_node_drag_visual(),
            )),
            frame.transform(),
            focus_handle,
            wires,
            cx,
        )
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
