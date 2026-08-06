use serde::Serialize;

use super::{SmokeOutcome, SmokeScenarioEvidence, SmokeState};

pub(super) const SMOKE_RECEIPT_SCHEMA_VERSION: u16 = 1;
pub(super) const SMOKE_RECEIPT_PREFIX: &str = "JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT ";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SmokeReceiptStatusV1 {
    Pass,
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct SmokeFailureV1 {
    pub(super) code: String,
    pub(super) message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct SmokeRunReceiptV1 {
    pub(super) schema_version: u16,
    pub(super) run_id: String,
    pub(super) status: SmokeReceiptStatusV1,
    pub(super) failure: Option<SmokeFailureV1>,
    pub(super) artifacts: Vec<String>,
}

pub(super) fn take_run_receipt(state: &mut SmokeState) -> Option<SmokeRunReceiptV1> {
    if state.receipt_emitted || state.outcome == SmokeOutcome::Pending {
        return None;
    }
    state.receipt_emitted = true;

    let artifacts = captured_artifacts(state);
    let failure = match state.outcome {
        SmokeOutcome::Failed(code) => Some(SmokeFailureV1 {
            code: code.to_owned(),
            message: failure_message(state),
        }),
        SmokeOutcome::Passed | SmokeOutcome::Pending => None,
    };
    Some(SmokeRunReceiptV1 {
        schema_version: SMOKE_RECEIPT_SCHEMA_VERSION,
        run_id: run_id(state),
        status: match state.outcome {
            SmokeOutcome::Passed => SmokeReceiptStatusV1::Pass,
            SmokeOutcome::Failed(_) => SmokeReceiptStatusV1::Fail,
            SmokeOutcome::Pending => unreachable!("pending outcomes do not emit receipts"),
        },
        failure,
        artifacts,
    })
}

fn run_id(state: &SmokeState) -> String {
    format!(
        "{}-{}-{}-z{}",
        state.config.scenario().label(),
        state.demo.label(),
        state.config.bounds().label(),
        state.config.zoom().file_label(),
    )
}

fn captured_artifacts(state: &SmokeState) -> Vec<String> {
    let Some(input) = state
        .last_observation
        .as_ref()
        .and_then(|observation| observation.input.as_ref())
    else {
        return Vec::new();
    };
    let captures = match &input.scenario {
        SmokeScenarioEvidence::Interaction(evidence) => &evidence.captures,
        SmokeScenarioEvidence::LodThreshold(evidence) => &evidence.captures,
    };
    captures
        .iter()
        .filter_map(|capture| state.config.capture_path(state.demo, *capture))
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

fn failure_message(state: &SmokeState) -> Option<String> {
    state.last_observation.as_ref().and_then(|observation| {
        observation
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.message.clone())
            .or_else(|| observation.native_probe_phase.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_serialization_is_deterministic_and_transport_only() {
        let receipt = SmokeRunReceiptV1 {
            schema_version: SMOKE_RECEIPT_SCHEMA_VERSION,
            run_id: "interaction-shader-compact-z1".to_owned(),
            status: SmokeReceiptStatusV1::Pass,
            failure: None,
            artifacts: vec!["target/capture.png".to_owned()],
        };

        assert_eq!(
            serde_json::to_string(&receipt).expect("receipt serializes"),
            r#"{"schema_version":1,"run_id":"interaction-shader-compact-z1","status":"pass","failure":null,"artifacts":["target/capture.png"]}"#,
        );
    }
}
