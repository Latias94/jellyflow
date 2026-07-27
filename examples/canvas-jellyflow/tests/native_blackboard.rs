use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use jellyflow_core::types::TypeDesc;
use jellyflow_core::{GraphBuilder, GraphId, Symbol, SymbolId};
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::graph::GraphRuntime;
use open_gpui::{AppContext, ScrollDelta, ScrollWheelEvent, TouchPhase, point};
use open_gpui_canvas_jellyflow::blackboard::{
    BlackboardCommandError, BlackboardResourceStatus, BlackboardSource, BlackboardSourceCallback,
    BlackboardSourceError, BlackboardSourceInvalidation, BlackboardSourceSnapshot,
    BlackboardSourceSubscription, GraphRuntimeBlackboardSource, NativeBlackboard,
    blackboard_empty_selector, blackboard_failed_selector, blackboard_loading_selector,
    blackboard_retry_selector, blackboard_scroll_selector,
    blackboard_symbol_accept_remote_selector, blackboard_symbol_apply_selector,
    blackboard_symbol_name_selector,
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

    cx.simulate_event(ScrollWheelEvent {
        position: scroll.center(),
        delta: ScrollDelta::Lines(point(0.0, -30.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
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
fn blackboard_snapshot_panic_becomes_an_explicit_retryable_failure(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0008);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b4))
        .with_symbol(symbol_id, symbol("recovered after snapshot panic"))
        .build()
        .expect("valid snapshot panic recovery graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("snapshot panic recovery runtime"),
    );
    let source = Rc::new(FaultInjectingBlackboardSource::new(
        runtime,
        [
            FaultySnapshot::Ready,
            FaultySnapshot::Panic,
            FaultySnapshot::Ready,
        ],
        0,
    ));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));

    draw(cx);
    assert_eq!(source.attempts(), 1);
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_some()
    );

    source.publish_refresh();
    draw(cx);

    assert!(cx.debug_bounds(blackboard_failed_selector()).is_some());
    cx.read_entity(&blackboard, |blackboard, _| {
        let BlackboardResourceStatus::Failed(error) = blackboard.resource_status() else {
            panic!("snapshot panic must publish a failed resource state");
        };
        assert_eq!(error.message(), "blackboard source snapshot panicked");
    });
    assert!(!source.has_subscriber());
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_none()
    );
    draw(cx);
    assert_eq!(source.attempts(), 1, "failure must wait for explicit retry");

    let retry = cx
        .debug_bounds(blackboard_retry_selector())
        .expect("snapshot panic exposes retry control");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);

    assert_eq!(source.attempts(), 2);
    assert!(source.has_subscriber());
    assert!(cx.debug_bounds(blackboard_failed_selector()).is_none());
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_some()
    );
}

#[open_gpui::test]
fn blackboard_subscription_cancel_panic_preserves_failure_and_retry(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0009);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b5))
        .with_symbol(symbol_id, symbol("recovered after cancel panic"))
        .build()
        .expect("valid cancel panic recovery graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("cancel panic recovery runtime"),
    );
    let source = Rc::new(FaultInjectingBlackboardSource::new(
        runtime,
        [
            FaultySnapshot::Error("source failed before cancellation"),
            FaultySnapshot::Ready,
        ],
        1,
    ));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));

    draw(cx);

    assert_eq!(source.attempts(), 1);
    assert!(cx.debug_bounds(blackboard_failed_selector()).is_some());
    cx.read_entity(&blackboard, |blackboard, _| {
        let BlackboardResourceStatus::Failed(error) = blackboard.resource_status() else {
            panic!("cancel panic must preserve a failed resource state");
        };
        assert_eq!(
            error.message(),
            "source failed before cancellation; blackboard source subscription cancellation panicked"
        );
    });
    assert!(!source.has_subscriber());
    assert!(
        cx.debug_bounds(&blackboard_symbol_name_selector(symbol_id))
            .is_none()
    );

    let retry = cx
        .debug_bounds(blackboard_retry_selector())
        .expect("cancel panic preserves retry control");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);

    assert_eq!(source.attempts(), 2);
    assert!(source.has_subscriber());
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
        .execute(GraphCommand::undo(edited.version()))
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
        .execute(GraphCommand::undo(reapplied.version()))
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
    cx.simulate_keystrokes("end");
    cx.simulate_input(" local");
    let unrelated_version = runtime
        .snapshot()
        .expect("unrelated command snapshot")
        .version();
    runtime
        .execute(GraphCommand::symbols(
            unrelated_version,
            jellyflow_runtime::commands::SymbolEdit::new()
                .update(unrelated, symbol("changed elsewhere")),
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
    cx.simulate_keystrokes("end");
    cx.simulate_input(" local");
    let remote_version = runtime
        .snapshot()
        .expect("remote command snapshot")
        .version();
    runtime
        .execute(GraphCommand::symbols(
            remote_version,
            jellyflow_runtime::commands::SymbolEdit::new().update(symbol_id, symbol("remote")),
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

#[open_gpui::test]
fn uncertain_blackboard_command_stays_single_flight_until_authoritative_refresh(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let symbol_id = SymbolId::from_u128(0xb1ac_0008);
    let graph = GraphBuilder::new(GraphId::from_u128(0xb1b4))
        .with_symbol(symbol_id, symbol("base"))
        .build()
        .expect("valid uncertain blackboard graph");
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("uncertain blackboard runtime"),
    );
    let source = Rc::new(UncertainCommitBlackboardSource::new(Rc::clone(&runtime)));
    let panel_source: Rc<dyn BlackboardSource> = source.clone();
    let (_blackboard, cx) = cx.add_window_view(move |_, _| NativeBlackboard::new(panel_source));
    draw(cx);

    let name = cx
        .debug_bounds(&blackboard_symbol_name_selector(symbol_id))
        .expect("uncertain symbol name input");
    cx.simulate_click(name.center(), Default::default());
    cx.simulate_keystrokes("end");
    cx.simulate_input(" local");
    draw(cx);

    let apply_selector = blackboard_symbol_apply_selector(symbol_id);
    let apply = cx
        .debug_bounds(&apply_selector)
        .expect("uncertain symbol apply command");
    cx.simulate_click(apply.center(), Default::default());
    draw(cx);

    assert_eq!(source.execute_calls(), 1);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .symbol(&symbol_id)
            .unwrap()
            .name,
        "base local"
    );

    let blocked_apply = cx
        .debug_bounds(&apply_selector)
        .expect("uncertain attempt keeps its apply control mounted");
    cx.simulate_click(blocked_apply.center(), Default::default());
    draw(cx);
    assert_eq!(
        source.execute_calls(),
        1,
        "an uncertain command must not be dispatched a second time"
    );

    source.publish_latest();
    draw(cx);
    let acknowledged_apply = cx
        .debug_bounds(&apply_selector)
        .expect("acknowledged symbol apply control");
    cx.simulate_click(acknowledged_apply.center(), Default::default());
    draw(cx);
    assert_eq!(source.execute_calls(), 1);
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
        .expect("remove edited symbol")
        .applied()
        .expect("edited symbol removal applies");

    let recreation_version = runtime.snapshot().expect("recreation snapshot").version();
    runtime
        .execute(GraphCommand::symbols(
            recreation_version,
            jellyflow_runtime::commands::SymbolEdit::new().add(reincarnated, symbol("fresh")),
        ))
        .expect("recreate symbol identity")
        .applied()
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

    fn execute(
        &self,
        command: GraphCommand,
    ) -> Result<jellyflow_runtime::commands::GraphCommandOutcome, BlackboardCommandError> {
        self.runtime
            .execute(command)
            .map_err(|error| BlackboardCommandError::new(error.to_string()))
    }
}

enum FaultySnapshot {
    Ready,
    Error(&'static str),
    Panic,
}

struct FaultInjectingBlackboardSource {
    runtime: Rc<GraphRuntime>,
    snapshot_outcomes: RefCell<VecDeque<FaultySnapshot>>,
    attempts: Cell<usize>,
    callback: BlackboardCallbackSlot,
    cancel_panics_remaining: Rc<Cell<usize>>,
}

impl FaultInjectingBlackboardSource {
    fn new(
        runtime: Rc<GraphRuntime>,
        snapshot_outcomes: impl IntoIterator<Item = FaultySnapshot>,
        cancel_panics: usize,
    ) -> Self {
        Self {
            runtime,
            snapshot_outcomes: RefCell::new(snapshot_outcomes.into_iter().collect()),
            attempts: Cell::new(0),
            callback: Rc::default(),
            cancel_panics_remaining: Rc::new(Cell::new(cancel_panics)),
        }
    }

    fn attempts(&self) -> usize {
        self.attempts.get()
    }

    fn has_subscriber(&self) -> bool {
        self.callback.borrow().is_some()
    }

    fn publish_refresh(&self) {
        let callback = self.callback.borrow().clone();
        if let Some(callback) = callback {
            callback(BlackboardSourceInvalidation::refresh());
        }
    }
}

impl BlackboardSource for FaultInjectingBlackboardSource {
    fn subscribe(
        &self,
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        self.attempts.set(self.attempts.get().saturating_add(1));
        *self.callback.borrow_mut() = Some(callback);
        let callback = Rc::clone(&self.callback);
        let cancel_panics_remaining = Rc::clone(&self.cancel_panics_remaining);
        Ok(BlackboardSourceSubscription::new(move || {
            callback.borrow_mut().take();
            let remaining = cancel_panics_remaining.get();
            if remaining > 0 {
                cancel_panics_remaining.set(remaining - 1);
                panic!("injected blackboard subscription cancellation panic");
            }
        }))
    }

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError> {
        match self
            .snapshot_outcomes
            .borrow_mut()
            .pop_front()
            .unwrap_or(FaultySnapshot::Ready)
        {
            FaultySnapshot::Ready => self
                .runtime
                .snapshot()
                .map(BlackboardSourceSnapshot::Ready)
                .map_err(|error| BlackboardSourceError::new(error.to_string())),
            FaultySnapshot::Error(message) => Err(BlackboardSourceError::new(message)),
            FaultySnapshot::Panic => panic!("injected blackboard snapshot panic"),
        }
    }

    fn execute(
        &self,
        command: GraphCommand,
    ) -> Result<jellyflow_runtime::commands::GraphCommandOutcome, BlackboardCommandError> {
        self.runtime
            .execute(command)
            .map_err(|error| BlackboardCommandError::new(error.to_string()))
    }
}

struct UncertainCommitBlackboardSource {
    runtime: Rc<GraphRuntime>,
    visible: RefCell<jellyflow_runtime::graph::GraphSnapshot>,
    callback: BlackboardCallbackSlot,
    execute_calls: Cell<usize>,
}

impl UncertainCommitBlackboardSource {
    fn new(runtime: Rc<GraphRuntime>) -> Self {
        let visible = runtime
            .snapshot()
            .expect("initial uncertain source snapshot");
        Self {
            runtime,
            visible: RefCell::new(visible),
            callback: Rc::default(),
            execute_calls: Cell::new(0),
        }
    }

    fn execute_calls(&self) -> usize {
        self.execute_calls.get()
    }

    fn publish_latest(&self) {
        *self.visible.borrow_mut() = self
            .runtime
            .snapshot()
            .expect("latest uncertain source snapshot");
        if let Some(callback) = self.callback.borrow().as_ref() {
            callback(BlackboardSourceInvalidation::refresh());
        }
    }
}

impl BlackboardSource for UncertainCommitBlackboardSource {
    fn subscribe(
        &self,
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        *self.callback.borrow_mut() = Some(callback);
        let callback = Rc::clone(&self.callback);
        Ok(BlackboardSourceSubscription::new(move || {
            callback.borrow_mut().take();
        }))
    }

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError> {
        Ok(BlackboardSourceSnapshot::Ready(
            self.visible.borrow().clone(),
        ))
    }

    fn execute(
        &self,
        command: GraphCommand,
    ) -> Result<jellyflow_runtime::commands::GraphCommandOutcome, BlackboardCommandError> {
        self.execute_calls
            .set(self.execute_calls.get().saturating_add(1));
        self.runtime
            .execute(command)
            .map_err(|error| BlackboardCommandError::new(error.to_string()))?;
        Err(BlackboardCommandError::new(
            "transport failed after dispatch",
        ))
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
