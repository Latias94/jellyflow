use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphId, GroupId, NodeId, PortDirection,
    PortId,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::GraphTransaction;
use jellyflow_runtime::io::{
    GraphFileV1, NodeGraphEditorConfig, NodeGraphEditorStateFile, NodeGraphInteractionConfig,
    NodeGraphInteractionState, NodeGraphPanInertiaTuning, NodeGraphPanOnDragButtons,
    NodeGraphViewState,
};
use jellyflow_runtime::profile::{ApplyPipelineError, GraphProfile as ModuleGraphProfile};
use jellyflow_runtime::rules::{ConnectPlan, EdgeEndpoint};
use jellyflow_runtime::runtime::{
    auto_pan, commit, conformance, connection, delete, drag, events, geometry, gesture, keyboard,
    measurement, rendering, resize, selection, store, viewport, xyflow,
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
    let selection_input = selection::SelectionBoxInput::replace(CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 10.0,
            height: 10.0,
        },
    });
    let reverse_drag_selection_input = selection::SelectionBoxInput::replace_from_drag(
        CanvasPoint { x: 10.0, y: 10.0 },
        CanvasPoint::default(),
    );
    assert_eq!(reverse_drag_selection_input.rect, selection_input.rect);
    let selection_store = NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let selection_decision = selection::resolve_selection_box(
        &graph,
        selection_store.lookups(),
        selection_store.view_state(),
        &NodeGraphInteractionState::default(),
        selection_input,
    );
    assert!(selection_decision.result().is_empty());
    let _selection_modifier = selection::SelectionModifier::Additive;
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
    let target_candidate = connection::ConnectionTargetCandidate::new(
        target_handle,
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 100.0,
                height: 80.0,
            },
        },
        geometry::HandleBounds {
            rect: CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 10.0,
                    height: 10.0,
                },
            },
            position: geometry::HandlePosition::Right,
        },
    );
    let target_candidates = [target_candidate];
    let resolved_target_from_handles = connection::resolve_connection_target_from_handles(
        connection::ConnectionTargetFromHandlesInput::new(
            CanvasPoint { x: 5.0, y: 5.0 },
            10.0,
            from_handle,
            &target_candidates,
            NodeGraphConnectionMode::Strict,
        ),
    );
    let _: Option<connection::ConnectionHandleConnection> = resolved_target.connection;
    let _: Option<connection::ConnectionHandleConnection> = resolved_target_from_handles.connection;
    assert_eq!(
        resolved_target.feedback,
        connection::ConnectionHandleValidity::Valid
    );
    let indicator = connection::resolve_connection_handle_indicator(
        connection::ConnectionHandleIndicatorInput::new(
            target_handle.handle,
            NodeGraphConnectionMode::Strict,
        )
        .with_connection(
            Some(from_handle),
            Some(target_handle.handle),
            resolved_target.feedback,
        ),
    );
    let _: connection::ConnectionHandleIndicator = indicator;
    assert!(indicator.show_connection_indicator);
    let _ = std::mem::size_of::<connection::ConnectEdgeRequest>();
    let _ = std::mem::size_of::<connection::ConnectEdgeError>();
    let _: fn(&ConnectPlan) -> Option<GraphTransaction> = connection::connect_edge_transaction;
    let _: fn(&ConnectPlan, EdgeId) -> Option<GraphTransaction> =
        connection::connect_edge_transaction_with_edge_id;
    assert_eq!(connection::CONNECT_EDGE_TRANSACTION_LABEL, "connect edge");
    let _ = std::mem::size_of::<connection::ReconnectEdgeRequest>();
    let _ = std::mem::size_of::<connection::ReconnectEdgeError>();
    let _: fn(&ConnectPlan) -> Option<GraphTransaction> = connection::reconnect_edge_transaction;
    assert_eq!(
        connection::RECONNECT_EDGE_TRANSACTION_LABEL,
        "reconnect edge"
    );
    assert!(drag::node_drag_threshold_met(
        drag::NodeDragActivationInput::new(CanvasPoint { x: 3.0, y: 4.0 }, 4.0),
    ));
    assert_eq!(
        drag::resolve_pointer_gesture_claim(drag::PointerGestureClaimInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            false,
            false,
            false,
            8.0,
            4.0,
        )),
        drag::PointerGestureClaim::NodeDrag
    );
    let _drag_item = drag::NodeDragItem {
        node: NodeId::new(),
        from: CanvasPoint::default(),
        to: CanvasPoint::default(),
    };
    let _node_drag_session = gesture::NodeDragSession::new(
        NodeId::new(),
        CanvasPoint::default(),
        CanvasPoint { x: 8.0, y: 8.0 },
    );
    let _connection_handle_target = gesture::PointerSessionTarget::ConnectionHandle(from_handle);
    let _ = std::mem::size_of::<gesture::ConnectEdgeSession>();
    let _viewport_drag_pan_session = gesture::ViewportDragPanSession::new(
        viewport::ViewportGestureContext::idle(),
        viewport::ViewportDragPanInput::new(
            viewport::ViewportPointerButton::Left,
            CanvasPoint { x: 3.0, y: 4.0 },
        ),
    );
    assert_eq!(
        gesture::PointerSessionClaim::NodeDrag,
        gesture::PointerSessionClaim::NodeDrag
    );
    let _ = std::mem::size_of::<gesture::PointerSessionClaimOutcome>();
    let _ = std::mem::size_of::<gesture::PointerSessionClaimRejection>();
    let _: fn(
        &NodeGraphStore,
        gesture::PointerSessionClaimInput,
    ) -> gesture::PointerSessionClaimOutcome = NodeGraphStore::resolve_pointer_session_claim;
    let measured_handle = measurement::MeasuredHandle::new(
        from_handle,
        geometry::HandleBounds {
            rect: CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 10.0,
                    height: 10.0,
                },
            },
            position: geometry::HandlePosition::Right,
        },
    );
    let _node_measurement = measurement::NodeMeasurement::new(NodeId::new())
        .with_size(Some(CanvasSize {
            width: 100.0,
            height: 80.0,
        }))
        .with_handles([measured_handle]);
    let _ = std::mem::size_of::<measurement::NodeMeasurementOutcome>();
    let _ = std::mem::size_of::<measurement::NodeMeasurementError>();
    let _ = std::mem::size_of::<measurement::LayoutEdgePosition>();
    let _ = std::mem::size_of::<measurement::LayoutFactsQueryResult>();
    let _: fn(
        &mut NodeGraphStore,
        measurement::NodeMeasurement,
    )
        -> Result<measurement::NodeMeasurementOutcome, measurement::NodeMeasurementError> =
        NodeGraphStore::report_node_measurement;
    let _: fn(&mut NodeGraphStore, NodeId) -> measurement::NodeMeasurementOutcome =
        NodeGraphStore::clear_node_measurement;
    let _: fn(&NodeGraphStore, NodeId) -> Option<measurement::NodeMeasurement> =
        NodeGraphStore::node_measurement;
    let _: fn(&NodeGraphStore) -> u64 = NodeGraphStore::layout_facts_revision;
    let _: fn(&NodeGraphStore, CanvasSize) -> measurement::LayoutFactsQueryResult =
        NodeGraphStore::layout_facts_query;
    let _: fn(&NodeGraphStore) -> Vec<connection::ConnectionTargetCandidate> =
        NodeGraphStore::connection_target_candidates_from_layout_facts;
    let _: fn(
        &NodeGraphStore,
        CanvasPoint,
        connection::ConnectionHandleRef,
    ) -> connection::ResolvedConnectionTarget =
        NodeGraphStore::resolve_connection_target_from_layout_facts;
    let _: fn(&NodeGraphStore, EdgeId) -> Option<geometry::EdgePosition> =
        NodeGraphStore::edge_position_from_layout_facts;
    let _ = std::mem::size_of::<selection::NodePointerDownDecision>();
    assert_eq!(drag::NODE_DRAG_TRANSACTION_LABEL, "node drag");
    let _ = std::mem::size_of::<drag::NodeDragPlan>();
    let resize_request = resize::NodeResizeRequest::new(
        NodeId::new(),
        CanvasSize {
            width: 1.0,
            height: 1.0,
        },
    )
    .with_constraints(resize::NodeResizeConstraints::unconstrained());
    let _: resize::NodeResizeRequest = resize_request;
    let _: resize::NodeResizeRequest =
        resize_request.with_direction(resize::NodeResizeDirection::BottomRight);
    let pointer_resize_request = resize::NodePointerResizeRequest::new(
        NodeId::new(),
        CanvasPoint { x: 1.0, y: 1.0 },
        CanvasPoint { x: 2.0, y: 3.0 },
        resize::NodeResizeDirection::BottomRight,
    )
    .with_constraints(resize::NodeResizeConstraints::unconstrained())
    .with_keep_aspect_ratio(true)
    .with_axis(resize::NodeResizeAxis::Both);
    let _: resize::NodePointerResizeRequest = pointer_resize_request;
    let _: resize::NodeResizeContext = resize::NodeResizeContext::new((0.5, 0.5));
    let _ = std::mem::size_of::<resize::NodeResizeAxis>();
    let _ = std::mem::size_of::<resize::NodeResizeDirection>();
    let _ = std::mem::size_of::<resize::NodeResizeItem>();
    let _ = std::mem::size_of::<resize::NodeResizePlan>();
    let _: fn(&Graph, resize::NodeResizeRequest) -> Option<resize::NodeResizePlan> =
        resize::plan_node_resize;
    let _: fn(
        &Graph,
        resize::NodeResizeContext,
        resize::NodeResizeRequest,
    ) -> Option<resize::NodeResizePlan> = resize::plan_node_resize_with_context;
    let _: fn(&Graph, resize::NodePointerResizeRequest) -> Option<resize::NodeResizePlan> =
        resize::plan_node_pointer_resize;
    let _: fn(
        &Graph,
        resize::NodeResizeContext,
        resize::NodePointerResizeRequest,
    ) -> Option<resize::NodeResizePlan> = resize::plan_node_pointer_resize_with_context;
    let _: fn(&NodeGraphStore, resize::NodePointerResizeRequest) -> Option<resize::NodeResizePlan> =
        NodeGraphStore::plan_node_pointer_resize;
    let _: fn(
        &mut NodeGraphStore,
        resize::NodePointerResizeRequest,
    ) -> Result<Option<DispatchOutcome>, DispatchError> = NodeGraphStore::apply_node_pointer_resize;
    assert_eq!(resize::NODE_RESIZE_TRANSACTION_LABEL, "node resize");
    assert_eq!(
        delete::DELETE_SELECTION_TRANSACTION_LABEL,
        "delete selection"
    );
    let delete_elements = delete::DeleteElements::new([NodeId::new()], [EdgeId::new()]);
    let _pre_delete =
        delete::PreDeleteRequest::new(delete_elements.clone(), delete_elements.clone());
    let _pre_delete_accept = delete::PreDeleteResolution::accept();
    let _pre_delete_veto = delete::PreDeleteResolution::veto();
    let _pre_delete_replace =
        delete::PreDeleteResolution::replace([NodeId::new()], [EdgeId::new()]);
    let _: fn(&NodeGraphViewState) -> delete::DeleteElements = delete::delete_selection_elements;
    let _: fn(&jellyflow_runtime::rules::DeletePlan) -> delete::DeleteElements =
        delete::delete_elements_from_plan;
    let _: fn(
        &NodeGraphStore,
    ) -> Result<Option<delete::PreDeleteRequest>, delete::DeleteSelectionError> =
        NodeGraphStore::prepare_delete_selection;
    let _: fn(
        &mut NodeGraphStore,
        &delete::PreDeleteRequest,
        delete::PreDeleteResolution,
    ) -> Result<Option<DispatchOutcome>, delete::DeleteSelectionError> =
        NodeGraphStore::apply_pre_delete_resolution;
    let delete_plan = delete::plan_delete_selection(
        &graph,
        &NodeGraphViewState::default(),
        &NodeGraphInteractionState::default(),
    );
    assert!(delete::delete_selection_transaction(&delete_plan).is_none());
    let _ = std::mem::size_of::<delete::DeleteSelectionError>();
    let _ = std::mem::size_of::<keyboard::KeyboardIntent>();
    let _ = std::mem::size_of::<keyboard::KeyboardActionError>();
    let _ = std::mem::size_of::<keyboard::KeyboardActionOutcome>();
    let _ = std::mem::size_of::<keyboard::KeyboardDeleteAction>();

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
    let zoomed = viewport::zoom_viewport(
        panned,
        viewport::ViewportZoomRequest::new(CanvasPoint { x: 24.0, y: 12.0 }, 2.0, 0.5, 4.0),
    )
    .expect("zoom");
    let animation_plan =
        viewport::plan_viewport_animation_with_options(viewport::ViewportAnimationRequest::new(
            transform,
            zoomed,
            viewport::ViewportAnimationOptions::new(0.25)
                .with_easing(viewport::ViewportAnimationEasing::Linear),
        ))
        .expect("viewport animation plan");
    let animation_frame = animation_plan
        .frame_at(0.125)
        .expect("viewport animation frame");
    assert!(!animation_frame.done);
    assert_eq!(
        animation_plan.easing,
        viewport::ViewportAnimationEasing::Linear
    );
    let _ = std::mem::size_of::<viewport::ViewportAnimationEasing>();
    let _ = std::mem::size_of::<viewport::ViewportAnimationPlan>();
    let _ = std::mem::size_of::<viewport::ViewportAnimationFrame>();
    let inertia_plan =
        viewport::plan_viewport_pan_inertia(viewport::ViewportPanInertiaRequest::new(
            transform,
            CanvasPoint { x: 500.0, y: 0.0 },
            NodeGraphPanInertiaTuning {
                enabled: true,
                decay_per_s: 2.0,
                min_speed: 100.0,
                max_speed: 1000.0,
            },
        ))
        .expect("pan inertia plan");
    let inertia_frame = inertia_plan.frame_at(0.25).expect("pan inertia frame");
    assert!(!inertia_frame.done);
    let _ = std::mem::size_of::<viewport::ViewportPanInertiaRequest>();
    let _ = std::mem::size_of::<viewport::ViewportPanInertiaPlan>();
    let _ = std::mem::size_of::<viewport::ViewportPanInertiaFrame>();
    let interaction_state = NodeGraphInteractionState::default();
    let double_click_plan = viewport::resolve_viewport_double_click_zoom(
        &interaction_state.zoom_interaction(),
        viewport::ViewportDoubleClickZoomInput::new(
            transform,
            CanvasPoint { x: 24.0, y: 12.0 },
            2.0,
            0.5,
            4.0,
            viewport::ViewportAnimationOptions::new(0.2),
        ),
    )
    .expect("double-click zoom policy");
    assert_eq!(double_click_plan.from, transform);
    let _ = std::mem::size_of::<viewport::ViewportDoubleClickZoomInput>();
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
    let selection_auto_pan_request = auto_pan::SelectionAutoPanRequest::new(
        CanvasPoint { x: 99.0, y: 40.0 },
        jellyflow_core::core::CanvasSize {
            width: 100.0,
            height: 80.0,
        },
        0.016,
    );
    let _ = auto_pan::compute_selection_auto_pan(
        &jellyflow_runtime::io::NodeGraphAutoPanTuning::default(),
        selection_auto_pan_request,
    )
    .expect("selection auto-pan");
    let _ = auto_pan_plan.viewport_pan_request();
    let _ = std::mem::size_of::<auto_pan::AutoPanOutcome>();
    let _: fn(
        &mut NodeGraphStore,
        auto_pan::SelectionAutoPanRequest,
    ) -> Option<auto_pan::AutoPanOutcome> = NodeGraphStore::apply_selection_auto_pan;

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
    let resize_start = events::NodeResizeStart {
        node: NodeId::new(),
        direction: resize::NodeResizeDirection::BottomRight,
        pointer: CanvasPoint::default(),
    };
    let resize_update = events::NodeResizeUpdate {
        node: resize_start.node,
        direction: resize_start.direction,
        pointer: CanvasPoint { x: 2.0, y: 3.0 },
        position: CanvasPoint::default(),
        size: CanvasSize {
            width: 4.0,
            height: 5.0,
        },
    };
    let resize_end = events::NodeResizeEnd {
        node: resize_start.node,
        direction: resize_start.direction,
        pointer: resize_update.pointer,
        outcome: events::NodeResizeEndOutcome::Committed,
    };
    let _resize_start_event = events::NodeGraphGestureEvent::NodeResizeStart(resize_start.clone());
    let _resize_update_event =
        events::NodeGraphGestureEvent::NodeResizeUpdate(resize_update.clone());
    let _resize_end_event = events::NodeGraphGestureEvent::NodeResizeEnd(resize_end.clone());

    let module_store = store::NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let _: fn(&NodeGraphStore, CanvasSize) -> Vec<NodeId> = NodeGraphStore::visible_node_ids;
    let _: fn(&NodeGraphStore, CanvasSize) -> Vec<NodeId> =
        NodeGraphStore::visible_node_render_order;
    let _: fn(&NodeGraphStore, CanvasSize) -> Vec<EdgeId> = NodeGraphStore::visible_edge_ids;
    let _: fn(&NodeGraphStore, CanvasSize) -> Vec<EdgeId> =
        NodeGraphStore::visible_edge_render_order;
    let _: fn(&NodeGraphStore) -> Vec<GroupId> = NodeGraphStore::group_render_order;
    let _: fn(&NodeGraphStore) -> Vec<NodeId> = NodeGraphStore::node_render_order;
    let _: fn(&NodeGraphStore) -> Vec<EdgeId> = NodeGraphStore::edge_render_order;
    let _ = std::mem::size_of::<rendering::RenderingQueryResult>();
    let query = module_store.rendering_query(CanvasSize {
        width: 100.0,
        height: 80.0,
    });
    assert!(query.group_order.is_empty());
    assert!(query.node_order.is_empty());
    assert!(query.edge_order.is_empty());
    assert!(query.visible_node_ids.is_empty());
    assert!(query.visible_node_render_order.is_empty());
    assert!(query.visible_edge_ids.is_empty());
    assert!(query.visible_edge_render_order.is_empty());
    let changes = xyflow::NodeGraphChanges::from_patch(&root_patch);
    assert!(changes.is_empty());
    let _ = std::mem::size_of::<xyflow::XyFlowNodeElement>();
    let _ = std::mem::size_of::<xyflow::XyFlowEdgeElement>();
    let _ = std::mem::size_of::<xyflow::XyFlowNodeChange>();
    let _ = std::mem::size_of::<xyflow::XyFlowEdgeChange>();
    let _ = std::mem::size_of::<xyflow::XyFlowDimensionAttribute>();
    let _ = std::mem::size_of::<xyflow::XyFlowDimensionsSetAttributes>();
    let _: fn(
        &[xyflow::XyFlowNodeChange],
        &[xyflow::XyFlowNodeElement],
    ) -> Vec<xyflow::XyFlowNodeElement> = xyflow::apply_xyflow_node_changes;
    let _: fn(
        &[xyflow::XyFlowEdgeChange],
        &[xyflow::XyFlowEdgeElement],
    ) -> Vec<xyflow::XyFlowEdgeElement> = xyflow::apply_xyflow_edge_changes;
    let _ = std::mem::size_of::<xyflow::NodeDragUpdate>();
    let _ = std::mem::size_of::<xyflow::NodeResizeStart>();
    let _ = std::mem::size_of::<xyflow::NodeResizeUpdate>();
    let _ = std::mem::size_of::<xyflow::NodeResizeEnd>();
    let _ = std::mem::size_of::<xyflow::NodeResizeEndOutcome>();
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
    let resize_start = events::NodeResizeStart {
        node: node_id,
        direction: resize::NodeResizeDirection::BottomRight,
        pointer: CanvasPoint { x: 1.0, y: 1.0 },
    };
    let resize_update = events::NodeResizeUpdate {
        node: node_id,
        direction: resize::NodeResizeDirection::BottomRight,
        pointer: CanvasPoint { x: 2.0, y: 3.0 },
        position: CanvasPoint::default(),
        size: CanvasSize {
            width: 2.0,
            height: 3.0,
        },
    };
    let resize_end = events::NodeResizeEnd {
        node: node_id,
        direction: resize::NodeResizeDirection::BottomRight,
        pointer: resize_update.pointer,
        outcome: events::NodeResizeEndOutcome::Committed,
    };
    let resize_start_event = events::NodeGraphGestureEvent::NodeResizeStart(resize_start.clone());
    let resize_update_event =
        events::NodeGraphGestureEvent::NodeResizeUpdate(resize_update.clone());
    let resize_end_event = events::NodeGraphGestureEvent::NodeResizeEnd(resize_end.clone());
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
    let auto_pan_action =
        conformance::ConformanceAction::apply_auto_pan(auto_pan::AutoPanRequest::new(
            auto_pan::AutoPanActivation::Always,
            CanvasPoint { x: 99.0, y: 40.0 },
            CanvasSize {
                width: 100.0,
                height: 80.0,
            },
            0.016,
        ));
    let selection_auto_pan_action = conformance::ConformanceAction::apply_selection_auto_pan(
        auto_pan::SelectionAutoPanRequest::new(
            CanvasPoint { x: 99.0, y: 40.0 },
            CanvasSize {
                width: 100.0,
                height: 80.0,
            },
            0.016,
        ),
    );
    let viewport_pan_constrained_action =
        conformance::ConformanceAction::apply_viewport_pan_constrained(
            viewport::ViewportPanRequest::new(CanvasPoint { x: 20.0, y: -10.0 }),
            CanvasSize {
                width: 100.0,
                height: 80.0,
            },
        );
    let viewport_zoom_constrained_action =
        conformance::ConformanceAction::apply_viewport_zoom_constrained(
            viewport::ViewportZoomRequest::new(CanvasPoint::default(), 1.5, 0.5, 4.0),
            CanvasSize {
                width: 100.0,
                height: 80.0,
            },
        );
    let viewport_animation_action = conformance::ConformanceAction::assert_viewport_animation_frame(
        viewport::ViewportAnimationRequest::new(
            viewport::ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
            viewport::ViewportTransform::new(CanvasPoint { x: 10.0, y: 0.0 }, 2.0)
                .expect("viewport"),
            viewport::ViewportAnimationOptions::new(1.0),
        ),
        0.5,
        viewport::ViewportAnimationFrame {
            elapsed_seconds: 0.5,
            progress: 0.5,
            eased_progress: 0.5,
            transform: viewport::ViewportTransform::new(CanvasPoint { x: 5.0, y: 0.0 }, 1.5)
                .expect("viewport"),
            done: false,
        },
    );
    let apply_viewport_animation_action =
        conformance::ConformanceAction::apply_viewport_animation_frame(
            viewport::ViewportAnimationRequest::new(
                viewport::ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
                viewport::ViewportTransform::new(CanvasPoint { x: 10.0, y: 0.0 }, 2.0)
                    .expect("viewport"),
                viewport::ViewportAnimationOptions::new(1.0),
            ),
            0.5,
        );
    let apply_viewport_animation_frames_action =
        conformance::ConformanceAction::apply_viewport_animation_frames(
            viewport::ViewportAnimationRequest::new(
                viewport::ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
                viewport::ViewportTransform::new(CanvasPoint { x: 10.0, y: 0.0 }, 2.0)
                    .expect("viewport"),
                viewport::ViewportAnimationOptions::new(1.0),
            ),
            [0.5, 1.0],
        );
    let visible_node_ids_action = conformance::ConformanceAction::assert_visible_node_ids(
        CanvasSize {
            width: 100.0,
            height: 80.0,
        },
        [node_id],
    );
    let visible_node_render_order_action =
        conformance::ConformanceAction::assert_visible_node_render_order(
            CanvasSize {
                width: 100.0,
                height: 80.0,
            },
            [node_id],
        );
    let inertia_request = viewport::ViewportPanInertiaRequest::new(
        viewport::ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
        CanvasPoint { x: 500.0, y: 0.0 },
        NodeGraphPanInertiaTuning {
            enabled: true,
            decay_per_s: 2.0,
            min_speed: 100.0,
            max_speed: 1000.0,
        },
    );
    let inertia_frame = viewport::plan_viewport_pan_inertia(inertia_request.clone())
        .expect("inertia plan")
        .frame_at(0.25)
        .expect("inertia frame");
    let apply_viewport_pan_inertia_action =
        conformance::ConformanceAction::apply_viewport_pan_inertia_frame(
            inertia_request.clone(),
            0.25,
        );
    let apply_viewport_pan_inertia_frames_action =
        conformance::ConformanceAction::apply_viewport_pan_inertia_frames(
            inertia_request.clone(),
            [0.25, 0.5],
        );
    let assert_viewport_pan_inertia_action =
        conformance::ConformanceAction::assert_viewport_pan_inertia_frame(
            inertia_request,
            0.25,
            inertia_frame,
        );
    let expect_viewport_pan_inertia_rejected_action =
        conformance::ConformanceAction::expect_viewport_pan_inertia_rejected(
            viewport::ViewportPanInertiaRequest::new(
                viewport::ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
                CanvasPoint { x: 50.0, y: 0.0 },
                NodeGraphPanInertiaTuning {
                    enabled: true,
                    decay_per_s: 2.0,
                    min_speed: 100.0,
                    max_speed: 1000.0,
                },
            ),
        );
    let viewport_double_click_action =
        conformance::ConformanceAction::assert_viewport_double_click_zoom(
            viewport::ViewportDoubleClickZoomInput::new(
                viewport::ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
                CanvasPoint { x: 10.0, y: 10.0 },
                2.0,
                0.5,
                4.0,
                viewport::ViewportAnimationOptions::new(0.2),
            ),
            viewport::ViewportAnimationPlan {
                from: viewport::ViewportTransform::new(CanvasPoint::default(), 1.0)
                    .expect("viewport"),
                to: viewport::ViewportTransform::new(CanvasPoint { x: -5.0, y: -5.0 }, 2.0)
                    .expect("viewport"),
                duration_seconds: 0.2,
                easing: viewport::ViewportAnimationEasing::CubicInOut,
            },
        );
    let delete_key_action = conformance::ConformanceAction::apply_delete_selection_for_key(
        keyboard_types::Code::Backspace,
    );
    let assert_node_position_action = conformance::ConformanceAction::assert_node_position(
        node_id,
        CanvasPoint { x: 1.0, y: 2.0 },
    );
    let source_handle =
        connection::ConnectionHandleRef::new(node_id, PortId::new(), PortDirection::Out);
    let target_handle = connection::ConnectionTargetHandle::new(
        connection::ConnectionHandleRef::new(NodeId::new(), PortId::new(), PortDirection::In),
        true,
        true,
    );
    let connection_target_input = connection::ConnectionTargetInput::new(
        source_handle,
        Some(target_handle),
        NodeGraphConnectionMode::Strict,
        true,
    );
    let connection_target_action = conformance::ConformanceAction::assert_connection_target(
        connection_target_input,
        connection::ResolvedConnectionTarget {
            target: Some(target_handle),
            connection: Some(connection::ConnectionHandleConnection {
                source: source_handle,
                target: target_handle.handle,
            }),
            is_handle_valid: true,
            feedback: connection::ConnectionHandleValidity::Valid,
        },
    );
    let connection_target_candidate = connection::ConnectionTargetCandidate::new(
        target_handle,
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 100.0,
                height: 80.0,
            },
        },
        geometry::HandleBounds {
            rect: CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 10.0,
                    height: 10.0,
                },
            },
            position: geometry::HandlePosition::Right,
        },
    );
    let connection_target_candidates = [connection_target_candidate];
    let connection_target_from_handles_action =
        conformance::ConformanceAction::assert_connection_target_from_handles(
            connection::ConnectionTargetFromHandlesInput::new(
                CanvasPoint { x: 5.0, y: 5.0 },
                10.0,
                source_handle,
                &connection_target_candidates,
                NodeGraphConnectionMode::Strict,
            ),
            connection::ResolvedConnectionTarget {
                target: Some(target_handle),
                connection: Some(connection::ConnectionHandleConnection {
                    source: source_handle,
                    target: target_handle.handle,
                }),
                is_handle_valid: true,
                feedback: connection::ConnectionHandleValidity::Valid,
            },
        );
    let connect_action = conformance::ConformanceAction::apply_connect_edge(
        connection::ConnectEdgeRequest::new(
            PortId::new(),
            PortId::new(),
            NodeGraphConnectionMode::Strict,
        )
        .with_edge_id(EdgeId::new()),
    );
    let reconnect_action = conformance::ConformanceAction::apply_reconnect_edge(
        connection::ReconnectEdgeRequest::new(
            EdgeId::new(),
            EdgeEndpoint::To,
            PortId::new(),
            NodeGraphConnectionMode::Strict,
        ),
    );
    let dispatch_action = conformance::ConformanceAction::dispatch_transaction(
        GraphTransaction::new().with_label("low-level graph fixture setup"),
    );
    let resize_action = conformance::ConformanceAction::apply_node_resize(
        resize::NodeResizeRequest::new(
            node_id,
            CanvasSize {
                width: 2.0,
                height: 2.0,
            },
        )
        .with_direction(resize::NodeResizeDirection::BottomRight),
    );
    let pointer_resize_action = conformance::ConformanceAction::apply_node_pointer_resize(
        resize::NodePointerResizeRequest::new(
            node_id,
            CanvasPoint { x: 1.0, y: 1.0 },
            CanvasPoint { x: 2.0, y: 3.0 },
            resize::NodeResizeDirection::BottomRight,
        ),
    );
    let resize_start_gesture_action =
        conformance::ConformanceAction::emit_gesture(resize_start_event.clone());
    let resize_update_gesture_action =
        conformance::ConformanceAction::emit_gesture(resize_update_event.clone());
    let resize_end_gesture_action =
        conformance::ConformanceAction::emit_gesture(resize_end_event.clone());
    let encoded_fixture_actions = serde_json::to_value([
        viewport_scroll_action,
        viewport_reject_action,
        auto_pan_action,
        selection_auto_pan_action,
        viewport_pan_constrained_action,
        viewport_zoom_constrained_action,
        viewport_animation_action,
        apply_viewport_animation_action,
        apply_viewport_animation_frames_action,
        visible_node_ids_action,
        visible_node_render_order_action,
        apply_viewport_pan_inertia_action,
        apply_viewport_pan_inertia_frames_action,
        assert_viewport_pan_inertia_action,
        expect_viewport_pan_inertia_rejected_action,
        viewport_double_click_action,
        delete_key_action,
        assert_node_position_action,
        connection_target_action,
        connection_target_from_handles_action,
        connect_action,
        reconnect_action,
        dispatch_action,
        resize_action,
        pointer_resize_action,
        resize_start_gesture_action,
        resize_update_gesture_action,
        resize_end_gesture_action,
    ])
    .expect("serialize fixture actions");
    assert!(encoded_fixture_actions.is_array());
    let encoded_resize_trace = serde_json::to_value([
        conformance::ConformanceTraceEvent::gesture(resize_start_event.clone()),
        conformance::ConformanceTraceEvent::callback(
            conformance::ConformanceCallbackEvent::NodeResizeStart(resize_start.clone()),
        ),
        conformance::ConformanceTraceEvent::gesture(resize_update_event.clone()),
        conformance::ConformanceTraceEvent::callback(
            conformance::ConformanceCallbackEvent::NodeResize(resize_update.clone()),
        ),
        conformance::ConformanceTraceEvent::gesture(resize_end_event.clone()),
        conformance::ConformanceTraceEvent::callback(
            conformance::ConformanceCallbackEvent::NodeResizeEnd(resize_end),
        ),
    ])
    .expect("serialize resize trace events");
    assert!(encoded_resize_trace.is_array());

    let scenario = conformance::ConformanceScenario::new("public node drag fixture", graph)
        .with_view_state(NodeGraphViewState::default())
        .with_editor_config(NodeGraphEditorConfig::default())
        .with_trace_config(conformance::ConformanceTraceConfig::with_xyflow_callbacks())
        .with_node_drag_session_contract(conformance::ConformanceNodeDragSessionContract::new(
            node_id,
            drag_start.pointer,
            target,
        ));

    assert_eq!(
        scenario.schema_version,
        conformance::CONFORMANCE_FIXTURE_SCHEMA_VERSION,
    );
    assert!(scenario.setup.trace.record_xyflow_callbacks);
    assert!(scenario.actions.is_empty());
    assert_eq!(scenario.behaviors.len(), 1);
    assert_eq!(scenario.expanded_actions().len(), 1);
    assert!(!scenario.expanded_expected_trace().is_empty());
    let _ = std::mem::size_of::<conformance::ConformanceBehavior>();

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
