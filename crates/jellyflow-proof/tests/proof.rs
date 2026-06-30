use jellyflow::core::NodeKindKey;
use jellyflow::runtime::runtime::geometry::HandlePosition;
use jellyflow::runtime::runtime::measurement::{
    MeasuredSurfaceAnchor, MeasuredSurfaceSlot, NodeMeasurement,
};
use jellyflow::runtime::schema::NodeSurfaceSlotKind;

#[test]
fn proof_crate_exposes_a_second_adapter_boundary() {
    let registry = jellyflow_proof::proof_node_registry();
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("proof.review_card"))
        .expect("descriptor");

    assert_eq!(descriptor.renderer_key, "review-card");
    assert_eq!(descriptor.surface_slots.len(), 8);
}

#[test]
fn proof_component_tree_measurements_can_drive_runtime_layout_facts() {
    let mut store = jellyflow_proof::proof_store();
    let registry = jellyflow_proof::proof_node_registry();
    let proof = jellyflow_proof::component_tree_proof_from_graph(
        "dioxus-component-tree",
        store.graph(),
        &registry,
    );
    let node = proof
        .nodes
        .iter()
        .find(|node| node.kind == "proof.review_card")
        .expect("review card component proof");

    assert_eq!(proof.framework_family, "dioxus-component-tree");
    assert_eq!(node.renderer_key, "review-card");
    assert_eq!(
        node.component_key, "review-card::proof.review_card",
        "component keys should be stable and adapter-local"
    );
    assert!(
        node.children
            .iter()
            .any(|child| child.kind == NodeSurfaceSlotKind::ConfigGroup)
    );
    assert!(
        node.children
            .iter()
            .any(|child| child.anchor.as_deref() == Some("field.assignee"))
    );
    assert!(
        node.children
            .iter()
            .any(|child| child.key == "field.assignee" && child.control_count == 1)
    );
    assert!(
        node.children
            .iter()
            .any(|child| child.anchor.as_deref() == Some("field.check.policy.check"))
    );

    let slots = node
        .measurements
        .iter()
        .map(|measurement| MeasuredSurfaceSlot::new(measurement.key.clone(), measurement.rect));
    let anchors = node.measurements.iter().filter_map(|measurement| {
        let anchor = measurement.anchor.as_ref()?;
        Some(MeasuredSurfaceAnchor::new(
            anchor.clone(),
            measurement.rect,
            anchor_position_for(anchor),
        ))
    });
    store
        .report_node_measurement(
            NodeMeasurement::new(node.node_id)
                .with_slots(slots)
                .with_anchors(anchors),
        )
        .expect("component proof measurements are valid runtime measurements");

    let facts = store.layout_facts_query(jellyflow::core::CanvasSize {
        width: 640.0,
        height: 480.0,
    });
    assert!(facts.node_measurement_status(node.node_id).is_fresh());
    let measurement = store
        .node_measurement(node.node_id)
        .expect("measurement stored for proof node");
    assert_eq!(measurement.slots.len(), node.children.len());
    assert_eq!(
        measurement.anchors.len(),
        node.children
            .iter()
            .filter(|child| child.anchor.is_some())
            .count()
    );
}

#[test]
fn proof_component_tree_remeasurement_updates_dynamic_children() {
    let mut store = jellyflow_proof::proof_store();
    let registry = jellyflow_proof::proof_node_registry();
    let proof = jellyflow_proof::component_tree_proof_from_graph(
        "dioxus-component-tree",
        store.graph(),
        &registry,
    );
    let node = proof
        .nodes
        .iter()
        .find(|node| node.kind == "proof.review_card")
        .expect("review card component proof");
    let without_actions = node
        .measurements
        .iter()
        .filter(|measurement| measurement.key != "actions.primary")
        .cloned()
        .collect::<Vec<_>>();
    let slots = without_actions
        .iter()
        .map(|measurement| MeasuredSurfaceSlot::new(measurement.key.clone(), measurement.rect));
    let anchors = without_actions.iter().filter_map(|measurement| {
        let anchor = measurement.anchor.as_ref()?;
        Some(MeasuredSurfaceAnchor::new(
            anchor.clone(),
            measurement.rect,
            anchor_position_for(anchor),
        ))
    });

    store
        .report_node_measurement(
            NodeMeasurement::new(node.node_id)
                .with_revision(1)
                .with_slots(slots)
                .with_anchors(anchors),
        )
        .expect("initial reduced component measurement");
    let reduced = store
        .node_measurement(node.node_id)
        .expect("reduced measurement stored");
    assert_eq!(reduced.slots.len(), node.children.len() - 1);
    assert!(
        reduced
            .anchors
            .iter()
            .all(|anchor| anchor.anchor != "actions.primary")
    );

    let slots = node
        .measurements
        .iter()
        .map(|measurement| MeasuredSurfaceSlot::new(measurement.key.clone(), measurement.rect));
    let anchors = node.measurements.iter().filter_map(|measurement| {
        let anchor = measurement.anchor.as_ref()?;
        Some(MeasuredSurfaceAnchor::new(
            anchor.clone(),
            measurement.rect,
            anchor_position_for(anchor),
        ))
    });
    store
        .report_node_measurement(
            NodeMeasurement::new(node.node_id)
                .with_revision(2)
                .with_slots(slots)
                .with_anchors(anchors),
        )
        .expect("full component remeasurement");

    let facts = store.layout_facts_query(jellyflow::core::CanvasSize {
        width: 640.0,
        height: 480.0,
    });
    assert!(facts.node_measurement_status(node.node_id).is_fresh());
    let full = store
        .node_measurement(node.node_id)
        .expect("full measurement stored");
    assert_eq!(full.slots.len(), node.children.len());
    assert!(
        full.anchors
            .iter()
            .any(|anchor| anchor.anchor == "actions.primary")
    );
}

fn anchor_position_for(anchor: &str) -> HandlePosition {
    if anchor == "actions.primary" {
        HandlePosition::Right
    } else {
        HandlePosition::Left
    }
}
