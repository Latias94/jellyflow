use egui::{Context, Event, Id, Key, PointerButton, Pos2, Rect, Response};
use jellyflow_core::{EdgeId, NodeGraphConnectionMode, PortId};
use jellyflow_runtime::commands::{CommandExecutionCompletion, GraphCommand};
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint, ViewSize};
use jellyflow_runtime::graph::{NodeLifetimeKey, PortInstanceKey};
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasInteractionPresentation, CanvasInteractionSession,
    CanvasInteractionUpdate, CanvasPressTarget, ConnectionInteractionStart,
    ConnectionSearchRequest, ConnectionSearchSelection, InteractionDamage,
    InteractionGraphSubmission, InteractionGraphSubmissionBindError, InteractionOwner,
    InteractionTickDemand, InteractionTicketId, SelectionMode,
};
use jellyflow_runtime::schema::NodeCreation;
use jellyflow_runtime::view::{ViewSession, ViewSessionError, WeakViewSession};
use jellyflow_runtime::viewport::ViewportPointerSample;

const DEFAULT_QUERY_PAGES_PER_FRAME: usize = 1;

/// Ordered semantic work emitted while translating egui input.
///
/// View-owned selection and viewport changes are committed by the headless interaction session.
/// Search and graph-submission effects remain explicit so the application can connect them to its
/// native search UI and ordered command transport.
#[derive(Debug, Default)]
pub struct EguiInteractionOutput {
    effects: Vec<CanvasInteractionEffect>,
    resolved_query_count: usize,
    terminal_query_error: Option<CanvasInteractionError>,
    damage: InteractionDamage,
    tick_demand: InteractionTickDemand,
    released_native_ownership: bool,
    renderer_work_pending: bool,
}

impl EguiInteractionOutput {
    pub fn effects(&self) -> &[CanvasInteractionEffect] {
        &self.effects
    }

    pub fn take_effects(&mut self) -> Vec<CanvasInteractionEffect> {
        std::mem::take(&mut self.effects)
    }

    pub fn into_effects(self) -> Vec<CanvasInteractionEffect> {
        self.effects
    }

    /// Returns how many runtime-owned query pages were attempted during this output.
    pub fn resolved_query_count(&self) -> usize {
        self.resolved_query_count
    }

    /// Returns a terminal query failure after its cleanup update has already been applied.
    pub fn terminal_query_error(&self) -> Option<CanvasInteractionError> {
        self.terminal_query_error
    }

    pub fn damage(&self) -> InteractionDamage {
        self.damage
    }

    pub fn tick_demand(&self) -> InteractionTickDemand {
        self.tick_demand
    }

    pub fn released_native_ownership(&self) -> bool {
        self.released_native_ownership
    }

    /// Returns whether runtime-owned query continuation work remains for a later frame.
    pub fn renderer_work_pending(&self) -> bool {
        self.renderer_work_pending
    }

    pub fn has_work(&self) -> bool {
        !self.effects.is_empty()
            || self.resolved_query_count != 0
            || self.terminal_query_error.is_some()
            || self.damage != InteractionDamage::default()
            || self.tick_demand != InteractionTickDemand::None
            || self.released_native_ownership
            || self.renderer_work_pending
    }

    pub fn merge(&mut self, other: Self) {
        self.effects.extend(other.effects);
        self.resolved_query_count = self
            .resolved_query_count
            .saturating_add(other.resolved_query_count);
        if other.terminal_query_error.is_some() {
            self.terminal_query_error = other.terminal_query_error;
        }
        merge_damage(&mut self.damage, other.damage);
        if other.tick_demand == InteractionTickDemand::WhileActive {
            self.tick_demand = InteractionTickDemand::WhileActive;
        }
        self.released_native_ownership |= other.released_native_ownership;
        self.renderer_work_pending = other.renderer_work_pending;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EguiInteractionError {
    #[error(transparent)]
    Interaction(#[from] CanvasInteractionError),
    #[error(transparent)]
    View(#[from] ViewSessionError),
    #[error(transparent)]
    Submission(#[from] InteractionGraphSubmissionBindError),
    #[error("an egui interaction frame is already open")]
    FrameAlreadyOpen,
    #[error("no egui interaction frame is open")]
    FrameNotOpen,
    #[error("the egui scene rectangle is invalid")]
    InvalidSceneRect,
}

#[derive(Debug)]
struct EguiInteractionFrame {
    scene_rect: Rect,
    pointer_pos: Option<Pos2>,
    primary_pressed: bool,
    primary_released: bool,
    primary_down: bool,
    selection_delete_pressed: bool,
    selection_delete_handled: bool,
    escape_pressed: bool,
    pointer_gone: bool,
    elapsed_seconds: f32,
    tracked_response_seen: bool,
    tracked_response_down: bool,
    tracked_response_pointer: Option<Pos2>,
    renderer_query_pages_remaining: usize,
    output: EguiInteractionOutput,
}

/// Immediate-mode input and ownership bridge for one renderer-neutral canvas interaction session.
///
/// Call [`Self::begin_frame`], feed every graph-interactive response through
/// [`Self::handle_response`], then call [`Self::end_frame`]. A tracked response that disappears in
/// a later frame retires its semantic owner; escape, pointer loss, or stolen egui drag ownership
/// cancels the session deterministically.
#[derive(Debug)]
pub struct EguiCanvasInteraction {
    view: WeakViewSession,
    session: CanvasInteractionSession,
    tracked_response: Option<Id>,
    tracked_owner: Option<InteractionOwner>,
    native_capture: bool,
    last_position: Option<ViewportPointerSample>,
    frame: Option<EguiInteractionFrame>,
}

impl EguiCanvasInteraction {
    pub fn new(
        view: &ViewSession,
        config: CanvasInteractionConfig,
    ) -> Result<Self, EguiInteractionError> {
        let session = CanvasInteractionSession::attach(view, config)?;
        Ok(Self {
            view: view.downgrade(),
            session,
            tracked_response: None,
            tracked_owner: None,
            native_capture: false,
            last_position: None,
            frame: None,
        })
    }

    pub fn presentation(&self) -> CanvasInteractionPresentation {
        self.session.presentation()
    }

    pub fn tracked_response(&self) -> Option<Id> {
        self.tracked_response
    }

    pub fn owns_native_pointer(&self) -> bool {
        self.native_capture
    }

    /// Resolves an explicit node drag handle from authoritative graph and selection facts.
    pub fn node_drag_target(
        &self,
        node: NodeLifetimeKey,
        selection_mode: SelectionMode,
    ) -> Result<Option<CanvasPressTarget>, EguiInteractionError> {
        Ok(self
            .view
            .node_drag_start(node, selection_mode)?
            .map(CanvasPressTarget::NodeDrag))
    }

    /// Resolves an explicit port handle from authoritative policy and published geometry.
    pub fn connection_target(
        &self,
        port: PortInstanceKey,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        allow_create_from_wire: bool,
    ) -> Result<Option<CanvasPressTarget>, EguiInteractionError> {
        let drag_intent = self.view.port_connection_drag_intent(port)?;
        let binding =
            ConnectionInteractionStart::binding_from_port_drag(drag_intent, new_edge, mode);
        let Some(binding) = binding else {
            return Ok(None);
        };
        let geometry = self.view.geometry_snapshot()?;
        let Some(fixed) = geometry.port(binding.fixed_port_instance().port()) else {
            return Ok(None);
        };
        let moving_outward = if let Some(detached) = binding.detached_port_instance() {
            let Some(moving) = geometry.port(detached.port()) else {
                return Ok(None);
            };
            moving.outward()
        } else {
            opposite_handle_position(fixed.outward())
        };
        Ok(self
            .view
            .connection_start_from_port_drag_intent(
                drag_intent,
                new_edge,
                mode,
                fixed.route_anchor(),
                fixed.outward(),
                moving_outward,
                allow_create_from_wire,
            )?
            .map(CanvasPressTarget::Connection))
    }

    /// Activates one real port through the shared non-drag connection authority.
    ///
    /// The first activation arms a source and the next activation evaluates a target. The supplied
    /// port instance must come from the current adapter frame; source/target state remains
    /// exclusively owned by the renderer-neutral interaction session.
    pub fn activate_connection_port(
        &mut self,
        ctx: &Context,
        port: PortInstanceKey,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let port = self.view.keyboard_connection_port(port)?;
        let mut output = self.drain_frame_output();
        let update = self
            .session
            .activate_connection_port(port, prospective_edge, mode)?;
        self.process_update_with_current_budget(update, &mut output);
        self.sync_tracked_response(ctx, &mut output);
        Ok(self.finish_output(ctx, output))
    }

    pub fn begin_frame(
        &mut self,
        ctx: &Context,
        scene_rect: Rect,
    ) -> Result<(), EguiInteractionError> {
        self.begin_frame_with_query_page_budget(ctx, scene_rect, DEFAULT_QUERY_PAGES_PER_FRAME)
    }

    /// Begins one native frame with an explicit runtime-query page grant.
    ///
    /// The grant belongs to this frame and is consumed at most once across every semantic update
    /// collected before [`Self::end_frame`]. A zero grant keeps runtime-owned continuation work
    /// pending without touching the bound view.
    pub fn begin_frame_with_query_page_budget(
        &mut self,
        ctx: &Context,
        scene_rect: Rect,
        query_page_budget: usize,
    ) -> Result<(), EguiInteractionError> {
        if self.frame.is_some() {
            return Err(EguiInteractionError::FrameAlreadyOpen);
        }
        if !scene_rect.min.is_finite()
            || !scene_rect.max.is_finite()
            || scene_rect.width() <= 0.0
            || scene_rect.height() <= 0.0
        {
            return Err(EguiInteractionError::InvalidSceneRect);
        }
        let (
            pointer_pos,
            primary_pressed,
            primary_released,
            primary_down,
            selection_delete_pressed,
            escape_pressed,
            pointer_gone,
            elapsed_seconds,
        ) = ctx.input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.button_pressed(PointerButton::Primary),
                input.pointer.button_released(PointerButton::Primary),
                input.pointer.button_down(PointerButton::Primary),
                input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace),
                input.key_pressed(Key::Escape),
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, Event::PointerGone)),
                input.stable_dt,
            )
        });
        self.frame = Some(EguiInteractionFrame {
            scene_rect,
            pointer_pos,
            primary_pressed,
            primary_released,
            primary_down,
            selection_delete_pressed,
            selection_delete_handled: false,
            escape_pressed,
            pointer_gone,
            elapsed_seconds,
            tracked_response_seen: false,
            tracked_response_down: false,
            tracked_response_pointer: None,
            renderer_query_pages_remaining: query_page_budget,
            output: EguiInteractionOutput::default(),
        });
        Ok(())
    }

    /// Maps one explicit graph-interactive response to semantic pointer and keyboard input.
    ///
    /// A focused non-native response owns Delete and Backspace while the semantic session is idle.
    /// Text-edit focus and [`CanvasPressTarget::NativeControl`] keep those keys inside the widget.
    pub fn handle_response(
        &mut self,
        response: &Response,
        target: CanvasPressTarget,
    ) -> Result<(), EguiInteractionError> {
        if self.handle_selection_delete_shortcut(response, &target)? {
            return Ok(());
        }
        let frame = self
            .frame
            .as_mut()
            .ok_or(EguiInteractionError::FrameNotOpen)?;
        if self.tracked_response == Some(response.id) {
            frame.tracked_response_seen = true;
            frame.tracked_response_down = response.is_pointer_button_down_on();
            frame.tracked_response_pointer = response.interact_pointer_pos();
        }
        let should_press = self.tracked_response.is_none()
            && frame.primary_pressed
            && response.is_pointer_button_down_on();
        if !should_press {
            return Ok(());
        }

        let scene_rect = frame.scene_rect;
        let pointer = response
            .interact_pointer_pos()
            .or(frame.pointer_pos)
            .ok_or(CanvasInteractionError::InvalidPosition)?;
        let position = interaction_position(&self.view, scene_rect, pointer)?;
        let owner = interaction_owner(&target);
        let native_control = matches!(target, CanvasPressTarget::NativeControl);
        let update = self.session.press(target, position)?;
        let mut output = EguiInteractionOutput::default();
        self.process_update_with_current_budget(update, &mut output);

        if !native_control && self.session.presentation().phase() != CanvasInteractionPhase::Idle {
            response.ctx.set_dragged_id(response.id);
            self.tracked_response = Some(response.id);
            self.tracked_owner = owner;
            self.native_capture = true;
            self.last_position = Some(position);
            let frame = self
                .frame
                .as_mut()
                .expect("the interaction frame remains open while handling a response");
            frame.tracked_response_seen = true;
            frame.tracked_response_down = true;
            frame.tracked_response_pointer = Some(pointer);
        }
        self.frame
            .as_mut()
            .expect("the interaction frame remains open while merging output")
            .output
            .merge(output);
        Ok(())
    }

    fn handle_selection_delete_shortcut(
        &mut self,
        response: &Response,
        target: &CanvasPressTarget,
    ) -> Result<bool, EguiInteractionError> {
        let should_delete = {
            let frame = self
                .frame
                .as_ref()
                .ok_or(EguiInteractionError::FrameNotOpen)?;
            frame.selection_delete_pressed
                && !frame.selection_delete_handled
                && self.session.presentation().phase() == CanvasInteractionPhase::Idle
                && !matches!(target, CanvasPressTarget::NativeControl)
                && response.has_focus()
                && !response.ctx.text_edit_focused()
        };
        if !should_delete {
            return Ok(false);
        }

        self.frame
            .as_mut()
            .expect("the interaction frame remains open while handling keyboard input")
            .selection_delete_handled = true;
        let update = self.session.delete_selection()?;
        let mut output = EguiInteractionOutput::default();
        self.process_update_with_current_budget(update, &mut output);
        self.frame
            .as_mut()
            .expect("the interaction frame remains open while merging keyboard output")
            .output
            .merge(output);
        Ok(true)
    }

    pub fn end_frame(
        &mut self,
        ctx: &Context,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let view_closed = !self.view.is_open();
        let mut frame = self
            .frame
            .take()
            .ok_or(EguiInteractionError::FrameNotOpen)?;
        if view_closed {
            let update = self.session.close();
            collect_terminal_update(update, &mut frame.output);
            self.release_native_ownership(ctx, &mut frame.output);
            self.clear_tracked_response();
            return Ok(self.finish_output(ctx, frame.output));
        }

        if frame.escape_pressed || frame.pointer_gone {
            let update = self.session.cancel()?;
            self.process_frame_update(update, &mut frame);
            self.release_native_ownership(ctx, &mut frame.output);
        } else if self.tracked_response.is_some() && !frame.tracked_response_seen {
            let update = match self.tracked_owner {
                Some(owner) => self.session.owner_retired(owner),
                None => self.session.cancel()?,
            };
            self.process_frame_update(update, &mut frame);
            self.release_native_ownership(ctx, &mut frame.output);
        } else if self.native_capture
            && frame.primary_down
            && (!frame.tracked_response_down
                || ctx
                    .dragged_id()
                    .is_some_and(|id| Some(id) != self.tracked_response))
        {
            let update = self.session.cancel()?;
            self.process_frame_update(update, &mut frame);
            self.release_native_ownership(ctx, &mut frame.output);
        } else if self.native_capture {
            let pointer = frame
                .tracked_response_pointer
                .or(frame.pointer_pos)
                .and_then(|pointer| {
                    interaction_position(&self.view, frame.scene_rect, pointer).ok()
                })
                .or(self.last_position);
            if frame.primary_released {
                let position = pointer.ok_or(CanvasInteractionError::InvalidPosition)?;
                self.last_position = Some(position);
                let update = self.session.release(position)?;
                self.process_frame_update(update, &mut frame);
                self.release_native_ownership(ctx, &mut frame.output);
            } else if frame.primary_down {
                let position = pointer.ok_or(CanvasInteractionError::InvalidPosition)?;
                if self.last_position != Some(position) || !self.has_pending_renderer_query() {
                    self.last_position = Some(position);
                    let update = self.session.move_to(position)?;
                    self.process_frame_update(update, &mut frame);
                }
                let tick = self.session.tick(
                    frame.elapsed_seconds,
                    ViewSize::new(frame.scene_rect.width(), frame.scene_rect.height()),
                )?;
                self.process_frame_update(tick, &mut frame);
            } else {
                let update = self.session.cancel()?;
                self.process_frame_update(update, &mut frame);
                self.release_native_ownership(ctx, &mut frame.output);
            }
        }

        self.process_frame_pending_work(&mut frame);
        self.sync_tracked_response(ctx, &mut frame.output);
        Ok(self.finish_output(ctx, frame.output))
    }

    pub fn cancel(&mut self, ctx: &Context) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let mut output = self.drain_frame_output();
        let update = self.session.cancel()?;
        self.process_update_with_current_budget(update, &mut output);
        self.release_native_ownership(ctx, &mut output);
        self.sync_tracked_response(ctx, &mut output);
        Ok(self.finish_output(ctx, output))
    }

    pub fn owner_retired(
        &mut self,
        ctx: &Context,
        owner: InteractionOwner,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let mut output = self.drain_frame_output();
        let update = self.session.owner_retired(owner);
        self.process_update_with_current_budget(update, &mut output);
        self.sync_tracked_response(ctx, &mut output);
        Ok(self.finish_output(ctx, output))
    }

    pub fn complete_submission(
        &mut self,
        ctx: &Context,
        ticket: InteractionTicketId,
        completion: CommandExecutionCompletion,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let update = self.session.complete_submission(ticket, completion)?;
        self.finish_submission_update(ctx, update)
    }

    pub fn complete_submission_without_command(
        &mut self,
        ctx: &Context,
        ticket: InteractionTicketId,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let update = self.session.complete_submission_without_command(ticket)?;
        self.finish_submission_update(ctx, update)
    }

    pub fn submission_not_dispatched(
        &mut self,
        ctx: &Context,
        ticket: InteractionTicketId,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let update = self.session.submission_not_dispatched(ticket)?;
        self.finish_submission_update(ctx, update)
    }

    pub fn submission_outcome_unknown(
        &mut self,
        ctx: &Context,
        ticket: InteractionTicketId,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let update = self.session.submission_outcome_unknown(ticket)?;
        self.finish_submission_update(ctx, update)
    }

    fn finish_submission_update(
        &mut self,
        ctx: &Context,
        update: CanvasInteractionUpdate,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let mut output = self.drain_frame_output();
        self.process_update_with_current_budget(update, &mut output);
        self.sync_tracked_response(ctx, &mut output);
        Ok(self.finish_output(ctx, output))
    }

    /// Accepts one current graph submission into the application's command transport.
    ///
    /// A successful return is the transport-acceptance linearization point: cancellation,
    /// closure, view retirement, or a successor ticket can revoke the submission before this
    /// call, but not after it. The caller must execute or enqueue the returned command
    /// immediately, then report its completion through [`Self::complete_submission`].
    pub fn bind_submission(
        &mut self,
        submission: InteractionGraphSubmission,
    ) -> Result<Option<GraphCommand>, EguiInteractionError> {
        Ok(self.session.bind_submission(submission)?)
    }

    pub fn search_ready(
        &mut self,
        ctx: &Context,
        request: &ConnectionSearchRequest,
        result_count: usize,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let mut output = self.drain_frame_output();
        let update = self.session.search_ready(request, result_count)?;
        self.process_update_with_current_budget(update, &mut output);
        Ok(self.finish_output(ctx, output))
    }

    /// Creates one sealed search selection through the bound headless interaction session.
    pub fn create_search_selection(
        &self,
        request: &ConnectionSearchRequest,
        target: PortId,
        creation: NodeCreation,
    ) -> Result<Option<ConnectionSearchSelection>, EguiInteractionError> {
        Ok(self
            .session
            .create_search_selection(request, target, creation)?)
    }

    pub fn select_search_result(
        &mut self,
        ctx: &Context,
        selection: ConnectionSearchSelection,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let mut output = self.drain_frame_output();
        let update = self.session.select_search_result(selection)?;
        self.process_update_with_current_budget(update, &mut output);
        Ok(self.finish_output(ctx, output))
    }

    /// Resumes one opaque graph submission after an application-owned command prefix settles.
    pub fn resume_submission_after_prefix(
        &mut self,
        ctx: &Context,
        submission: InteractionGraphSubmission,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        let mut output = self.drain_frame_output();
        let update = self.session.resume_submission_after_prefix(submission)?;
        self.process_update_with_current_budget(update, &mut output);
        Ok(self.finish_output(ctx, output))
    }

    pub fn close(&mut self, ctx: &Context) -> EguiInteractionOutput {
        let mut output = self.drain_frame_output();
        collect_terminal_update(self.session.close(), &mut output);
        self.release_native_ownership(ctx, &mut output);
        self.clear_tracked_response();
        self.finish_output(ctx, output)
    }

    fn process_update(
        &mut self,
        update: CanvasInteractionUpdate,
        output: &mut EguiInteractionOutput,
        page_budget: &mut usize,
    ) {
        self.collect_update(update, output);
        self.process_pending_work(output, page_budget);
    }

    fn process_update_with_current_budget(
        &mut self,
        update: CanvasInteractionUpdate,
        output: &mut EguiInteractionOutput,
    ) {
        let mut page_budget = self.take_current_query_page_budget();
        self.process_update(update, output, &mut page_budget);
        self.restore_current_query_page_budget(page_budget);
    }

    fn process_frame_update(
        &mut self,
        update: CanvasInteractionUpdate,
        frame: &mut EguiInteractionFrame,
    ) {
        self.process_update(
            update,
            &mut frame.output,
            &mut frame.renderer_query_pages_remaining,
        );
    }

    fn process_frame_pending_work(&mut self, frame: &mut EguiInteractionFrame) {
        self.process_pending_work(&mut frame.output, &mut frame.renderer_query_pages_remaining);
    }

    fn process_pending_work(
        &mut self,
        output: &mut EguiInteractionOutput,
        page_budget: &mut usize,
    ) {
        if !self.session.query_work_pending() || *page_budget == 0 {
            output.renderer_work_pending = self.session.query_work_pending();
            return;
        }

        let pages_granted = *page_budget;
        let advance = self.session.advance_query_work(pages_granted);
        let (update, needs_tick, receipt, terminal_error) = advance.into_parts();

        // Query advancement can publish damage and effects even when it terminates with an error.
        // Preserve those facts before recording the terminal failure for the native host.
        self.collect_update(update, output);
        debug_assert_eq!(receipt.pages_granted(), pages_granted);
        *page_budget = (*page_budget).saturating_sub(receipt.pages_attempted());
        output.resolved_query_count = output
            .resolved_query_count
            .saturating_add(receipt.pages_attempted());
        if terminal_error.is_some() {
            output.terminal_query_error = terminal_error;
        }
        output.renderer_work_pending = self.session.query_work_pending();
        debug_assert_eq!(needs_tick, output.renderer_work_pending);
    }

    fn collect_update(
        &mut self,
        update: CanvasInteractionUpdate,
        output: &mut EguiInteractionOutput,
    ) {
        let dispatch = update.into_dispatch();
        merge_damage(&mut output.damage, dispatch.damage);
        if dispatch.tick_demand == InteractionTickDemand::WhileActive {
            output.tick_demand = InteractionTickDemand::WhileActive;
        }
        output.effects.extend(dispatch.effects);
        output.renderer_work_pending = dispatch.query_work_pending;
    }

    fn has_pending_renderer_query(&self) -> bool {
        self.session.query_work_pending()
    }

    fn take_current_query_page_budget(&mut self) -> usize {
        self.frame
            .as_mut()
            .map_or(DEFAULT_QUERY_PAGES_PER_FRAME, |frame| {
                std::mem::take(&mut frame.renderer_query_pages_remaining)
            })
    }

    fn restore_current_query_page_budget(&mut self, page_budget: usize) {
        if let Some(frame) = self.frame.as_mut() {
            frame.renderer_query_pages_remaining = page_budget;
        }
    }

    fn drain_frame_output(&mut self) -> EguiInteractionOutput {
        self.frame
            .as_mut()
            .map_or_else(EguiInteractionOutput::default, |frame| {
                std::mem::take(&mut frame.output)
            })
    }

    fn release_native_ownership(&mut self, ctx: &Context, output: &mut EguiInteractionOutput) {
        if self.native_capture {
            if ctx.dragged_id() == self.tracked_response {
                ctx.stop_dragging();
            }
            self.native_capture = false;
            output.released_native_ownership = true;
        }
    }

    fn sync_tracked_response(&mut self, ctx: &Context, output: &mut EguiInteractionOutput) {
        if matches!(
            self.session.presentation().phase(),
            CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed
        ) {
            self.release_native_ownership(ctx, output);
            self.clear_tracked_response();
        }
    }

    fn clear_tracked_response(&mut self) {
        self.tracked_response = None;
        self.tracked_owner = None;
        self.last_position = None;
    }

    fn finish_output(
        &self,
        ctx: &Context,
        mut output: EguiInteractionOutput,
    ) -> EguiInteractionOutput {
        output.renderer_work_pending = self.session.query_work_pending();
        if output.has_work() {
            ctx.request_repaint();
        }
        output
    }
}

const fn opposite_handle_position(position: HandlePosition) -> HandlePosition {
    match position {
        HandlePosition::Top => HandlePosition::Bottom,
        HandlePosition::Right => HandlePosition::Left,
        HandlePosition::Bottom => HandlePosition::Top,
        HandlePosition::Left => HandlePosition::Right,
    }
}

fn interaction_position(
    view: &WeakViewSession,
    scene_rect: Rect,
    pointer: Pos2,
) -> Result<ViewportPointerSample, EguiInteractionError> {
    let point = ViewPoint::new(pointer.x - scene_rect.min.x, pointer.y - scene_rect.min.y);
    Ok(view.viewport_pointer_sample(point)?)
}

fn interaction_owner(target: &CanvasPressTarget) -> Option<InteractionOwner> {
    match target {
        CanvasPressTarget::NodeDrag(start) => {
            Some(InteractionOwner::Node(start.clicked_lifetime()))
        }
        CanvasPressTarget::Connection(start) => {
            let binding = start.binding();
            Some(InteractionOwner::Port(
                binding
                    .detached_port_instance()
                    .unwrap_or_else(|| binding.fixed_port_instance()),
            ))
        }
        CanvasPressTarget::Edge { instance, .. } => Some(InteractionOwner::Edge(*instance)),
        CanvasPressTarget::NativeControl
        | CanvasPressTarget::Pan
        | CanvasPressTarget::Marquee { .. }
        | CanvasPressTarget::Canvas(_) => None,
    }
}

fn collect_terminal_update(update: CanvasInteractionUpdate, output: &mut EguiInteractionOutput) {
    let dispatch = update.into_dispatch();
    debug_assert!(dispatch.effects.is_empty());
    debug_assert!(!dispatch.query_work_pending);
    merge_damage(&mut output.damage, dispatch.damage);
    if dispatch.tick_demand == InteractionTickDemand::WhileActive {
        output.tick_demand = InteractionTickDemand::WhileActive;
    }
    output.renderer_work_pending = dispatch.query_work_pending;
}

fn merge_damage(target: &mut InteractionDamage, other: InteractionDamage) {
    target.presentation |= other.presentation;
    target.nodes |= other.nodes;
    target.ports |= other.ports;
    target.edges |= other.edges;
    target.viewport |= other.viewport;
    target.selection |= other.selection;
}
