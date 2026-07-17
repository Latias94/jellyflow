use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use jellyflow::core::types::TypeDesc;
use jellyflow::core::{GraphBuilder, GraphId, Symbol, SymbolId};
use jellyflow::runtime::{GraphCommand, GraphRuntime};
use open_gpui_canvas_jellyflow::blackboard::{
    BlackboardCommandError, BlackboardSource, BlackboardSourceError, BlackboardSourceSnapshot,
    BlackboardSourceSubscription, GraphRuntimeBlackboardSource, NativeBlackboard,
    blackboard_empty_selector, blackboard_failed_selector, blackboard_loading_selector,
    blackboard_retry_selector, blackboard_symbol_accept_remote_selector,
    blackboard_symbol_apply_selector, blackboard_symbol_name_selector,
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
fn native_blackboard_edits_graph_symbols_with_stable_identity_and_exact_undo(
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
    assert_eq!(edited.revision(), 1);
    let symbol = edited
        .graph()
        .symbol(&symbol_id)
        .expect("stable symbol identity survives edit");
    assert_eq!(symbol.name, "exposure_ev");
    assert_eq!(symbol.ty, Some(TypeDesc::Float));
    assert_eq!(symbol.default_value, Some(serde_json::json!(1.0)));
    assert_eq!(symbol.meta, serde_json::json!({ "group": "material" }));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    runtime
        .execute(GraphCommand::undo(edited.revision()))
        .expect("undo symbol edit")
        .applied()
        .expect("undo applies");
    let undone = runtime.snapshot().expect("undone snapshot");
    assert_eq!(
        undone
            .graph()
            .symbol(&symbol_id)
            .expect("undo preserves symbol identity")
            .name,
        "exposure"
    );
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
    cx.simulate_input(" local");
    runtime
        .execute(GraphCommand::symbols(
            0,
            jellyflow::runtime::SymbolEdit::new().update(symbol_id, symbol("remote winner")),
        ))
        .expect("remote symbol command")
        .applied()
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
    assert_eq!(reapplied.revision(), 2);
    assert_eq!(
        reapplied.graph().symbol(&symbol_id).unwrap().name,
        "remote base local"
    );

    runtime
        .execute(GraphCommand::undo(reapplied.revision()))
        .expect("undo explicit reapply")
        .applied()
        .expect("undo applies");
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .symbol(&symbol_id)
            .unwrap()
            .name,
        "remote winner"
    );
}

#[open_gpui::test]
fn unrelated_graph_revision_advances_blackboard_draft_without_a_false_conflict(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let edited = SymbolId::from_u128(0xb1ac_0003);
    let unrelated = SymbolId::from_u128(0xb1ac_0004);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1ae))
        .with_symbol(edited, symbol("editable"))
        .with_symbol(unrelated, symbol("unrelated"))
        .build()
        .expect("valid blackboard base-advance graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("blackboard base-advance runtime"),
    );
    let panel_runtime = Rc::clone(&runtime);
    let (_blackboard, cx) = cx.add_window_view(move |_, _| runtime_blackboard(panel_runtime));
    draw(cx);

    let name = cx
        .debug_bounds(&blackboard_symbol_name_selector(edited))
        .expect("editable symbol input");
    cx.simulate_click(name.center(), Default::default());
    cx.simulate_input(" local");
    runtime
        .execute(GraphCommand::symbols(
            0,
            jellyflow::runtime::SymbolEdit::new().update(unrelated, symbol("changed elsewhere")),
        ))
        .expect("unrelated symbol command")
        .applied()
        .expect("unrelated symbol edit applies");
    draw(cx);

    let apply = cx
        .debug_bounds(&blackboard_symbol_apply_selector(edited))
        .expect("base-advanced apply command");
    cx.simulate_click(apply.center(), Default::default());
    draw(cx);

    let snapshot = runtime.snapshot().expect("base-advanced snapshot");
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(
        snapshot.graph().symbol(&edited).unwrap().name,
        "editable local"
    );
    assert_eq!(
        snapshot.graph().symbol(&unrelated).unwrap().name,
        "changed elsewhere"
    );
}

#[open_gpui::test]
fn accepting_remote_blackboard_value_clears_conflict_without_writing_the_graph(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0005);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1af))
        .with_symbol(symbol_id, symbol("base"))
        .build()
        .expect("valid blackboard accept-remote graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("blackboard accept-remote runtime"),
    );
    let panel_runtime = Rc::clone(&runtime);
    let (_blackboard, cx) = cx.add_window_view(move |_, _| runtime_blackboard(panel_runtime));
    draw(cx);

    let name = cx
        .debug_bounds(&blackboard_symbol_name_selector(symbol_id))
        .expect("native symbol name input");
    cx.simulate_click(name.center(), Default::default());
    cx.simulate_input(" local");
    runtime
        .execute(GraphCommand::symbols(
            0,
            jellyflow::runtime::SymbolEdit::new().update(symbol_id, symbol("remote")),
        ))
        .expect("remote symbol command")
        .applied()
        .expect("remote symbol edit applies");
    draw(cx);

    let accept = cx
        .debug_bounds(&blackboard_symbol_accept_remote_selector(symbol_id))
        .expect("accept remote command");
    cx.simulate_click(accept.center(), Default::default());
    draw(cx);

    let snapshot = runtime.snapshot().expect("accepted remote snapshot");
    assert_eq!(snapshot.revision(), 1);
    assert_eq!(snapshot.graph().symbol(&symbol_id).unwrap().name, "remote");
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
}

type BlackboardCallbackSlot = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

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

    fn publish_ready(&self) {
        self.loading.set(false);
        if let Some(callback) = self.callback.borrow().as_ref() {
            callback();
        }
    }
}

impl BlackboardSource for ControlledBlackboardSource {
    fn subscribe(
        &self,
        callback: Rc<dyn Fn()>,
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

    fn execute(
        &self,
        command: GraphCommand,
    ) -> Result<jellyflow::runtime::GraphCommandOutcome, BlackboardCommandError> {
        self.runtime
            .execute(command)
            .map_err(|error| BlackboardCommandError::new(error.to_string()))
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
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}
