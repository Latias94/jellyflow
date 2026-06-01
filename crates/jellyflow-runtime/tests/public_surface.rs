use jellyflow_core::core::{CanvasPoint, Graph, GraphId, NodeId};
use jellyflow_core::ops::GraphTransaction;
use jellyflow_runtime::io::{
    GraphFileV1, NodeGraphEditorConfig, NodeGraphEditorStateFile, NodeGraphInteractionConfig,
    NodeGraphViewState,
};
use jellyflow_runtime::profile::{ApplyPipelineError, GraphProfile as ModuleGraphProfile};
use jellyflow_runtime::rules::ConnectPlan;
use jellyflow_runtime::runtime::{commit, drag, events, selection, store, xyflow};
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
