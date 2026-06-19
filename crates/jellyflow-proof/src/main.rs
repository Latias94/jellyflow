fn main() {
    let graph = jellyflow_proof::proof_graph();
    println!(
        "proof graph: {} node(s), {} port(s)",
        graph.nodes().len(),
        graph.ports().len()
    );
}
