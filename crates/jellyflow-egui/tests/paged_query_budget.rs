use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Event, Id, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Ui, Vec2};
use egui_kittest::Harness;
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow_egui::{EguiCanvasInteraction, EguiInteractionOutput, EguiNativeFrameHost};
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::geometry::{ViewPoint, ViewRect, ViewSize};
use jellyflow_runtime::graph::{
    GraphRuntime, GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot,
};
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionError, CanvasInteractionPhase, CanvasPressTarget,
    InteractionTickDemand, SelectionBaseline, SelectionMode,
};
use jellyflow_runtime::profile::ApplyPipelineError;
use jellyflow_runtime::view::{
    FrameRequest, RendererStamp, ViewCloseReason, ViewSession, ViewSessionEvent, ViewSessionOptions,
};

const NODE_COUNT: usize = 4;
const PRESS: Pos2 = Pos2::new(40.0, 40.0);
const PARTIAL_DRAG: Pos2 = Pos2::new(115.0, 130.0);
const DRAG: Pos2 = Pos2::new(200.0, 180.0);

struct PagedMarqueeState {
    runtime: Rc<RefCell<Option<GraphRuntime>>>,
    host: EguiNativeFrameHost,
    interaction: Rc<RefCell<EguiCanvasInteraction>>,
    nodes: Vec<NodeId>,
    selection_baseline: SelectionBaseline,
    scene_size: Vec2,
    drop_runtime_on_viewport_change: Rc<Cell<bool>>,
    delay_interaction_close: Rc<Cell<bool>>,
    interaction_context: Rc<RefCell<Option<egui::Context>>>,
    delayed_close_output: Rc<RefCell<Option<EguiInteractionOutput>>>,
    response_visible: bool,
    close_host_during_frame: bool,
    query_page_budget: usize,
    output: Option<EguiInteractionOutput>,
}

impl PagedMarqueeState {
    fn new() -> Self {
        let nodes = (0..NODE_COUNT)
            .map(|index| NodeId::from_u128(index as u128 + 1))
            .collect::<Vec<_>>();
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
        for (index, node) in nodes.iter().copied().enumerate() {
            builder.insert_node(node, test_node(index));
        }
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("paged marquee graph"))
            .expect("paged marquee runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("paged marquee view");
        publish_node_geometry(&view, &nodes, 0.0);
        let selection_baseline = view
            .selection_baseline()
            .expect("paged marquee selection baseline");
        let host = EguiNativeFrameHost::new(view);
        let interaction = Rc::new(RefCell::new(
            EguiCanvasInteraction::new(
                host.view(),
                CanvasInteractionConfig {
                    canvas_drag_threshold_in_view: 1.0,
                    query_work_budget: 1,
                    ..CanvasInteractionConfig::default()
                },
            )
            .expect("paged marquee interaction"),
        ));
        let delay_interaction_close = Rc::new(Cell::new(false));
        let interaction_context = Rc::new(RefCell::new(None));
        let delayed_close_output = Rc::new(RefCell::new(None));
        let runtime = runtime.with_middleware(DelayedInteractionCloseMiddleware {
            trigger: Rc::clone(&delay_interaction_close),
            interaction: Rc::clone(&interaction),
            context: Rc::clone(&interaction_context),
            output: Rc::clone(&delayed_close_output),
        });
        let runtime = Rc::new(RefCell::new(Some(runtime)));
        let drop_runtime_on_viewport_change = Rc::new(Cell::new(false));
        let callback_runtime = Rc::clone(&runtime);
        let callback_enabled = Rc::clone(&drop_runtime_on_viewport_change);
        host.view()
            .subscribe(move |event| {
                if callback_enabled.get()
                    && matches!(event, ViewSessionEvent::ViewportChanged { .. })
                {
                    callback_runtime.borrow_mut().take();
                }
            })
            .expect("subscribe runtime-drop trigger");
        Self {
            runtime,
            host,
            interaction,
            nodes,
            selection_baseline,
            scene_size: Vec2::new(480.0, 320.0),
            drop_runtime_on_viewport_change,
            delay_interaction_close,
            interaction_context,
            delayed_close_output,
            response_visible: true,
            close_host_during_frame: false,
            query_page_budget: 1,
            output: None,
        }
    }
}

struct DelayedInteractionCloseMiddleware {
    trigger: Rc<Cell<bool>>,
    interaction: Rc<RefCell<EguiCanvasInteraction>>,
    context: Rc<RefCell<Option<egui::Context>>>,
    output: Rc<RefCell<Option<EguiInteractionOutput>>>,
}

impl GraphRuntimeMiddleware for DelayedInteractionCloseMiddleware {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        if self.trigger.replace(false) {
            let context = self
                .context
                .borrow()
                .clone()
                .expect("paged marquee frame captured an egui context");
            let output = self.interaction.borrow_mut().close(&context);
            self.output.replace(Some(output));
        }
        Ok(())
    }
}

fn render_paged_marquee(ui: &mut Ui, state: &mut PagedMarqueeState) {
    state.interaction_context.replace(Some(ui.ctx().clone()));
    let query_page_budget = state.query_page_budget;
    let scene_rect = Rect::from_min_size(ui.max_rect().min, state.scene_size);
    state
        .interaction
        .borrow_mut()
        .begin_frame_with_query_page_budget(ui.ctx(), scene_rect, query_page_budget)
        .expect("begin paged marquee frame");
    if state.response_visible {
        let response = ui.interact(
            Rect::from_center_size(PRESS, Vec2::splat(28.0)),
            Id::new("paged-marquee-target"),
            Sense::drag(),
        );
        state
            .interaction
            .borrow_mut()
            .handle_response(
                &response,
                CanvasPressTarget::Marquee {
                    selection_mode: SelectionMode::Replace,
                    baseline: state.selection_baseline.clone(),
                },
            )
            .expect("map paged marquee response");
    }
    if state.close_host_during_frame {
        state
            .host
            .close()
            .expect("close paged marquee host during the frame");
        state.close_host_during_frame = false;
    }
    state.output = Some(
        state
            .interaction
            .borrow_mut()
            .end_frame(ui.ctx())
            .expect("end paged marquee frame"),
    );
}

#[test]
fn zero_page_grant_preserves_runtime_owned_work_for_a_later_frame() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    harness.state_mut().output.take();

    harness.hover_at(PRESS);
    harness.step();
    harness.state_mut().output.take();
    harness.drag_at(PRESS);
    harness.step();
    harness.state_mut().output.take();

    harness.state_mut().query_page_budget = 0;
    harness.hover_at(DRAG);
    harness.step();
    let deferred = harness
        .state_mut()
        .output
        .take()
        .expect("zero-grant drag output");
    assert_eq!(deferred.attempted_query_page_count(), 0);
    assert!(deferred.renderer_work_pending());
    assert!(deferred.terminal_query_error().is_none());
    assert!(!deferred.damage().selection);
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    harness.state_mut().query_page_budget = 1;
    harness.step();
    let advanced = harness
        .state_mut()
        .output
        .take()
        .expect("one-grant continuation output");
    assert_query_page_continues(&advanced);
    assert!(!advanced.damage().selection);
    assert_selection_is_empty_and_marquee_is_active(harness.state());
}

#[test]
fn marquee_query_consumes_one_page_per_frame_and_publishes_only_the_complete_selection() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    harness.state_mut().output.take();

    harness.hover_at(PRESS);
    harness.step();
    harness.state_mut().output.take();
    harness.drag_at(PRESS);
    harness.step();
    harness.state_mut().output.take();
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Armed
    );

    harness.hover_at(DRAG);
    harness.step();
    let moved = harness
        .state_mut()
        .output
        .take()
        .expect("drag frame output");
    assert_query_page_continues(&moved);
    assert!(!moved.damage().selection);
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    harness.event(Event::PointerButton {
        pos: DRAG,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    let released = harness
        .state_mut()
        .output
        .take()
        .expect("release frame output");
    assert_query_page_continues(&released);
    assert!(released.released_native_ownership());
    assert!(!released.damage().selection);
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    let selection_publications =
        usize::from(released.damage().selection) + drain_marquee_queries(&mut harness);
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_eq!(
        harness
            .state()
            .host
            .view()
            .selection()
            .expect("complete marquee selection")
            .nodes(),
        harness.state().nodes
    );
    assert_eq!(selection_publications, 1);
}

#[test]
fn retired_marquee_response_discards_its_pending_page_without_partial_selection() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, DRAG);
    let moved = harness
        .state_mut()
        .output
        .take()
        .expect("drag frame output");
    assert_query_page_continues(&moved);
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    harness.state_mut().response_visible = false;
    harness.step();
    let retired = harness
        .state_mut()
        .output
        .take()
        .expect("retired response output");
    assert_eq!(retired.attempted_query_page_count(), 0);
    assert!(!retired.renderer_work_pending());
    assert!(retired.terminal_query_error().is_none());
    assert!(retired.released_native_ownership());
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert!(
        harness
            .state()
            .host
            .view()
            .selection()
            .expect("retired marquee selection")
            .nodes()
            .is_empty()
    );
}

#[test]
fn newer_marquee_bounds_coalesce_the_old_cursor_and_publish_only_the_latest_selection() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, PARTIAL_DRAG);
    let partial = harness
        .state_mut()
        .output
        .take()
        .expect("partial bounds output");
    assert_query_page_continues(&partial);
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    harness.hover_at(DRAG);
    harness.step();
    let superseding = harness
        .state_mut()
        .output
        .take()
        .expect("superseding bounds output");
    assert_query_page_continues(&superseding);
    assert!(!superseding.damage().selection);
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    assert_eq!(drain_marquee_queries(&mut harness), 1);
    assert_eq!(
        harness
            .state()
            .host
            .view()
            .selection()
            .expect("latest complete selection")
            .nodes(),
        harness.state().nodes
    );
}

#[test]
fn stale_marquee_cursor_restarts_from_the_latest_geometry_without_partial_selection() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, DRAG);
    let first_page = harness
        .state_mut()
        .output
        .take()
        .expect("first marquee page output");
    assert_query_page_continues(&first_page);
    assert!(first_page.renderer_work_pending());
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    let state = harness.state();
    publish_node_geometry(state.host.view(), &state.nodes, 1.0);
    harness.step();
    let restarted = harness
        .state_mut()
        .output
        .take()
        .expect("stale cursor restart output");
    assert_query_page_continues(&restarted);
    assert!(restarted.renderer_work_pending());
    assert!(!restarted.damage().selection);
    assert_selection_is_empty_and_marquee_is_active(harness.state());

    harness.event(Event::PointerButton {
        pos: DRAG,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    let released = harness
        .state_mut()
        .output
        .take()
        .expect("release after stale cursor restart");
    assert_query_page_continues(&released);
    assert!(released.renderer_work_pending());
    assert!(released.released_native_ownership());
    assert!(!released.damage().selection);

    let selection_publications =
        usize::from(released.damage().selection) + drain_marquee_queries(&mut harness);
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_eq!(
        harness
            .state()
            .host
            .view()
            .selection()
            .expect("selection after stale cursor restart")
            .nodes(),
        harness.state().nodes
    );
    assert_eq!(selection_publications, 1);
}

#[test]
fn terminal_query_error_cleans_runtime_owned_continuation_without_an_extra_page() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, DRAG);
    let first_page = harness
        .state_mut()
        .output
        .take()
        .expect("first marquee page output");
    assert_query_page_continues(&first_page);

    let state = harness.state_mut();
    state.drop_runtime_on_viewport_change.set(true);
    state.scene_size = Vec2::new(220.0, 200.0);
    harness.step();
    let terminal = harness
        .state_mut()
        .output
        .take()
        .expect("terminal query output");
    assert_eq!(terminal.attempted_query_page_count(), 1);
    assert!(matches!(
        terminal.terminal_query_error(),
        Some(CanvasInteractionError::ViewUnavailable)
    ));
    assert!(!terminal.renderer_work_pending());
    assert_eq!(terminal.tick_demand(), InteractionTickDemand::None);
    assert!(terminal.released_native_ownership());
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_eq!(
        harness.state().host.view().close_reason(),
        Some(ViewCloseReason::RuntimeDropped)
    );

    harness.step();
    let following = harness
        .state_mut()
        .output
        .take()
        .expect("frame after terminal query cleanup");
    assert_eq!(following.attempted_query_page_count(), 0);
    assert!(following.terminal_query_error().is_none());
    assert!(!following.renderer_work_pending());
    assert_eq!(following.tick_demand(), InteractionTickDemand::None);
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Closed
    );
}

#[test]
fn escape_discards_pending_marquee_pages() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, DRAG);
    let first_page = harness
        .state_mut()
        .output
        .take()
        .expect("first marquee page output");
    assert_query_page_continues(&first_page);

    harness.key_down(Key::Escape);
    harness.step();
    let cancelled = harness
        .state_mut()
        .output
        .take()
        .expect("escape cancellation output");
    assert_eq!(cancelled.attempted_query_page_count(), 0);
    assert!(!cancelled.renderer_work_pending());
    assert!(cancelled.terminal_query_error().is_none());
    assert!(cancelled.released_native_ownership());
    assert_marquee_cancelled_without_selection(harness.state());

    harness.step();
    let following = harness
        .state_mut()
        .output
        .take()
        .expect("frame after escape cancellation");
    assert_eq!(following.attempted_query_page_count(), 0);
    assert!(!following.renderer_work_pending());
}

#[test]
fn explicit_cancel_discards_pending_marquee_pages() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, DRAG);
    let first_page = harness
        .state_mut()
        .output
        .take()
        .expect("first marquee page output");
    assert_query_page_continues(&first_page);

    let ctx = harness.ctx.clone();
    let state = harness.state_mut();
    let cancelled = state
        .interaction
        .borrow_mut()
        .cancel(&ctx)
        .expect("explicitly cancel paged marquee");
    assert_eq!(cancelled.attempted_query_page_count(), 0);
    assert!(!cancelled.renderer_work_pending());
    assert!(cancelled.terminal_query_error().is_none());
    assert!(cancelled.released_native_ownership());
    assert_marquee_cancelled_without_selection(harness.state());

    harness.step();
    let following = harness
        .state_mut()
        .output
        .take()
        .expect("frame after explicit cancellation");
    assert_eq!(following.attempted_query_page_count(), 0);
    assert!(!following.renderer_work_pending());
}

#[test]
fn host_close_during_frame_discards_pending_marquee_pages() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, DRAG);
    let first_page = harness
        .state_mut()
        .output
        .take()
        .expect("first marquee page output");
    assert_query_page_continues(&first_page);

    harness.state_mut().close_host_during_frame = true;
    harness.step();
    let closed = harness
        .state_mut()
        .output
        .take()
        .expect("host-close output");
    assert_eq!(closed.attempted_query_page_count(), 0);
    assert!(!closed.renderer_work_pending());
    assert!(closed.terminal_query_error().is_none());
    assert!(closed.released_native_ownership());
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Closed
    );
    assert!(harness.state().host.view().close_reason().is_some());
}

#[test]
fn closed_frame_finishes_a_busy_delayed_close_with_its_page_budget_exactly_once() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .build_ui_state(render_paged_marquee, PagedMarqueeState::new());
    begin_marquee_drag(&mut harness, DRAG);
    let first_page = harness
        .state_mut()
        .output
        .take()
        .expect("first marquee page output");
    assert_query_page_continues(&first_page);
    assert_eq!(drain_marquee_queries(&mut harness), 1);
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Marquee,
    );
    assert_eq!(
        harness
            .state()
            .host
            .view()
            .selection()
            .expect("temporary marquee selection")
            .nodes(),
        harness.state().nodes,
    );

    let state = harness.state_mut();
    state.delay_interaction_close.set(true);
    let version = state
        .runtime
        .borrow()
        .as_ref()
        .expect("paged marquee runtime remains available")
        .snapshot()
        .expect("graph before delayed close")
        .version();
    state
        .runtime
        .borrow()
        .as_ref()
        .expect("paged marquee runtime remains available")
        .execute(GraphCommand::set_node_data(
            version,
            state.nodes[0],
            serde_json::json!({ "trigger": "delayed-close" }),
        ))
        .into_result()
        .expect("middleware trigger command applies");
    let delayed = state
        .delayed_close_output
        .borrow_mut()
        .take()
        .expect("middleware observes delayed interaction close");
    assert_eq!(delayed.attempted_query_page_count(), 0);
    assert!(delayed.renderer_work_pending());
    assert!(delayed.released_native_ownership());
    assert!(!delayed.damage().selection);
    assert!(
        state
            .interaction_context
            .borrow()
            .as_ref()
            .expect("paged marquee context")
            .dragged_id()
            .is_none(),
    );
    assert_eq!(
        state.interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Marquee,
    );

    state.host.close().expect("close delayed marquee host");
    assert_eq!(
        state.host.view().close_reason(),
        Some(ViewCloseReason::HostClosed),
    );
    state.query_page_budget = 1;

    harness.step();
    let cleaned = harness
        .state_mut()
        .output
        .take()
        .expect("closed frame cleanup output");
    assert_eq!(cleaned.attempted_query_page_count(), 0);
    assert!(matches!(
        cleaned.terminal_query_error(),
        Some(CanvasInteractionError::ViewUnavailable)
    ));
    assert!(!cleaned.renderer_work_pending());
    assert_eq!(cleaned.tick_demand(), InteractionTickDemand::None);
    assert_eq!(
        harness.state().interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Closed,
    );

    harness.step();
    let following = harness
        .state_mut()
        .output
        .take()
        .expect("frame after delayed close cleanup");
    assert_eq!(following.attempted_query_page_count(), 0);
    assert!(following.terminal_query_error().is_none());
    assert!(!following.renderer_work_pending());
    assert!(!following.damage().selection);
    assert_eq!(
        usize::from(delayed.terminal_query_error().is_some())
            + usize::from(cleaned.terminal_query_error().is_some())
            + usize::from(following.terminal_query_error().is_some()),
        1,
        "the deferred selection cleanup must report its terminal failure exactly once",
    );
}

fn begin_marquee_drag(harness: &mut Harness<'_, PagedMarqueeState>, destination: Pos2) {
    harness.state_mut().output.take();
    harness.hover_at(PRESS);
    harness.step();
    harness.state_mut().output.take();
    harness.drag_at(PRESS);
    harness.step();
    harness.state_mut().output.take();
    harness.hover_at(destination);
    harness.step();
}

fn assert_query_page_continues(output: &EguiInteractionOutput) {
    assert_eq!(
        output.query_advances().len(),
        1,
        "each adapter frame must retain the exact runtime query advance"
    );
    assert_eq!(output.query_advances()[0].pages_granted(), 1);
    assert_eq!(output.query_advances()[0].pages_attempted(), 1);
    assert_eq!(
        output.attempted_query_page_count(),
        1,
        "the adapter must grant at most one runtime-owned query page per frame"
    );
    assert!(output.terminal_query_error().is_none());
    assert!(
        output.renderer_work_pending(),
        "the one-page renderer budget must leave the marquee continuation pending"
    );
}

fn drain_marquee_queries(harness: &mut Harness<'_, PagedMarqueeState>) -> usize {
    for _ in 0..NODE_COUNT * 3 {
        harness.step();
        let output = harness
            .state_mut()
            .output
            .take()
            .expect("marquee continuation frame output");
        assert!(
            output.attempted_query_page_count() <= 1,
            "the adapter may grant at most one runtime-owned query page per frame"
        );
        assert!(output.terminal_query_error().is_none());
        if output.renderer_work_pending() {
            assert_query_page_continues(&output);
            assert!(!output.damage().selection);
            assert_selection_is_empty_and_marquee_is_active(harness.state());
            continue;
        }
        assert_eq!(output.attempted_query_page_count(), 1);
        assert_eq!(output.query_advances().len(), 1);
        assert!(!output.renderer_work_pending());
        assert!(output.damage().selection);
        return usize::from(output.damage().selection);
    }
    panic!("marquee query did not complete within the bounded frame budget");
}

fn assert_selection_is_empty_and_marquee_is_active(state: &PagedMarqueeState) {
    assert!(
        state
            .host
            .view()
            .selection()
            .expect("intermediate marquee selection")
            .nodes()
            .is_empty()
    );
    assert_eq!(
        state.interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Marquee
    );
}

fn assert_marquee_cancelled_without_selection(state: &PagedMarqueeState) {
    assert_eq!(
        state.interaction.borrow().presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert!(
        state
            .host
            .view()
            .selection()
            .expect("selection after marquee cancellation")
            .nodes()
            .is_empty()
    );
}

fn publish_node_geometry(view: &ViewSession, nodes: &[NodeId], x_offset: f32) {
    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 480.0,
                    height: 320.0,
                },
            },
            RendererStamp::new("paged.marquee", 1),
        ))
        .expect("paged marquee frame");
    let collector = frame.collector();
    for (index, node) in nodes.iter().copied().enumerate() {
        let frame_node = frame
            .nodes()
            .iter()
            .find(|candidate| candidate.id() == node)
            .expect("paged marquee frame node");
        let origin = ViewPoint::new(80.0 + index as f32 * 20.0 + x_offset, 80.0);
        let layout = frame_node.begin_layout(ViewRect::new(origin, ViewSize::new(12.0, 12.0)));
        collector
            .layout(layout.finish().expect("paged marquee node layout"))
            .expect("collect paged marquee node layout");
    }
    view.submit_feedback(collector.finish().expect("paged marquee feedback"))
        .expect("publish paged marquee geometry");
}

fn test_node(index: usize) -> Node {
    Node {
        kind: NodeKindKey::new("paged.marquee"),
        kind_version: 1,
        pos: CanvasPoint {
            x: 80.0 + index as f32 * 20.0,
            y: 80.0,
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
            width: 12.0,
            height: 12.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}
