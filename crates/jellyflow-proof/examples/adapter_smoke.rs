fn main() {
    let registry = jellyflow_proof::proof_node_registry();
    let descriptor = registry
        .view_descriptor(&jellyflow::core::NodeKindKey::new("proof.review_card"))
        .expect("descriptor");
    println!(
        "{} -> {} slot(s)",
        descriptor.title,
        descriptor.surface_slots.len()
    );
}
