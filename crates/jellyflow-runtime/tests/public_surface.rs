use jellyflow_core::core::{CanvasPoint, Graph, GraphId, NodeId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::GraphTransaction;
use jellyflow_runtime::io::{
    GraphFileV1, NodeGraphEditorConfig, NodeGraphEditorStateFile, NodeGraphInteractionConfig,
    NodeGraphInteractionState, NodeGraphPanOnDragButtons, NodeGraphViewState,
};
use jellyflow_runtime::profile::{ApplyPipelineError, GraphProfile as ModuleGraphProfile};
use jellyflow_runtime::rules::ConnectPlan;
use jellyflow_runtime::runtime::{
    auto_pan, commit, conformance, connection, drag, events, selection, store, viewport, xyflow,
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
    assert!(selection::selection_drag_threshold_met(
        selection::SelectionDragActivationInput::new(CanvasPoint { x: 3.0, y: 4.0 }, 4.0, false),
    ));
    let drag_start_selection = selection::resolve_node_drag_start_selection(
        &graph,
        &NodeGraphViewState::default(),
        &NodeGraphInteractionState::default(),
        selection::NodeDragStartSelectionInput::new(NodeId::new(), false),
    );
    assert_eq!(
        drag_start_selection,
        selection::NodeDragStartSelectionAction::Unchanged
    );

    let _drag_request = drag::NodeDragRequest {
        node: NodeId::new(),
        to: CanvasPoint::default(),
    };
    assert!(connection::connection_drag_threshold_met(
        connection::ConnectionDragActivationInput::new(CanvasPoint { x: 3.0, y: 4.0 }, 4.0),
    ));
    let from_handle = connection::ConnectionHandleRef::new(
        NodeId::new(),
        jellyflow_core::core::PortId::new(),
        jellyflow_core::core::PortDirection::Out,
    );
    let _: Option<connection::ClosestConnectionHandle> =
        connection::closest_connection_handle(connection::ClosestConnectionHandleInput::new(
            CanvasPoint::default(),
            0.0,
            from_handle,
            &[],
        ));
    assert_eq!(
        connection::connection_handle_validity(true, false),
        connection::ConnectionHandleValidity::Invalid
    );
    let target_handle = connection::ConnectionTargetHandle::new(
        connection::ConnectionHandleRef::new(
            NodeId::new(),
            jellyflow_core::core::PortId::new(),
            jellyflow_core::core::PortDirection::In,
        ),
        true,
        true,
    );
    let resolved_target =
        connection::resolve_connection_target(connection::ConnectionTargetInput::new(
            from_handle,
            Some(target_handle),
            NodeGraphConnectionMode::Strict,
            true,
        ));
    let _: Option<connection::ConnectionHandleConnection> = resolved_target.connection;
    assert_eq!(
        resolved_target.feedback,
        connection::ConnectionHandleValidity::Valid
    );
    assert!(drag::node_drag_threshold_met(
        drag::NodeDragActivationInput::new(CanvasPoint { x: 3.0, y: 4.0 }, 4.0),
    ));
    let _drag_item = drag::NodeDragItem {
        node: NodeId::new(),
        from: CanvasPoint::default(),
        to: CanvasPoint::default(),
    };
    assert_eq!(drag::NODE_DRAG_TRANSACTION_LABEL, "node drag");
    let _ = std::mem::size_of::<drag::NodeDragPlan>();

    let transform =
        viewport::ViewportTransform::new(CanvasPoint { x: 1.0, y: 2.0 }, 1.5).expect("viewport");
    assert_eq!(
        viewport::resolve_pane_click_distance(viewport::PaneClickDistanceInput::new(3.0, false)),
        3.0
    );
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
    let interaction_state = NodeGraphInteractionState::default();
    let scroll_intent = viewport::resolve_viewport_scroll_gesture(
        &interaction_state.pan_interaction(),
        &interaction_state.zoom_interaction(),
        viewport::ViewportGestureContext::idle(),
        viewport::ViewportScrollInput::new(
            CanvasPoint { x: 3.0, y: 6.0 },
            CanvasPoint { x: 24.0, y: 12.0 },
            false,
            2.0,
            0.5,
            4.0,
        ),
    )
    .expect("scroll policy");
    assert_eq!(
        scroll_intent.move_kind(),
        events::ViewportMoveKind::ZoomWheel
    );
    let drag_state = NodeGraphInteractionState {
        pan_on_drag: NodeGraphPanOnDragButtons {
            left: true,
            middle: false,
            right: false,
        },
        ..NodeGraphInteractionState::default()
    };
    let drag_intent = viewport::resolve_viewport_drag_pan_gesture(
        &drag_state.pan_interaction(),
        viewport::ViewportGestureContext::idle(),
        viewport::ViewportDragPanInput::new(
            viewport::ViewportPointerButton::Left,
            CanvasPoint { x: 1.0, y: 0.0 },
        ),
    )
    .expect("drag pan policy");
    assert_eq!(drag_intent.move_kind(), events::ViewportMoveKind::PanDrag);
    let _ = std::mem::size_of::<viewport::ViewportGestureRejection>();
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
    let viewport_scroll_action = conformance::ConformanceAction::apply_viewport_scroll_gesture(
        viewport::ViewportGestureContext::idle(),
        viewport::ViewportScrollInput::new(
            CanvasPoint { x: 1.0, y: 2.0 },
            CanvasPoint { x: 4.0, y: 5.0 },
            false,
            1.25,
            0.5,
            4.0,
        ),
    );
    let viewport_reject_action =
        conformance::ConformanceAction::expect_viewport_drag_pan_gesture_rejected(
            viewport::ViewportGestureContext {
                connection_in_progress: true,
                ..viewport::ViewportGestureContext::idle()
            },
            viewport::ViewportDragPanInput::new(
                viewport::ViewportPointerButton::Right,
                CanvasPoint { x: 4.0, y: 5.0 },
            ),
            viewport::ViewportGestureRejection::ConnectionInProgress,
        );
    let encoded_viewport_actions =
        serde_json::to_value([viewport_scroll_action, viewport_reject_action])
            .expect("serialize viewport fixture actions");
    assert!(encoded_viewport_actions.is_array());

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
    let suite_approval = suite.approve_actual_traces();
    assert!(suite_approval.is_approvable());
    let suite_approval_encoded =
        serde_json::to_value(&suite_approval.report).expect("serialize suite approval report");
    let suite_approval_decoded: conformance::ConformanceSuiteApprovalReport =
        serde_json::from_value(suite_approval_encoded).expect("deserialize suite approval report");
    assert!(suite_approval_decoded.is_approvable());

    let fixture_root = std::env::temp_dir().join(format!(
        "jellyflow-public-fixtures-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&fixture_root).expect("create fixture root");
    suite
        .save_json(fixture_root.join("suite.json"))
        .expect("save directory suite json");
    let fixture_directory = conformance::ConformanceFixtureDirectory::load_json(&fixture_root)
        .expect("load fixture directory");
    assert_eq!(fixture_directory.file_count(), 1);
    let file_approval =
        conformance::ConformanceSuiteFile::load_json(fixture_root.join("suite.json"))
            .expect("load suite file")
            .approve_actual_traces_to_json()
            .expect("approve suite file");
    assert!(file_approval.is_approvable());
    let directory_approval = fixture_directory
        .approve_actual_traces_to_json()
        .expect("approve fixture directory");
    assert!(directory_approval.is_approvable());
    let fixture_report = fixture_directory.run();
    assert!(fixture_report.is_match());
    assert_eq!(fixture_report.failed_files(), 0);
    let fixture_report_encoded =
        serde_json::to_value(&fixture_report).expect("serialize fixture directory report");
    let fixture_report_decoded: conformance::ConformanceFixtureDirectoryReport =
        serde_json::from_value(fixture_report_encoded)
            .expect("deserialize fixture directory report");
    assert!(fixture_report_decoded.is_match());
    assert!(
        conformance::ConformanceFixtureDirectory::load_json_if_exists(&fixture_root)
            .expect("optional fixture directory")
            .is_some()
    );
    let _ = std::fs::remove_dir_all(&fixture_root);
    assert!(
        conformance::ConformanceFixtureDirectory::load_json_if_exists(&fixture_root)
            .expect("missing optional fixture directory")
            .is_none()
    );
    let _ = std::mem::size_of::<conformance::ConformanceSuiteFile>();
    let _ = std::mem::size_of::<conformance::ConformanceSuiteFileReport>();
    let _ = std::mem::size_of::<conformance::ConformanceFixtureDirectoryReport>();
    let _ = std::mem::size_of::<conformance::ConformanceSuiteApproval>();
    let _ = std::mem::size_of::<conformance::ConformanceSuiteApprovalReport>();
    let _ = std::mem::size_of::<conformance::ConformanceScenarioApprovalReport>();
    let _ = std::mem::size_of::<conformance::ConformanceSuiteFileApprovalReport>();
    let _ = std::mem::size_of::<conformance::ConformanceFixtureDirectoryApprovalReport>();
    let _ = std::mem::size_of::<conformance::ConformanceApprovalError>();
}
