use crate::runtime::conformance::scenario::trace::ConformanceTraceEvent;
use crate::runtime::conformance::scenario::{ConformanceAction, ConformanceBehavior};

use super::ConformanceScenario;

#[derive(Debug, Clone)]
pub(crate) struct ConformanceScenarioCompiled {
    actions: Vec<ConformanceAction>,
    expected_trace: Vec<ConformanceTraceEvent>,
    behavior_expected_trace: Vec<ConformanceTraceEvent>,
}

impl ConformanceScenarioCompiled {
    pub(crate) fn from_scenario(scenario: &ConformanceScenario) -> Self {
        let mut actions = scenario.actions.clone();
        actions.extend(
            scenario
                .behaviors
                .iter()
                .flat_map(ConformanceBehavior::actions),
        );

        let behavior_expected_trace = scenario
            .behaviors
            .iter()
            .flat_map(ConformanceBehavior::expected_trace)
            .collect::<Vec<_>>();
        let mut expected_trace = scenario.expected_trace.clone();
        expected_trace.extend(behavior_expected_trace.iter().cloned());

        Self {
            actions,
            expected_trace,
            behavior_expected_trace,
        }
    }

    pub(crate) fn actions(&self) -> &[ConformanceAction] {
        &self.actions
    }

    pub(crate) fn expected_trace(&self) -> &[ConformanceTraceEvent] {
        &self.expected_trace
    }

    pub(crate) fn expected_event_count(&self) -> usize {
        self.expected_trace.len()
    }

    pub(crate) fn approval_expected_trace(
        &self,
        authored_expected_trace: &[ConformanceTraceEvent],
        actual_trace: &[ConformanceTraceEvent],
    ) -> Vec<ConformanceTraceEvent> {
        if self.behavior_expected_trace.is_empty() {
            return actual_trace.to_vec();
        }

        if actual_trace.ends_with(&self.behavior_expected_trace) {
            return actual_trace[..actual_trace.len() - self.behavior_expected_trace.len()]
                .to_vec();
        }

        authored_expected_trace.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::conformance::ConformanceNodeDragSessionContract;
    use jellyflow_core::core::{CanvasPoint, Graph, GraphId, NodeId};

    #[test]
    fn compiled_scenario_falls_back_to_authored_expected_trace_when_suffix_is_missing() {
        let node_id = NodeId::from_u128(1);
        let scenario = ConformanceScenario::new("compiled fallback", Graph::new(GraphId::new()))
            .with_expected_trace([ConformanceTraceEvent::graph_commit(
                Some("authored"),
                ["keep"],
            )])
            .with_behavior(ConformanceBehavior::node_drag_session(
                ConformanceNodeDragSessionContract::new(
                    node_id,
                    CanvasPoint { x: 1.0, y: 2.0 },
                    CanvasPoint { x: 3.0, y: 4.0 },
                ),
            ));
        let compiled = scenario.compiled();
        let actual_trace = [ConformanceTraceEvent::viewport(
            CanvasPoint { x: 9.0, y: 9.0 },
            1.0,
        )];

        let approved = compiled.approval_expected_trace(&scenario.expected_trace, &actual_trace);

        assert_eq!(approved, scenario.expected_trace);
        assert_eq!(compiled.actions().len(), 1);
        assert!(!compiled.expected_trace().is_empty());
    }
}
