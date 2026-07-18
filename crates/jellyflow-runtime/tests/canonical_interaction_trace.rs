#[path = "support/canonical_interaction_trace.rs"]
mod trace;

use trace::{
    TraceEffect, TracePhase, TraceReceipt, TraceReconnectEndpoint, control_and_drag_handle_traces,
    create_from_wire_trace, edge_pick_trace, new_connection_trace, reconnect_trace,
    terminal_ownership_traces,
};

#[test]
fn canonical_trace_vocabulary_is_deterministic_and_covers_terminal_receipts() {
    for receipt in [
        TraceReceipt::Applied,
        TraceReceipt::Noop,
        TraceReceipt::Rejected,
        TraceReceipt::Conflict,
    ] {
        let trace = new_connection_trace(receipt);
        assert_eq!(trace, new_connection_trace(receipt));
        assert_eq!(
            trace.steps.last().expect("terminal step").phase,
            TracePhase::Idle
        );
        assert_eq!(
            trace
                .steps
                .iter()
                .filter(|step| step.effect == TraceEffect::SubmitConnection)
                .count(),
            1,
        );

        for endpoint in [
            TraceReconnectEndpoint::Source,
            TraceReconnectEndpoint::Target,
        ] {
            let reconnect = reconnect_trace(endpoint, receipt);
            assert_eq!(
                reconnect
                    .steps
                    .iter()
                    .filter(|step| step.effect == TraceEffect::SubmitConnection)
                    .count(),
                1,
            );
        }
    }
}

#[test]
fn search_and_native_control_traces_keep_terminal_ownership_explicit() {
    let rejected = create_from_wire_trace(TraceReceipt::Rejected);
    assert_eq!(
        rejected.steps.last().expect("terminal search step").phase,
        TracePhase::Searching,
    );

    let [control, drag] = control_and_drag_handle_traces();
    assert_eq!(control.steps[0].phase, TracePhase::Idle);
    assert!(
        drag.steps
            .iter()
            .any(|step| step.phase == TracePhase::NodeDrag)
    );

    let edge = edge_pick_trace();
    assert_eq!(edge.steps[0].effect, TraceEffect::PickEdge);
    assert_eq!(
        edge.steps.last().expect("edge terminal").phase,
        TracePhase::Idle,
    );

    for trace in terminal_ownership_traces() {
        let terminal = trace.steps.last().expect("ownership terminal");
        assert_eq!(terminal.phase, TracePhase::Idle);
        assert_eq!(terminal.effect, TraceEffect::ReleaseNativeOwnership);
    }
}
