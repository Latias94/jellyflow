use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::io::{ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed};
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::{
    GraphRuntime, GraphRuntimeError, GraphRuntimeEvent, RuntimePatch, RuntimePatchKind,
};
use crate::runtime::view_session::{ViewCloseReason, ViewSession, ViewSessionEvent};
use crate::schema::NodeCatalog;

use super::fixtures::{graph_with_nodes, view_options};

struct PanicIfDropped;

impl Drop for PanicIfDropped {
    fn drop(&mut self) {
        panic!("panic payload must not be dropped during unwind");
    }
}

#[test]
fn subscriber_panics_do_not_truncate_committed_patch_fanout() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    view.seed_all_node_facts_for_test();

    runtime
        .subscribe(|_| panic!("first runtime observer panic"))
        .unwrap();
    let later_runtime_calls = Rc::new(Cell::new(0));
    let callback_calls = later_runtime_calls.clone();
    runtime
        .subscribe(move |event| {
            if matches!(event, GraphRuntimeEvent::Patched { .. }) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .unwrap();
    view.subscribe(|event| {
        if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
            panic!("first view observer panic");
        }
    })
    .unwrap();
    let later_view_calls = Rc::new(Cell::new(0));
    let callback_calls = later_view_calls.clone();
    view.subscribe(move |event| {
        if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
            callback_calls.set(callback_calls.get() + 1);
        }
    })
    .unwrap();
    let node_calls = Rc::new(Cell::new(0));
    let callback_calls = node_calls.clone();
    view.subscribe_node(ids[0], move |_| {
        callback_calls.set(callback_calls.get() + 1);
    })
    .unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
    }))
    .expect_err("the first subscriber panic remains observable");

    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"first runtime observer panic")
    );
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(later_runtime_calls.get(), 1);
    assert_eq!(later_view_calls.get(), 1);
    assert_eq!(node_calls.get(), 1);
    assert_eq!(view.stats().unwrap().runtime_patches, 1);
}

#[test]
fn terminal_subscriber_panic_still_notifies_and_revokes_every_subscription() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    view.subscribe(|event| {
        if matches!(event, ViewSessionEvent::Closed { .. }) {
            panic!("first terminal observer panic");
        }
    })
    .unwrap();
    let later_terminal_calls = Rc::new(Cell::new(0));
    let callback_calls = later_terminal_calls.clone();
    view.subscribe(move |event| {
        if matches!(
            event,
            ViewSessionEvent::Closed {
                reason: ViewCloseReason::HostClosed
            }
        ) {
            callback_calls.set(callback_calls.get() + 1);
        }
    })
    .unwrap();
    let node_subscription_owner = Rc::new(());
    let callback_owner = node_subscription_owner.clone();
    view.subscribe_node(ids[0], move |_| {
        let _ = &callback_owner;
    })
    .unwrap();
    assert_eq!(Rc::strong_count(&node_subscription_owner), 2);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| view.close()))
        .expect_err("the terminal subscriber panic remains observable");

    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"first terminal observer panic")
    );
    assert_eq!(later_terminal_calls.get(), 1);
    assert_eq!(Rc::strong_count(&node_subscription_owner), 1);
    assert_eq!(view.close_reason(), Some(ViewCloseReason::HostClosed));
    assert_eq!(runtime.live_view_count(), 0);
}

#[test]
fn drop_paths_do_not_replace_an_existing_unwind_with_subscriber_panics() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("runtime");
    let view_terminal_calls = Rc::new(Cell::new(0));
    let callback_calls = view_terminal_calls.clone();

    let view_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let view = runtime.attach_view(Default::default()).expect("view");
        view.subscribe(|event| {
            if matches!(
                event,
                ViewSessionEvent::Closed {
                    reason: ViewCloseReason::HostClosed
                }
            ) {
                std::panic::panic_any(PanicIfDropped);
            }
        })
        .unwrap();
        view.subscribe(move |event| {
            if matches!(
                event,
                ViewSessionEvent::Closed {
                    reason: ViewCloseReason::HostClosed
                }
            ) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .unwrap();
        panic!("outer view unwind");
    }))
    .expect_err("the outer view unwind remains observable");

    assert_eq!(
        view_panic.downcast_ref::<&str>(),
        Some(&"outer view unwind")
    );
    assert_eq!(view_terminal_calls.get(), 1);
    assert_eq!(runtime.live_view_count(), 0);

    struct RuntimeDropsBeforeView {
        _runtime: GraphRuntime,
        _view: ViewSession,
    }

    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    view.subscribe(|event| {
        if matches!(
            event,
            ViewSessionEvent::Closed {
                reason: ViewCloseReason::RuntimeDropped
            }
        ) {
            panic!("runtime drop subscriber panic");
        }
    })
    .unwrap();
    let runtime_terminal_calls = Rc::new(Cell::new(0));
    let callback_calls = runtime_terminal_calls.clone();
    view.subscribe(move |event| {
        if matches!(
            event,
            ViewSessionEvent::Closed {
                reason: ViewCloseReason::RuntimeDropped
            }
        ) {
            callback_calls.set(callback_calls.get() + 1);
        }
    })
    .unwrap();
    let owner = RuntimeDropsBeforeView {
        _runtime: runtime,
        _view: view,
    };

    let runtime_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _ = &owner;
        panic!("outer runtime unwind");
    }))
    .expect_err("the outer runtime unwind remains observable");

    assert_eq!(
        runtime_panic.downcast_ref::<&str>(),
        Some(&"outer runtime unwind")
    );
    assert_eq!(runtime_terminal_calls.get(), 1);
}

#[test]
fn panicking_reentrant_callback_can_unsubscribe_and_close_without_truncating_later_events() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("runtime");
    let view = Rc::new(runtime.attach_view(Default::default()).expect("view"));
    let token_slot = Rc::new(Cell::new(None));
    let callback_token = token_slot.clone();
    let callback_view = view.clone();
    let token = view
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
                callback_view
                    .unsubscribe(callback_token.get().expect("callback token"))
                    .expect("self unsubscribe");
                callback_view.close().expect("reentrant close");
                panic!("reentrant subscriber panic");
            }
        })
        .unwrap();
    token_slot.set(Some(token));
    let later_events = Rc::new(RefCell::new(Vec::new()));
    let callback_events = later_events.clone();
    view.subscribe(move |event| match event {
        ViewSessionEvent::RuntimePatched { .. } => callback_events.borrow_mut().push("patched"),
        ViewSessionEvent::Closed {
            reason: ViewCloseReason::HostClosed,
        } => callback_events.borrow_mut().push("closed"),
        ViewSessionEvent::Closed { .. }
        | ViewSessionEvent::LayoutPublished { .. }
        | ViewSessionEvent::ViewportChanged { .. }
        | ViewSessionEvent::SelectionChanged { .. } => {}
    })
    .unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
    }))
    .expect_err("the reentrant subscriber panic remains observable");

    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"reentrant subscriber panic")
    );
    assert_eq!(later_events.borrow().as_slice(), &["patched", "closed"]);
    assert_eq!(view.close_reason(), Some(ViewCloseReason::HostClosed));
    assert_eq!(runtime.live_view_count(), 0);
    assert_eq!(Rc::strong_count(&view), 1);
}

#[test]
fn document_replacement_fans_out_one_reset_patch_and_retires_every_old_incarnation() {
    let (graph, ids) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("runtime");
    let initial_incarnations = ids
        .iter()
        .map(|node| runtime.node_incarnation(*node).unwrap().unwrap())
        .collect::<Vec<_>>();
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "before_reset": true }),
        ))
        .unwrap();
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    let views = (0..2)
        .map(|_| runtime.attach_view(Default::default()).unwrap())
        .collect::<Vec<_>>();
    let observed = views
        .iter()
        .map(|view| {
            let patches = Rc::new(RefCell::new(Vec::<RuntimePatch>::new()));
            let observer = patches.clone();
            view.subscribe(move |event| {
                if let ViewSessionEvent::RuntimePatched { patch } = event {
                    observer.borrow_mut().push(patch);
                }
            })
            .unwrap();
            patches
        })
        .collect::<Vec<_>>();
    let (replacement, _) = graph_with_nodes(1);
    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();

    let patch = runtime.replace_document(replacement).unwrap();

    assert_eq!(patch.kind(), RuntimePatchKind::DocumentReset);
    assert_eq!(patch.from_revision(), 1);
    assert_eq!(patch.to_revision(), 2);
    assert_eq!(patch.document_epoch(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    let replacement_incarnation = runtime.node_incarnation(ids[0]).unwrap().unwrap();
    assert!(replacement_incarnation > initial_incarnations[0]);
    assert_eq!(
        patch.node_incarnation_change(ids[0]),
        Some(Some(replacement_incarnation))
    );
    assert_eq!(patch.node_incarnation_change(ids[1]), Some(None));
    assert_eq!(runtime.node_incarnation(ids[1]).unwrap(), None);
    assert_eq!(runtime.stats().unwrap().patch_deliveries, 2);
    for (view, patches) in views.iter().zip(&observed) {
        assert_eq!(view.graph_snapshot().unwrap().revision(), 2);
        let patches = patches.borrow();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].kind(), RuntimePatchKind::DocumentReset);
        assert!(patch.ptr_eq(&patches[0]));
    }
}

#[test]
fn foreign_catalog_document_replacement_has_zero_side_effects() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("runtime");
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "retained": true }),
        ))
        .unwrap();
    let view = runtime.attach_view(Default::default()).unwrap();
    let view_events = Rc::new(Cell::new(0));
    let event_count = view_events.clone();
    view.subscribe(move |_| event_count.set(event_count.get() + 1))
        .unwrap();
    let runtime_events = Rc::new(Cell::new(0));
    let event_count = runtime_events.clone();
    runtime
        .subscribe(move |_| event_count.set(event_count.get() + 1))
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let stats_before = runtime.stats().unwrap();
    let incarnation_before = runtime.node_incarnation(ids[0]).unwrap();
    let foreign_catalog = NodeCatalog::new();
    let replacement = foreign_catalog
        .canonicalize_graph(before.graph())
        .expect("structurally equivalent document");

    assert!(matches!(
        runtime.replace_document(replacement),
        Err(GraphRuntimeError::CatalogProvenanceMismatch)
    ));
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(runtime.stats().unwrap(), stats_before);
    assert_eq!(
        runtime.node_incarnation(ids[0]).unwrap(),
        incarnation_before
    );
    assert_eq!(view_events.get(), 0);
    assert_eq!(runtime_events.get(), 0);
}

#[test]
fn one_node_change_has_bounded_four_view_fanout_in_ten_thousand_node_graph() {
    let (graph, ids) = graph_with_nodes(10_000);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let retained_snapshot = runtime.snapshot().expect("retained snapshot");
    assert_eq!(
        retained_snapshot
            .graph()
            .node(&ids[4_321])
            .expect("retained node")
            .data,
        serde_json::Value::Null
    );
    let mut views = Vec::new();
    let mut observed = Vec::new();
    let mut binding_counts = Vec::new();
    for _ in 0..4 {
        let view = runtime
            .attach_view(view_options(
                ViewSessionSeed::new(graph_id).with_stacking(ViewStackingSeed::new(
                    ids.clone(),
                    vec![],
                    vec![],
                )),
            ))
            .expect("view");
        view.seed_all_node_facts_for_test();
        let counters: Vec<_> = ids.iter().map(|_| Rc::new(Cell::new(0))).collect();
        for (node, counter) in ids.iter().copied().zip(&counters) {
            let counter = counter.clone();
            view.subscribe_node(node, move |_| counter.set(counter.get() + 1))
                .unwrap();
        }
        let patches = Rc::new(RefCell::new(Vec::<RuntimePatch>::new()));
        let patches_observer = patches.clone();
        view.subscribe(move |event| {
            if let ViewSessionEvent::RuntimePatched { patch } = event {
                patches_observer.borrow_mut().push(patch);
            }
        })
        .unwrap();
        views.push(view);
        observed.push(patches);
        binding_counts.push(counters);
    }

    let outcome = runtime
        .execute(GraphCommand::set_node_data(
            0,
            ids[4_321],
            serde_json::json!({ "value": 42 }),
        ))
        .expect("single-node command");
    let receipt = outcome.applied().expect("single-node command applies");
    assert_eq!(receipt.revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    assert_eq!(
        retained_snapshot
            .graph()
            .node(&ids[4_321])
            .expect("retained node")
            .data,
        serde_json::Value::Null
    );

    let runtime_stats = runtime.stats().unwrap();
    assert_eq!(runtime_stats.patch_builds, 1);
    assert_eq!(runtime_stats.patch_deliveries, 4);
    assert_eq!(runtime_stats.command_planning_fact_reads, 1);
    assert_eq!(runtime_stats.command_planning_operations_emitted, 1);

    for ((view, patches), counters) in views.iter().zip(&observed).zip(&binding_counts) {
        let stats = view.stats().unwrap();
        assert_eq!(stats.runtime_patches, 1);
        assert_eq!(stats.node_invalidations, 1);
        assert_eq!(stats.fact_records_visited, 1);
        assert_eq!(stats.node_binding_notifications, 1);
        assert_eq!(stats.spatial_updates, 0);
        assert_eq!(stats.route_invalidations, 0);
        assert_eq!(stats.view_state_sanitizations, 0);
        assert_eq!(view.export_seed().unwrap().stacking().nodes().len(), 10_000);
        assert_eq!(view.derived_fact_counts_for_test().nodes, 10_000);
        assert_eq!(patches.borrow().len(), 1);
        assert_eq!(patches.borrow()[0].affected_nodes().len(), 1);
        for (index, counter) in counters.iter().enumerate() {
            assert_eq!(counter.get(), usize::from(index == 4_321));
        }
    }

    let first = observed[0].borrow();
    for patches in observed.iter().skip(1) {
        assert!(first[0].ptr_eq(&patches.borrow()[0]));
    }
}

#[test]
fn one_retirement_uses_exact_tombstones_across_four_ten_thousand_item_draw_orders() {
    let (graph, ids) = graph_with_nodes(10_000);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let retired = ids[4_321];
    let views = (0..4)
        .map(|_| {
            runtime
                .attach_view(view_options(
                    ViewSessionSeed::new(graph_id)
                        .with_selection(ViewSelectionSeed::new(vec![retired], vec![], vec![]))
                        .with_stacking(ViewStackingSeed::new(ids.clone(), vec![], vec![])),
                ))
                .expect("view")
        })
        .collect::<Vec<_>>();

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(retired, IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("retirement applies");

    for view in views {
        assert_eq!(view.draw_order_storage_counts_for_test(), (10_000, 1));
        let stats = view.stats().unwrap();
        assert_eq!(stats.view_state_sanitizations, 1);
        let seed = view.export_seed().unwrap();
        assert!(seed.selection().nodes().is_empty());
        assert_eq!(seed.stacking().nodes().len(), 9_999);
        assert!(!seed.stacking().nodes().contains(&retired));
        assert_eq!(view.draw_order_storage_counts_for_test(), (10_000, 1));
    }
}

#[test]
fn unselected_retirement_skips_full_selection_sanitation_in_four_large_views() {
    let (graph, ids) = graph_with_nodes(10_001);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let selected = ids[..10_000].to_vec();
    let retired = ids[10_000];
    let views = (0..4)
        .map(|_| {
            runtime
                .attach_view(view_options(ViewSessionSeed::new(graph_id).with_selection(
                    ViewSelectionSeed::new(selected.clone(), vec![], vec![]),
                )))
                .expect("view")
        })
        .collect::<Vec<_>>();

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(retired, IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("unselected retirement applies");

    for view in views {
        let seed = view.export_seed().expect("selection remains available");
        assert_eq!(seed.selection().nodes(), selected);
        assert_eq!(view.stats().unwrap().view_state_sanitizations, 0);
    }
}

#[test]
fn retirements_outside_explicit_draw_order_do_not_allocate_tombstones() {
    let (graph, ids) = graph_with_nodes(128);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");

    for (revision, node) in ids.into_iter().enumerate() {
        runtime
            .execute(GraphCommand::topology(
                revision as u64,
                TopologyEdit::new().remove_node(node, IncidentEdgePolicy::Reject),
            ))
            .unwrap()
            .applied()
            .expect("retirement applies");
    }

    assert_eq!(view.draw_order_storage_counts_for_test(), (0, 0));
}

#[test]
fn undoing_retirement_restores_explicit_draw_order_without_a_stale_tombstone() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let node = ids[0];
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(
            ViewSessionSeed::new(graph_id).with_stacking(ViewStackingSeed::new(
                vec![node],
                vec![],
                vec![],
            )),
        ))
        .expect("view");

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(node, IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("retirement applies");
    assert!(view.export_seed().unwrap().stacking().nodes().is_empty());
    assert_eq!(view.draw_order_storage_counts_for_test(), (1, 1));

    runtime
        .execute(GraphCommand::undo(1))
        .unwrap()
        .applied()
        .expect("undo applies");

    assert_eq!(view.export_seed().unwrap().stacking().nodes(), [node]);
    assert_eq!(view.draw_order_storage_counts_for_test(), (1, 0));
}
