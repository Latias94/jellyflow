use serde::{Deserialize, Serialize};

use jellyflow_core::core::Graph;

use super::ConformanceScenarioCompiled;
use super::action::ConformanceAction;
use super::behavior::ConformanceBehavior;
use super::constants::default_schema_version;
use super::setup::{ConformanceSetup, ConformanceTraceConfig};
use super::trace::ConformanceTraceEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceScenario {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    setup: ConformanceSetup,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ConformanceAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behaviors: Vec<ConformanceBehavior>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_trace: Vec<ConformanceTraceEvent>,
}

impl ConformanceScenario {
    pub fn new(name: impl Into<String>, graph: Graph) -> Self {
        Self {
            schema_version: default_schema_version(),
            name: name.into(),
            setup: ConformanceSetup::from_graph(graph),
            actions: Vec::new(),
            behaviors: Vec::new(),
            expected_trace: Vec::new(),
        }
    }

    pub fn with_view_state(mut self, view_state: crate::io::NodeGraphViewState) -> Self {
        self.setup.view_state = view_state;
        self
    }

    pub fn with_editor_config(mut self, editor_config: crate::io::NodeGraphEditorConfig) -> Self {
        self.setup.editor_config = editor_config;
        self
    }

    pub fn with_xyflow_callbacks(mut self) -> Self {
        self.setup.trace = ConformanceTraceConfig::with_xyflow_callbacks();
        self
    }

    pub fn with_actions(mut self, actions: impl IntoIterator<Item = ConformanceAction>) -> Self {
        self.actions = actions.into_iter().collect();
        self
    }

    pub fn with_behaviors(
        mut self,
        behaviors: impl IntoIterator<Item = ConformanceBehavior>,
    ) -> Self {
        self.behaviors = behaviors.into_iter().collect();
        self
    }

    pub fn with_behavior(mut self, behavior: ConformanceBehavior) -> Self {
        self.behaviors.push(behavior);
        self
    }

    pub fn with_expected_trace(
        mut self,
        expected_trace: impl IntoIterator<Item = ConformanceTraceEvent>,
    ) -> Self {
        self.expected_trace = expected_trace.into_iter().collect();
        self
    }

    pub(crate) fn compiled(&self) -> ConformanceScenarioCompiled {
        ConformanceScenarioCompiled::from_scenario(self)
    }

    pub(crate) fn setup(&self) -> &ConformanceSetup {
        &self.setup
    }

    pub fn expanded_actions(&self) -> Vec<ConformanceAction> {
        self.compiled().actions().to_vec()
    }

    pub fn expanded_expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        self.compiled().expected_trace().to_vec()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuite {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<ConformanceScenario>,
}

impl ConformanceSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: default_schema_version(),
            name: name.into(),
            scenarios: Vec::new(),
        }
    }

    pub fn with_scenarios(
        mut self,
        scenarios: impl IntoIterator<Item = ConformanceScenario>,
    ) -> Self {
        self.scenarios = scenarios.into_iter().collect();
        self
    }

    pub fn push_scenario(&mut self, scenario: ConformanceScenario) {
        self.scenarios.push(scenario);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::conformance::ConformanceNodeDragSessionContract;
    use jellyflow_core::core::{CanvasPoint, Graph, GraphId, NodeId};

    #[test]
    fn compiled_scenario_strips_behavior_trace_suffix_from_approved_trace() {
        let node_id = NodeId::from_u128(1);
        let scenario = ConformanceScenario::new("compiled approval", Graph::new(GraphId::new()))
            .with_behavior(ConformanceBehavior::node_drag_session(
                ConformanceNodeDragSessionContract::new(
                    node_id,
                    CanvasPoint { x: 1.0, y: 2.0 },
                    CanvasPoint { x: 3.0, y: 4.0 },
                ),
            ));
        let compiled = scenario.compiled();
        let approved =
            compiled.approval_expected_trace(&scenario.expected_trace, compiled.expected_trace());

        assert!(scenario.expected_trace.is_empty());
        assert!(approved.is_empty());
        assert_eq!(
            compiled.expected_event_count(),
            compiled.expected_trace().len()
        );
        assert_eq!(compiled.actions().len(), 1);
    }
}
