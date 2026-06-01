use jellyflow_core::core::{CanvasPoint, Graph, GraphId, NodeId};
use jellyflow_core::ops::GraphTransaction;
use jellyflow_runtime::io::{
    GraphFileV1, NodeGraphEditorConfig, NodeGraphEditorStateFile, NodeGraphInteractionConfig,
    NodeGraphViewState,
};
use jellyflow_runtime::profile::{ApplyPipelineError, GraphProfile as ModuleGraphProfile};
use jellyflow_runtime::rules::ConnectPlan;
use jellyflow_runtime::runtime::{
    auto_pan, commit, conformance, drag, events, selection, store, viewport, xyflow,
};
use jellyflow_runtime::{
    DispatchError, DispatchOutcome, GraphProfile, NodeGraphPatch, NodeGraphStore,
    apply_connect_plan_with_profile, apply_transaction_with_profile,
};

fn accepts_root_profile<T: ?Sized + GraphProfile>() {}

fn accepts_module_profile<T: ?Sized + ModuleGraphProfile>() {}

#[test]
fn crate_root_exposes_canonical_runtime_api() {
    accepts_root_profile::<dyn GraphProfile>();
    accepts_module_profile::<dyn ModuleGraphProfile>();

    let _: fn(
        &mut Graph,
        &mut dyn GraphProfile,
        &GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> = apply_transaction_with_profile;
    let _: fn(
        &mut Graph,
        &mut dyn GraphProfile,
        &ConnectPlan,
    ) -> Result<GraphTransaction, ApplyPipelineError> = apply_connect_plan_with_profile;

    let graph = Graph::new(GraphId::new());
    let store = NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );

    assert_eq!(store.graph().graph_id, graph.graph_id);
    assert!(NodeGraphPatch::default().is_empty());
    let _ = std::mem::size_of::<DispatchOutcome>();
    let _ = std::mem::size_of::<DispatchError>();
}

#[test]
fn explicit_modules_expose_their_owned_surfaces() {
    let graph = Graph::new(GraphId::new());

    let graph_file = GraphFileV1::from_graph(graph.clone());
    assert_eq!(graph_file.graph_id, graph.graph_id);

    let editor_file = NodeGraphEditorStateFile::new(
        graph.graph_id,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    assert_eq!(editor_file.graph_id, graph.graph_id);
    let _interaction = NodeGraphInteractionConfig::default();

    let root_patch = NodeGraphPatch::default();
    let module_patch = commit::NodeGraphPatch::default();
    assert!(root_patch.is_empty());
    assert!(module_patch.is_empty());

    let selection_result = selection::SelectionBoxResult::default();
    assert!(selection_result.is_empty());
    let _selection_options = selection::SelectionBoxOptions::default();

    let _drag_request = drag::NodeDragRequest {
        node: NodeId::new(),
        to: CanvasPoint::default(),
    };
    let _drag_item = drag::NodeDragItem {
        node: NodeId::new(),
        from: CanvasPoint::default(),
        to: CanvasPoint::default(),
    };
    assert_eq!(drag::NODE_DRAG_TRANSACTION_LABEL, "node drag");
    let _ = std::mem::size_of::<drag::NodeDragPlan>();

    let transform =
        viewport::ViewportTransform::new(CanvasPoint { x: 1.0, y: 2.0 }, 1.5).expect("viewport");
    let panned = viewport::pan_viewport(
        transform,
        viewport::ViewportPanRequest::new(CanvasPoint { x: 3.0, y: 6.0 }),
    )
    .expect("pan");
    let _zoomed = viewport::zoom_viewport(
        panned,
        viewport::ViewportZoomRequest::new(CanvasPoint { x: 24.0, y: 12.0 }, 2.0, 0.5, 4.0),
    )
    .expect("zoom");
    let auto_pan_request = auto_pan::AutoPanRequest::new(
        auto_pan::AutoPanActivation::Always,
        CanvasPoint { x: 99.0, y: 40.0 },
        jellyflow_core::core::CanvasSize {
            width: 100.0,
            height: 80.0,
        },
        0.016,
    );
    let auto_pan_plan = auto_pan::compute_auto_pan(
        &jellyflow_runtime::io::NodeGraphAutoPanTuning::default(),
        auto_pan_request,
    )
    .expect("auto-pan");
    let _ = auto_pan_plan.viewport_pan_request();
    let _ = std::mem::size_of::<auto_pan::AutoPanOutcome>();

    let drag_start = events::NodeDragStart {
        primary: NodeId::new(),
        nodes: Vec::new(),
        pointer: CanvasPoint::default(),
    };
    let drag_update = events::NodeDragUpdate {
        primary: drag_start.primary,
        nodes: drag_start.nodes.clone(),
        pointer: drag_start.pointer,
    };
    let _drag_end = events::NodeDragEnd {
        primary: drag_start.primary,
        nodes: drag_start.nodes,
        pointer: drag_update.pointer,
        outcome: events::NodeDragEndOutcome::NoOp,
    };
    let _gesture = events::NodeGraphGestureEvent::NodeDragUpdate(drag_update);

    let _module_store = store::NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let changes = xyflow::NodeGraphChanges::from_patch(&root_patch);
    assert!(changes.is_empty());
    let _ = std::mem::size_of::<xyflow::NodeDragUpdate>();
}

#[test]
fn conformance_module_exposes_serde_friendly_headless_fixture_vocabulary() {
    let graph = Graph::new(GraphId::new());
    let node_id = NodeId::new();
    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let drag_start = events::NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let drag_update = events::NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };

    let scenario = conformance::ConformanceScenario::new("public node drag fixture", graph)
        .with_view_state(NodeGraphViewState::default())
        .with_editor_config(NodeGraphEditorConfig::default())
        .with_trace_config(conformance::ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            conformance::ConformanceAction::emit_gesture(
                events::NodeGraphGestureEvent::NodeDragStart(drag_start.clone()),
            ),
            conformance::ConformanceAction::apply_node_drag(node_id, target),
            conformance::ConformanceAction::emit_gesture(
                events::NodeGraphGestureEvent::NodeDragUpdate(drag_update.clone()),
            ),
        ])
        .with_expected_trace([
            conformance::ConformanceTraceEvent::gesture(
                events::NodeGraphGestureEvent::NodeDragStart(drag_start.clone()),
            ),
            conformance::ConformanceTraceEvent::graph_commit(
                Some(drag::NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos"],
            ),
            conformance::ConformanceTraceEvent::callback(
                conformance::ConformanceCallbackEvent::NodeDrag(drag_update),
            ),
        ]);

    assert_eq!(
        scenario.schema_version,
        conformance::CONFORMANCE_FIXTURE_SCHEMA_VERSION,
    );
    assert!(scenario.setup.trace.record_xyflow_callbacks);
    assert_eq!(scenario.actions.len(), 3);
    assert_eq!(scenario.expected_trace.len(), 3);

    let encoded = serde_json::to_value(&scenario).expect("serialize fixture");
    let decoded: conformance::ConformanceScenario =
        serde_json::from_value(encoded.clone()).expect("deserialize fixture");
    assert_eq!(
        serde_json::to_value(decoded).expect("reserialize fixture"),
        encoded,
    );

    let empty_scenario =
        conformance::ConformanceScenario::new("public empty fixture", Graph::new(GraphId::new()));
    let mut suite = conformance::ConformanceSuite::new("public adapter suite");
    suite.push_scenario(empty_scenario.clone());
    let suite = suite.with_scenarios([empty_scenario]);
    let suite_report = conformance::run_conformance_suite(&suite);
    assert!(suite_report.is_match(), "{suite_report}");
    assert_eq!(suite_report.scenario_count(), 1);
    assert_eq!(suite_report.failed_scenarios(), 0);

    let suite_encoded = serde_json::to_value(&suite).expect("serialize suite");
    let suite_decoded: conformance::ConformanceSuite =
        serde_json::from_value(suite_encoded.clone()).expect("deserialize suite");
    assert_eq!(
        serde_json::to_value(suite_decoded).expect("reserialize suite"),
        suite_encoded,
    );

    let suite_path = std::env::temp_dir().join(format!(
        "jellyflow-public-suite-{}.json",
        uuid::Uuid::new_v4()
    ));
    suite.save_json(&suite_path).expect("save suite json");
    let loaded_suite =
        conformance::ConformanceSuite::load_json(&suite_path).expect("load suite json");
    assert!(loaded_suite.run().is_match());
    assert!(
        conformance::ConformanceSuite::load_json_if_exists(&suite_path)
            .expect("optional suite json")
            .is_some()
    );
    let _ = std::fs::remove_file(&suite_path);
    assert!(
        conformance::ConformanceSuite::load_json_if_exists(&suite_path)
            .expect("missing optional suite json")
            .is_none()
    );
    let _ = std::mem::size_of::<conformance::ConformanceFixtureFileError>();
}
