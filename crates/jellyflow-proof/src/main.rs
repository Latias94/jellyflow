fn main() {
    let trace = jellyflow_proof::proof_adapter_trace();
    println!("{}", jellyflow_proof::render_proof_trace(&trace));
}
