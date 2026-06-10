use serde::{Deserialize, Serialize};

use jellyflow_core::core::Graph;

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
    pub setup: ConformanceSetup,
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

    pub fn with_setup(mut self, setup: ConformanceSetup) -> Self {
        self.setup = setup;
        self
    }

    pub fn with_view_state(mut self, view_state: crate::io::NodeGraphViewState) -> Self {
        self.setup.view_state = view_state;
        self
    }

    pub fn with_editor_config(mut self, editor_config: crate::io::NodeGraphEditorConfig) -> Self {
        self.setup.editor_config = editor_config;
        self
    }

    pub fn with_trace_config(mut self, trace: ConformanceTraceConfig) -> Self {
        self.setup.trace = trace;
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

    pub fn expanded_actions(&self) -> Vec<ConformanceAction> {
        let mut actions = self.actions.clone();
        actions.extend(self.behaviors.iter().flat_map(ConformanceBehavior::actions));
        actions
    }

    pub fn expanded_expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        let mut expected_trace = self.expected_trace.clone();
        expected_trace.extend(self.behavior_expected_trace());
        expected_trace
    }

    pub(crate) fn approval_expected_trace(
        &self,
        actual_trace: &[ConformanceTraceEvent],
    ) -> Vec<ConformanceTraceEvent> {
        let behavior_expected_trace = self.behavior_expected_trace();
        if behavior_expected_trace.is_empty() {
            return actual_trace.to_vec();
        }

        if actual_trace.ends_with(&behavior_expected_trace) {
            return actual_trace[..actual_trace.len() - behavior_expected_trace.len()].to_vec();
        }

        self.expected_trace.clone()
    }

    fn behavior_expected_trace(&self) -> Vec<ConformanceTraceEvent> {
        self.behaviors
            .iter()
            .flat_map(ConformanceBehavior::expected_trace)
            .collect()
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
