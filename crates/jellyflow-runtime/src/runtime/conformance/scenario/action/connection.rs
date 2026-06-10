use serde::{Deserialize, Serialize};

use crate::runtime::connection::{
    ConnectEdgeRequest, ConnectionTargetCandidate, ConnectionTargetFromHandlesInput,
    ConnectionTargetInput, ReconnectEdgeRequest, ResolvedConnectionTarget,
};
use jellyflow_core::core::CanvasPoint;
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::ConformanceAction;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceConnectionTargetFromHandlesInput {
    pub pointer: CanvasPoint,
    pub radius: f32,
    pub from: crate::runtime::connection::ConnectionHandleRef,
    pub candidates: Vec<ConnectionTargetCandidate>,
    pub mode: NodeGraphConnectionMode,
    #[serde(default = "default_connection_validity")]
    pub is_valid_connection: bool,
}

impl ConformanceConnectionTargetFromHandlesInput {
    pub(crate) fn as_runtime(&self) -> ConnectionTargetFromHandlesInput<'_> {
        ConnectionTargetFromHandlesInput::new(
            self.pointer,
            self.radius,
            self.from,
            &self.candidates,
            self.mode,
        )
        .with_connection_validity(self.is_valid_connection)
    }

    fn from_runtime(input: ConnectionTargetFromHandlesInput<'_>) -> Self {
        Self {
            pointer: input.pointer,
            radius: input.radius,
            from: input.from,
            candidates: input.candidates.to_vec(),
            mode: input.mode,
            is_valid_connection: input.is_valid_connection,
        }
    }
}

fn default_connection_validity() -> bool {
    true
}

impl ConformanceAction {
    pub fn assert_connection_target(
        input: ConnectionTargetInput,
        expected: ResolvedConnectionTarget,
    ) -> Self {
        Self::AssertConnectionTarget { input, expected }
    }

    pub fn assert_connection_target_from_handles(
        input: ConnectionTargetFromHandlesInput<'_>,
        expected: ResolvedConnectionTarget,
    ) -> Self {
        Self::AssertConnectionTargetFromHandles {
            input: ConformanceConnectionTargetFromHandlesInput::from_runtime(input),
            expected,
        }
    }

    pub fn apply_connect_edge(request: ConnectEdgeRequest) -> Self {
        Self::ApplyConnectEdge { request }
    }

    pub fn apply_reconnect_edge(request: ReconnectEdgeRequest) -> Self {
        Self::ApplyReconnectEdge { request }
    }
}
