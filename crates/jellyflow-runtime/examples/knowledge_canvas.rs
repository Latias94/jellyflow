use jellyflow_core::{
    Binding, BindingId, CanvasPoint, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow_runtime::NodeGraphStore;
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::runtime::binding::BindingEndpointResolution;
use jellyflow_runtime::runtime::layout::{
    LayoutEngineRequest, LayoutFamilyId, LayoutRequest, builtin_layout_engine_registry,
};
use jellyflow_runtime::runtime::measurement::NodeMeasurement;

fn make_note_node(pos: CanvasPoint) -> Node {
    Node {
        kind: NodeKindKey::new("knowledge.note"),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::json!({ "title": "Chapter note" }),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node = NodeId::from_u128(1);
    let binding = BindingId::from_u128(10);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(node, make_note_node(CanvasPoint { x: 160.0, y: 96.0 }));
    graph.insert_binding(
        binding,
        Binding::node_to_source(
            node,
            "source://paper.pdf",
            serde_json::json!({ "page": 12, "rect": [72, 144, 260, 190] }),
        )
        .with_kind("excerpt"),
    );

    let mut store = NodeGraphStore::new(
        graph.build_unchecked(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    store.report_node_measurement(NodeMeasurement::new(node).with_size(Some(CanvasSize {
        width: 220.0,
        height: 96.0,
    })))?;

    let binding_query = store.binding_query();
    let resolved = binding_query
        .binding(binding)
        .expect("binding should resolve");
    if let BindingEndpointResolution::NodeRect { center, .. } = resolved.subject.resolution {
        println!(
            "binding {:?} resolves graph node at ({:.1}, {:.1})",
            binding, center.x, center.y
        );
    }

    let context = store.layout_context_with_binding_pins();
    assert!(context.pinned_nodes.contains(&node));

    let registry = builtin_layout_engine_registry();
    let mind_map_engines = registry
        .engines_for_family(&LayoutFamilyId::mind_map())
        .map(|metadata| metadata.engine.as_str().to_owned())
        .collect::<Vec<_>>();
    println!("mind-map engines: {}", mind_map_engines.join(", "));

    let layout = store.plan_layout(
        &LayoutEngineRequest::mind_map_freeform(LayoutRequest::all()),
        &registry,
    )?;
    assert!(layout.node_position(node).is_some());

    Ok(())
}
