use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use jellyflow_core::types::TypeDesc;
use jellyflow_core::{GraphBuilder, GraphId, Symbol, SymbolId};
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::graph::GraphRuntime;
use open_gpui::{ScrollDelta, ScrollEvent, ScrollSource, TouchPhase, point};
use open_gpui_canvas_jellyflow::blackboard::{
    NativeBlackboard, blackboard_empty_selector, blackboard_failed_selector,
    blackboard_loading_selector, blackboard_retry_selector, blackboard_scroll_selector,
    blackboard_symbol_apply_selector, blackboard_symbol_name_selector,
};
use open_gpui_canvas_jellyflow::product_domain::blackboard::{
    BlackboardDispatchOutcome, BlackboardSource, BlackboardSourceCallback, BlackboardSourceError,
    BlackboardSourceInvalidation, BlackboardSourceSnapshot, BlackboardSourceSubscription,
    GraphRuntimeBlackboardSource,
};
use open_gpui_ui_components::gpui_adapter::init_text_input;

#[open_gpui::test]
fn loading_blackboard_source_keeps_symbols_unmounted_until_publication(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0007);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b1))
        .with_symbol(symbol_id, symbol("pending"))
        .build()
        .expect("valid loading blackboard graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("loading blackboard runtime"),
    );
    let source = Rc::new(ControlledBlackboardSource::new(runtime, [None], true));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (_blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));

    draw(cx);
    assert_eq!(source.attempts(), 1);
    assert!(cx.debug_bounds(blackboard_loading_selector()).is_some());
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_none()
    );

    source.publish_ready();
    draw(cx);
    assert_eq!(source.attempts(), 1);
    assert!(cx.debug_bounds(blackboard_loading_selector()).is_none());
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_some()
    );
}

#[open_gpui::test]
fn empty_blackboard_source_renders_an_explicit_empty_state(cx: &mut open_gpui::TestAppContext) {
    cx.update(init_text_input);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b2))
        .build()
        .expect("valid empty blackboard graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("empty blackboard runtime"),
    );
    let source = Rc::new(ControlledBlackboardSource::new(runtime, [None], false));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (_blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));

    draw(cx);
    assert_eq!(source.attempts(), 1);
    assert!(cx.debug_bounds(blackboard_empty_selector()).is_some());
    assert!(cx.debug_bounds(blackboard_loading_selector()).is_none());
    assert!(cx.debug_bounds(blackboard_failed_selector()).is_none());
}

#[open_gpui::test]
fn blackboard_scroll_keeps_large_symbol_sets_reachable(cx: &mut open_gpui::TestAppContext) {
    cx.update(init_text_input);
    let symbol_ids = (0..12)
        .map(|index| SymbolId::from_u128(0xb1ac_1000 + index))
        .collect::<Vec<_>>();
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xb1b3));
    for (index, symbol_id) in symbol_ids.iter().copied().enumerate() {
        graph = graph.with_symbol(symbol_id, symbol(&format!("parameter_{index:02}")));
    }
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph.build().expect("valid scrollable blackboard graph"))
            .expect("scrollable blackboard runtime"),
    );
    let panel_runtime = Rc::clone(&runtime);
    let (_blackboard, cx) = cx.add_window_view(move |_, _| runtime_blackboard(panel_runtime));
    draw(cx);

    let first_selector = blackboard_symbol_name_selector(symbol_ids[0]);
    let last_selector = blackboard_symbol_name_selector(*symbol_ids.last().unwrap());
    let first_before = cx
        .debug_bounds(&first_selector)
        .expect("first blackboard symbol before scrolling");
    let last_before = cx
        .debug_bounds(&last_selector)
        .expect("last blackboard symbol before scrolling");
    let scroll = cx
        .debug_bounds(blackboard_scroll_selector())
        .expect("blackboard scroll region");

    cx.simulate_event(ScrollEvent {
        position: scroll.center(),
        delta: ScrollDelta::Lines(point(0.0, -30.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    draw(cx);

    let first_after = cx
        .debug_bounds(&first_selector)
        .expect("first blackboard symbol after scrolling");
    let last_after = cx
        .debug_bounds(&last_selector)
        .expect("last blackboard symbol after scrolling");
    assert!(first_after.origin.y < first_before.origin.y);
    assert!(last_after.origin.y < last_before.origin.y);
    assert!(last_after.is_contained_within(&scroll));
}

#[open_gpui::test]
fn failed_blackboard_source_waits_for_explicit_retry_before_loading_symbols(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0006);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b0))
        .with_symbol(symbol_id, symbol("recovered"))
        .build()
        .expect("valid blackboard recovery graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("blackboard recovery runtime"),
    );
    let source = Rc::new(ControlledBlackboardSource::new(
        Rc::clone(&runtime),
        [Some("symbol service unavailable"), None],
        false,
    ));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (_blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));

    draw(cx);
    assert_eq!(source.attempts(), 1);
    assert!(cx.debug_bounds(blackboard_failed_selector()).is_some());
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_none()
    );

    draw(cx);
    draw(cx);
    assert_eq!(source.attempts(), 1, "draw must not retry a failed source");

    let retry = cx
        .debug_bounds(blackboard_retry_selector())
        .expect("failed source exposes an explicit retry control");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);

    assert_eq!(source.attempts(), 2);
    assert!(cx.debug_bounds(blackboard_failed_selector()).is_none());
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_some()
    );
}

#[open_gpui::test]
fn callback_from_retired_source_attempt_cannot_refresh_its_successor(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0011);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b8))
        .with_symbol(symbol_id, symbol("successor"))
        .build()
        .expect("valid successor source graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("successor source runtime"),
    );
    let source = Rc::new(LateCallbackBlackboardSource::new(runtime));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (_blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));

    draw(cx);
    assert!(cx.debug_bounds(blackboard_failed_selector()).is_some());
    assert_eq!(source.snapshot_calls(), 1);

    let retry = cx
        .debug_bounds(blackboard_retry_selector())
        .expect("failed predecessor exposes retry");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    assert_eq!(source.snapshot_calls(), 2);
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_some()
    );

    source.publish_from_attempt(0);
    draw(cx);
    assert_eq!(
        source.snapshot_calls(),
        2,
        "a predecessor callback must not poll or mutate the active source attempt"
    );
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_some()
    );
}

#[open_gpui::test]
fn native_blackboard_retained_input_commits_to_the_existing_symbol(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0001);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1ac))
        .with_symbol(
            symbol_id,
            Symbol {
                name: "exposure".to_owned(),
                ty: Some(TypeDesc::Float),
                default_value: Some(serde_json::json!(1.0)),
                meta: serde_json::json!({ "group": "material" }),
            },
        )
        .build()
        .expect("valid blackboard graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("blackboard runtime"),
    );
    let panel_runtime = Rc::clone(&runtime);
    let (_blackboard, cx) = cx.add_window_view(move |_, _| runtime_blackboard(panel_runtime));
    draw(cx);

    let name = cx
        .debug_bounds(&blackboard_symbol_name_selector(symbol_id))
        .expect("native symbol name input");
    cx.simulate_click(name.center(), Default::default());
    cx.simulate_keystrokes("end");
    cx.simulate_input("_ev");
    draw(cx);

    let before_apply = runtime.snapshot().expect("draft snapshot");
    assert_eq!(before_apply.revision(), 0);
    assert_eq!(
        before_apply
            .graph()
            .symbol(&symbol_id)
            .expect("draft keeps authoritative symbol")
            .name,
        "exposure"
    );

    let apply = cx
        .debug_bounds(&blackboard_symbol_apply_selector(symbol_id))
        .expect("native symbol apply command");
    cx.simulate_click(apply.center(), Default::default());
    draw(cx);

    let edited = runtime.snapshot().expect("edited snapshot");
    let symbol = edited
        .graph()
        .symbol(&symbol_id)
        .expect("stable symbol identity survives edit");
    assert_eq!(symbol.name, "exposure_ev");
}

#[open_gpui::test]
fn stale_blackboard_draft_requires_an_explicit_reapply_before_overwriting_remote_state(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0002);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1ad))
        .with_symbol(symbol_id, symbol("remote base"))
        .build()
        .expect("valid blackboard conflict graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("blackboard conflict runtime"),
    );
    let panel_runtime = Rc::clone(&runtime);
    let (_blackboard, cx) = cx.add_window_view(move |_, _| runtime_blackboard(panel_runtime));
    draw(cx);

    let name = cx
        .debug_bounds(&blackboard_symbol_name_selector(symbol_id))
        .expect("native symbol name input");
    cx.simulate_click(name.center(), Default::default());
    cx.simulate_keystrokes("end");
    cx.simulate_input(" local");
    let remote_version = runtime
        .snapshot()
        .expect("remote command snapshot")
        .version();
    runtime
        .execute(GraphCommand::symbols(
            remote_version,
            jellyflow_runtime::commands::SymbolEdit::new()
                .update(symbol_id, symbol("remote winner")),
        ))
        .applied_revision()
        .expect("remote symbol edit applies");
    draw(cx);

    let conflicted = runtime.snapshot().expect("conflicted snapshot");
    assert_eq!(conflicted.revision(), 1);
    assert_eq!(
        conflicted.graph().symbol(&symbol_id).unwrap().name,
        "remote winner"
    );

    let reapply = cx
        .debug_bounds(&blackboard_symbol_apply_selector(symbol_id))
        .expect("explicit native reapply command");
    cx.simulate_click(reapply.center(), Default::default());
    draw(cx);
    let reapplied = runtime.snapshot().expect("reapplied snapshot");
    assert_eq!(
        reapplied.graph().symbol(&symbol_id).unwrap().name,
        "remote base local"
    );
}

#[open_gpui::test]
fn coalesced_remove_and_readd_does_not_restore_the_removed_symbol_draft(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let reincarnated = SymbolId::from_u128(0xb1ac_0009);
    let survivor = SymbolId::from_u128(0xb1ac_000a);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b5))
        .with_symbol(reincarnated, symbol("first"))
        .with_symbol(survivor, symbol("second"))
        .build()
        .expect("valid multi-symbol blackboard graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("multi-symbol blackboard runtime"),
    );
    let panel_runtime = Rc::clone(&runtime);
    let (_blackboard, cx) = cx.add_window_view(move |_, _| runtime_blackboard(panel_runtime));
    draw(cx);

    for (id, suffix) in [(reincarnated, " old draft"), (survivor, " survivor draft")] {
        let input = cx
            .debug_bounds(&blackboard_symbol_name_selector(id))
            .expect("multi-symbol name input");
        cx.simulate_click(input.center(), Default::default());
        cx.simulate_keystrokes("end");
        cx.simulate_input(suffix);
        draw(cx);
    }

    let removal_version = runtime.snapshot().expect("removal snapshot").version();
    runtime
        .execute(GraphCommand::symbols(
            removal_version,
            jellyflow_runtime::commands::SymbolEdit::new().remove(reincarnated),
        ))
        .applied_revision()
        .expect("edited symbol removal applies");

    let recreation_version = runtime.snapshot().expect("recreation snapshot").version();
    runtime
        .execute(GraphCommand::symbols(
            recreation_version,
            jellyflow_runtime::commands::SymbolEdit::new().add(reincarnated, symbol("fresh")),
        ))
        .applied_revision()
        .expect("symbol recreation applies");
    draw(cx);

    let reincarnated_input = cx
        .debug_bounds(&blackboard_symbol_name_selector(reincarnated))
        .expect("reincarnated symbol input");
    cx.simulate_click(reincarnated_input.center(), Default::default());
    cx.simulate_keystrokes("end");
    cx.simulate_input(" new edit");
    draw(cx);
    let reincarnated_apply = cx
        .debug_bounds(&blackboard_symbol_apply_selector(reincarnated))
        .expect("reincarnated symbol apply");
    cx.simulate_click(reincarnated_apply.center(), Default::default());
    draw(cx);

    let survivor_apply = cx
        .debug_bounds(&blackboard_symbol_apply_selector(survivor))
        .expect("surviving symbol apply");
    cx.simulate_click(survivor_apply.center(), Default::default());
    draw(cx);

    let snapshot = runtime.snapshot().expect("final multi-symbol snapshot");
    assert_eq!(snapshot.revision(), 4);
    assert_eq!(
        snapshot.graph().symbol(&reincarnated).unwrap().name,
        "fresh new edit",
        "a removed application owner must not restore its old draft"
    );
    assert_eq!(
        snapshot.graph().symbol(&survivor).unwrap().name,
        "second survivor draft",
        "another symbol's owner must survive unrelated identity churn"
    );
}

#[open_gpui::test]
fn closing_blackboard_window_releases_its_source_subscription(cx: &mut open_gpui::TestAppContext) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_000b);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b6))
        .with_symbol(symbol_id, symbol("lifecycle"))
        .build()
        .expect("valid lifecycle blackboard graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("lifecycle blackboard runtime"),
    );
    let source = Rc::new(ControlledBlackboardSource::new(runtime, [None], false));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));
    draw(cx);
    assert!(source.has_subscriber());

    drop(blackboard);
    let window = cx
        .windows()
        .into_iter()
        .next()
        .expect("blackboard test window");
    assert!(cx.simulate_window_close(window));
    assert!(
        !source.has_subscriber(),
        "panel entity teardown must release source callbacks and owned fields"
    );
}

type BlackboardCallbackSlot = Rc<RefCell<Option<BlackboardSourceCallback>>>;

struct LateCallbackBlackboardSource {
    runtime: Rc<GraphRuntime>,
    callbacks: Rc<RefCell<Vec<BlackboardSourceCallback>>>,
    snapshot_calls: Cell<usize>,
}

impl LateCallbackBlackboardSource {
    fn new(runtime: Rc<GraphRuntime>) -> Self {
        Self {
            runtime,
            callbacks: Rc::default(),
            snapshot_calls: Cell::new(0),
        }
    }

    fn snapshot_calls(&self) -> usize {
        self.snapshot_calls.get()
    }

    fn publish_from_attempt(&self, attempt: usize) {
        self.callbacks.borrow()[attempt](BlackboardSourceInvalidation::refresh());
    }
}

impl BlackboardSource for LateCallbackBlackboardSource {
    fn subscribe(
        &self,
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        self.callbacks.borrow_mut().push(callback);
        Ok(BlackboardSourceSubscription::new(|| {}))
    }

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError> {
        let calls = self.snapshot_calls.get().saturating_add(1);
        self.snapshot_calls.set(calls);
        if calls == 1 {
            Err(BlackboardSourceError::new("predecessor source failed"))
        } else {
            self.runtime
                .snapshot()
                .map(BlackboardSourceSnapshot::Ready)
                .map_err(|error| BlackboardSourceError::new(error.to_string()))
        }
    }

    fn execute(&self, command: GraphCommand) -> BlackboardDispatchOutcome {
        BlackboardDispatchOutcome::completed(self.runtime.execute(command))
    }
}

struct ControlledBlackboardSource {
    runtime: Rc<GraphRuntime>,
    subscribe_outcomes: RefCell<VecDeque<Option<&'static str>>>,
    attempts: Cell<usize>,
    loading: Cell<bool>,
    callback: BlackboardCallbackSlot,
}

impl ControlledBlackboardSource {
    fn new(
        runtime: Rc<GraphRuntime>,
        subscribe_outcomes: impl IntoIterator<Item = Option<&'static str>>,
        loading: bool,
    ) -> Self {
        Self {
            runtime,
            subscribe_outcomes: RefCell::new(subscribe_outcomes.into_iter().collect()),
            attempts: Cell::new(0),
            loading: Cell::new(loading),
            callback: Rc::default(),
        }
    }

    fn attempts(&self) -> usize {
        self.attempts.get()
    }

    fn has_subscriber(&self) -> bool {
        self.callback.borrow().is_some()
    }

    fn publish_ready(&self) {
        self.loading.set(false);
        if let Some(callback) = self.callback.borrow().as_ref() {
            callback(BlackboardSourceInvalidation::refresh());
        }
    }
}

impl BlackboardSource for ControlledBlackboardSource {
    fn subscribe(
        &self,
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        self.attempts.set(self.attempts.get().saturating_add(1));
        if let Some(Some(message)) = self.subscribe_outcomes.borrow_mut().pop_front() {
            return Err(BlackboardSourceError::new(message));
        }
        *self.callback.borrow_mut() = Some(callback);
        let callback = Rc::clone(&self.callback);
        Ok(BlackboardSourceSubscription::new(move || {
            callback.borrow_mut().take();
        }))
    }

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError> {
        if self.loading.get() {
            return Ok(BlackboardSourceSnapshot::Loading);
        }
        self.runtime
            .snapshot()
            .map(BlackboardSourceSnapshot::Ready)
            .map_err(|error| BlackboardSourceError::new(error.to_string()))
    }

    fn execute(&self, command: GraphCommand) -> BlackboardDispatchOutcome {
        BlackboardDispatchOutcome::completed(self.runtime.execute(command))
    }
}

fn runtime_blackboard(runtime: Rc<GraphRuntime>) -> NativeBlackboard {
    let source: Rc<dyn BlackboardSource> = Rc::new(GraphRuntimeBlackboardSource::new(runtime));
    NativeBlackboard::new(source)
}

fn symbol(name: &str) -> Symbol {
    Symbol {
        name: name.to_owned(),
        ty: Some(TypeDesc::String),
        default_value: None,
        meta: serde_json::Value::Null,
    }
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
