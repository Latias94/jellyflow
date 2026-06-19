use jellyflow::core::NodeKindKey;

#[test]
fn proof_crate_exposes_a_second_adapter_boundary() {
    let registry = jellyflow_proof::proof_node_registry();
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("proof.review_card"))
        .expect("descriptor");

    assert_eq!(descriptor.renderer_key, "review-card");
    assert_eq!(descriptor.surface_slots.len(), 5);
}
