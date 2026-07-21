use std::collections::VecDeque;

use egui::{Context, Event, Id, Key, PointerButton, Pos2, Rect, Response};
use jellyflow_core::{EdgeId, NodeGraphConnectionMode, NodeId, PortId};
use jellyflow_runtime::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasInteractionPresentation, CanvasInteractionQuery,
    CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget, CommandSettlement,
    ConnectionInteractionStart, ConnectionProbeQueryError, ConnectionSearchSelection,
    EdgePickQueryError, InteractionDamage, InteractionGeneration, InteractionOwner,
    InteractionPosition, InteractionTickDemand, InteractionTicketId, SelectionMode, ViewPoint,
    ViewSession, ViewSessionError, ViewSize,
};

/// Renderer-neutral query work resolved by the egui adapter during one transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiInteractionQueryKind {
    AlignmentSnap,
    ConnectionProbe,
    EdgePick,
}

/// Ordered semantic work emitted while translating egui input.
///
/// Selection and viewport effects are applied to the bound [`ViewSession`] before queries run.
/// Search and graph-submission effects remain explicit so the application can connect them to its
/// native search UI and ordered command transport.
#[derive(Debug, Default)]
pub struct EguiInteractionOutput {
    effects: Vec<CanvasInteractionEffect>,
    resolved_queries: Vec<EguiInteractionQueryKind>,
    damage: InteractionDamage,
    tick_demand: InteractionTickDemand,
    released_native_ownership: bool,
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

    pub fn resolved_queries(&self) -> &[EguiInteractionQueryKind] {
        &self.resolved_queries
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

    pub fn has_work(&self) -> bool {
        !self.effects.is_empty()
            || !self.resolved_queries.is_empty()
            || self.damage != InteractionDamage::default()
            || self.tick_demand != InteractionTickDemand::None
            || self.released_native_ownership
    }

    pub fn merge(&mut self, other: Self) {
        self.effects.extend(other.effects);
        self.resolved_queries.extend(other.resolved_queries);
        merge_damage(&mut self.damage, other.damage);
        if other.tick_demand == InteractionTickDemand::WhileActive {
            self.tick_demand = InteractionTickDemand::WhileActive;
        }
        self.released_native_ownership |= other.released_native_ownership;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EguiInteractionError {
    #[error(transparent)]
    Interaction(#[from] CanvasInteractionError),
    #[error(transparent)]
    View(#[from] ViewSessionError),
    #[error(transparent)]
    ConnectionProbe(#[from] ConnectionProbeQueryError),
    #[error(transparent)]
    EdgePick(#[from] EdgePickQueryError),
    #[error("an egui interaction frame is already open")]
    FrameAlreadyOpen,
    #[error("no egui interaction frame is open")]
    FrameNotOpen,
    #[error("the egui scene rectangle is invalid")]
    InvalidSceneRect,
    #[error("the supplied view does not own this egui interaction session")]
    ViewMismatch,
}

#[derive(Debug)]
struct EguiInteractionFrame {
    scene_rect: Rect,
    pointer_pos: Option<Pos2>,
    primary_pressed: bool,
    primary_released: bool,
    primary_down: bool,
    escape_pressed: bool,
    pointer_gone: bool,
    elapsed_seconds: f32,
    tracked_response_seen: bool,
    tracked_response_down: bool,
    tracked_response_pointer: Option<Pos2>,
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
    session: CanvasInteractionSession,
    tracked_response: Option<Id>,
    tracked_owner: Option<InteractionOwner>,
    native_capture: bool,
    last_position: Option<InteractionPosition>,
    frame: Option<EguiInteractionFrame>,
}

impl EguiCanvasInteraction {
    pub fn new(
        view: &ViewSession,
        config: CanvasInteractionConfig,
    ) -> Result<Self, EguiInteractionError> {
        let session = CanvasInteractionSession::new(view.id(), view.generation()?, config)?;
        Ok(Self {
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
        view: &ViewSession,
        node: NodeId,
        selection_mode: SelectionMode,
    ) -> Result<Option<CanvasPressTarget>, EguiInteractionError> {
        self.ensure_view(view)?;
        Ok(view
            .node_drag_start(node, selection_mode)?
            .map(CanvasPressTarget::NodeDrag))
    }

    /// Resolves an explicit port handle from authoritative policy and published geometry.
    pub fn connection_target(
        &self,
        view: &ViewSession,
        port: PortId,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        allow_create_from_wire: bool,
    ) -> Result<Option<CanvasPressTarget>, EguiInteractionError> {
        self.ensure_view(view)?;
        let Some(drag_intent) = view.port_connection_drag_intent(port)? else {
            return Ok(None);
        };
        let Some(intent) =
            ConnectionInteractionStart::intent_from_port_drag(drag_intent, new_edge, mode)
        else {
            return Ok(None);
        };
        let geometry = view.geometry_snapshot()?;
        let Some(fixed) = geometry.port(intent.fixed_port()) else {
            return Ok(None);
        };
        Ok(ConnectionInteractionStart::from_port_drag_intent(
            drag_intent,
            new_edge,
            mode,
            fixed.route_anchor(),
            fixed.outward(),
            allow_create_from_wire,
        )
        .map(CanvasPressTarget::Connection))
    }

    pub fn begin_frame(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
        scene_rect: Rect,
    ) -> Result<(), EguiInteractionError> {
        self.ensure_view(view)?;
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
            escape_pressed,
            pointer_gone,
            elapsed_seconds,
        ) = ctx.input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.button_pressed(PointerButton::Primary),
                input.pointer.button_released(PointerButton::Primary),
                input.pointer.button_down(PointerButton::Primary),
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
            escape_pressed,
            pointer_gone,
            elapsed_seconds,
            tracked_response_seen: false,
            tracked_response_down: false,
            tracked_response_pointer: None,
            output: EguiInteractionOutput::default(),
        });
        Ok(())
    }

    pub fn handle_response(
        &mut self,
        view: &ViewSession,
        response: &Response,
        target: CanvasPressTarget,
    ) -> Result<(), EguiInteractionError> {
        self.ensure_view(view)?;
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
        let position = interaction_position(view, scene_rect, pointer)?;
        let owner = interaction_owner(&target);
        let native_control = matches!(target, CanvasPressTarget::NativeControl);
        let update = self.session.press(target, position)?;
        let mut output = EguiInteractionOutput::default();
        self.process_update(view, update, &mut output)?;

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

    pub fn end_frame(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        if view.id() != self.session.view() {
            return Err(EguiInteractionError::ViewMismatch);
        }
        if view.close_reason().is_none() {
            self.ensure_view(view)?;
        }
        let mut frame = self
            .frame
            .take()
            .ok_or(EguiInteractionError::FrameNotOpen)?;
        if view.close_reason().is_some() {
            let update = self.session.close();
            collect_terminal_update(update, &mut frame.output);
            self.release_native_ownership(ctx, &mut frame.output);
            self.clear_tracked_response();
            return Ok(finish_output(ctx, frame.output));
        }

        if frame.escape_pressed || frame.pointer_gone {
            let update = self.session.cancel();
            self.process_update(view, update, &mut frame.output)?;
            self.release_native_ownership(ctx, &mut frame.output);
        } else if self.tracked_response.is_some() && !frame.tracked_response_seen {
            let update = match self.tracked_owner {
                Some(owner) => self.session.owner_retired(owner),
                None => self.session.cancel(),
            };
            self.process_update(view, update, &mut frame.output)?;
            self.release_native_ownership(ctx, &mut frame.output);
        } else if self.native_capture
            && frame.primary_down
            && (!frame.tracked_response_down
                || ctx
                    .dragged_id()
                    .is_some_and(|id| Some(id) != self.tracked_response))
        {
            let update = self.session.cancel();
            self.process_update(view, update, &mut frame.output)?;
            self.release_native_ownership(ctx, &mut frame.output);
        } else if self.native_capture {
            let pointer = frame
                .tracked_response_pointer
                .or(frame.pointer_pos)
                .and_then(|pointer| interaction_position(view, frame.scene_rect, pointer).ok())
                .or(self.last_position);
            if frame.primary_released {
                let position = pointer.ok_or(CanvasInteractionError::InvalidPosition)?;
                self.last_position = Some(position);
                if self.session.presentation().phase() == CanvasInteractionPhase::Armed {
                    let update = self.session.move_to(position)?;
                    self.process_update(view, update, &mut frame.output)?;
                }
                let update = self.session.release(position)?;
                self.process_update(view, update, &mut frame.output)?;
                self.release_native_ownership(ctx, &mut frame.output);
            } else if frame.primary_down {
                let position = pointer.ok_or(CanvasInteractionError::InvalidPosition)?;
                self.last_position = Some(position);
                let update = self.session.move_to(position)?;
                self.process_update(view, update, &mut frame.output)?;
                let tick = self.session.tick(
                    frame.elapsed_seconds,
                    ViewSize::new(frame.scene_rect.width(), frame.scene_rect.height()),
                    view.viewport_transform()?,
                )?;
                self.process_update(view, tick, &mut frame.output)?;
            } else {
                let update = self.session.cancel();
                self.process_update(view, update, &mut frame.output)?;
                self.release_native_ownership(ctx, &mut frame.output);
            }
        }

        self.sync_tracked_response(ctx, &mut frame.output);
        Ok(finish_output(ctx, frame.output))
    }

    pub fn cancel(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        self.ensure_view(view)?;
        let mut output = self.drain_frame_output();
        let update = self.session.cancel();
        self.process_update(view, update, &mut output)?;
        self.release_native_ownership(ctx, &mut output);
        self.sync_tracked_response(ctx, &mut output);
        Ok(finish_output(ctx, output))
    }

    pub fn owner_retired(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
        owner: InteractionOwner,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        self.ensure_view(view)?;
        let mut output = self.drain_frame_output();
        let update = self.session.owner_retired(owner);
        self.process_update(view, update, &mut output)?;
        self.sync_tracked_response(ctx, &mut output);
        Ok(finish_output(ctx, output))
    }

    pub fn settle(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
        ticket: InteractionTicketId,
        settlement: CommandSettlement,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        self.ensure_view(view)?;
        let mut output = self.drain_frame_output();
        let update = self.session.settle(ticket, settlement)?;
        self.process_update(view, update, &mut output)?;
        self.sync_tracked_response(ctx, &mut output);
        Ok(finish_output(ctx, output))
    }

    pub fn request_connection_commit_revalidation(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
        ticket: InteractionTicketId,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        self.ensure_view(view)?;
        let mut output = self.drain_frame_output();
        let update = self
            .session
            .request_connection_commit_revalidation(ticket)?;
        self.process_update(view, update, &mut output)?;
        Ok(finish_output(ctx, output))
    }

    pub fn search_ready(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
        generation: InteractionGeneration,
        result_count: usize,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        self.ensure_view(view)?;
        let mut output = self.drain_frame_output();
        let update = self.session.search_ready(generation, result_count)?;
        self.process_update(view, update, &mut output)?;
        Ok(finish_output(ctx, output))
    }

    pub fn select_search_result(
        &mut self,
        view: &ViewSession,
        ctx: &Context,
        generation: InteractionGeneration,
        selection: ConnectionSearchSelection,
    ) -> Result<EguiInteractionOutput, EguiInteractionError> {
        self.ensure_view(view)?;
        let mut output = self.drain_frame_output();
        let update = self.session.select_search_result(generation, selection)?;
        self.process_update(view, update, &mut output)?;
        Ok(finish_output(ctx, output))
    }

    pub fn close(&mut self, ctx: &Context) -> EguiInteractionOutput {
        let mut output = self.drain_frame_output();
        collect_terminal_update(self.session.close(), &mut output);
        self.release_native_ownership(ctx, &mut output);
        self.clear_tracked_response();
        finish_output(ctx, output)
    }

    fn process_update(
        &mut self,
        view: &ViewSession,
        update: CanvasInteractionUpdate,
        output: &mut EguiInteractionOutput,
    ) -> Result<(), EguiInteractionError> {
        let mut pending = VecDeque::from([update]);
        while let Some(update) = pending.pop_front() {
            let dispatch = update.into_dispatch();
            merge_damage(&mut output.damage, dispatch.damage);
            if dispatch.tick_demand == InteractionTickDemand::WhileActive {
                output.tick_demand = InteractionTickDemand::WhileActive;
            }
            for effect in dispatch.pre_query_effects {
                match effect {
                    CanvasInteractionEffect::Selection(mutation) => {
                        view.apply_selection_mutation(mutation)?;
                    }
                    CanvasInteractionEffect::Viewport(mutation) => {
                        view.apply_viewport_mutation(mutation)?;
                    }
                    effect @ (CanvasInteractionEffect::Search(_)
                    | CanvasInteractionEffect::SubmitGraph { .. }) => {
                        output.effects.push(effect);
                    }
                }
            }
            for query in dispatch.queries {
                let next = match query {
                    CanvasInteractionQuery::AlignmentSnap { id, query, .. } => {
                        output
                            .resolved_queries
                            .push(EguiInteractionQueryKind::AlignmentSnap);
                        self.session
                            .resolve_alignment_snap(id, view.alignment_snap(query)?)?
                    }
                    CanvasInteractionQuery::ConnectionProbe { id, query, .. } => {
                        output
                            .resolved_queries
                            .push(EguiInteractionQueryKind::ConnectionProbe);
                        match view.connection_probe(query) {
                            Ok(outcome) => self.session.resolve_connection_probe(id, outcome)?,
                            Err(ConnectionProbeQueryError::StaleCursor(reason))
                                if reason.is_restartable() =>
                            {
                                self.session.restart_stale_query(id)?
                            }
                            Err(error) => {
                                let cancelled = self.session.cancel();
                                self.process_update(view, cancelled, output)?;
                                return Err(error.into());
                            }
                        }
                    }
                    CanvasInteractionQuery::EdgePick { id, query } => {
                        output
                            .resolved_queries
                            .push(EguiInteractionQueryKind::EdgePick);
                        match view.edge_pick(query) {
                            Ok(outcome) => self.session.resolve_edge_pick(id, outcome)?,
                            Err(EdgePickQueryError::StaleCursor(reason))
                                if reason.is_restartable() =>
                            {
                                self.session.restart_stale_query(id)?
                            }
                            Err(error) => {
                                let cancelled = self.session.cancel();
                                self.process_update(view, cancelled, output)?;
                                return Err(error.into());
                            }
                        }
                    }
                };
                pending.push_back(next);
            }
        }
        Ok(())
    }

    fn ensure_view(&self, view: &ViewSession) -> Result<(), EguiInteractionError> {
        if view.id() != self.session.view() || view.generation()? != self.session.view_generation()
        {
            return Err(EguiInteractionError::ViewMismatch);
        }
        Ok(())
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
}

fn interaction_position(
    view: &ViewSession,
    scene_rect: Rect,
    pointer: Pos2,
) -> Result<InteractionPosition, EguiInteractionError> {
    let point = ViewPoint::new(pointer.x - scene_rect.min.x, pointer.y - scene_rect.min.y);
    InteractionPosition::from_view(point, view.viewport_transform()?)
        .ok_or(CanvasInteractionError::InvalidPosition.into())
}

fn interaction_owner(target: &CanvasPressTarget) -> Option<InteractionOwner> {
    match target {
        CanvasPressTarget::NodeDrag(start) => Some(InteractionOwner::Node(start.clicked())),
        CanvasPressTarget::Connection(start) => Some(InteractionOwner::Port(
            start
                .intent()
                .detached_port()
                .unwrap_or_else(|| start.intent().fixed_port()),
        )),
        CanvasPressTarget::Edge { edge, .. } => Some(InteractionOwner::Edge(*edge)),
        CanvasPressTarget::NativeControl
        | CanvasPressTarget::Pan
        | CanvasPressTarget::Marquee { .. }
        | CanvasPressTarget::Canvas(_) => None,
    }
}

fn collect_terminal_update(update: CanvasInteractionUpdate, output: &mut EguiInteractionOutput) {
    let dispatch = update.into_dispatch();
    debug_assert!(dispatch.pre_query_effects.is_empty());
    debug_assert!(dispatch.queries.is_empty());
    merge_damage(&mut output.damage, dispatch.damage);
    if dispatch.tick_demand == InteractionTickDemand::WhileActive {
        output.tick_demand = InteractionTickDemand::WhileActive;
    }
}

fn merge_damage(target: &mut InteractionDamage, other: InteractionDamage) {
    target.presentation |= other.presentation;
    target.nodes |= other.nodes;
    target.ports |= other.ports;
    target.edges |= other.edges;
    target.viewport |= other.viewport;
    target.selection |= other.selection;
}

fn finish_output(ctx: &Context, output: EguiInteractionOutput) -> EguiInteractionOutput {
    if output.has_work() {
        ctx.request_repaint();
    }
    output
}
