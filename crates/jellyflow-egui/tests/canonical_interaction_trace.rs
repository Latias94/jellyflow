#[path = "../../jellyflow-runtime/tests/support/canonical_interaction_trace.rs"]
mod trace;

use trace::{TraceReceipt, TraceReconnectEndpoint, new_connection_trace, reconnect_trace};

#[test]
fn egui_compiles_the_renderer_neutral_trace_contract() {
    let connection = new_connection_trace(TraceReceipt::Applied);
    let reconnect = reconnect_trace(TraceReconnectEndpoint::Target, TraceReceipt::Conflict);

    assert_eq!(connection.name, "new-connection");
    assert_eq!(reconnect.name, "reconnect-target");
}
