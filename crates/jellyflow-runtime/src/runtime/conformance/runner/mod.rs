use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::store::NodeGraphStore;

use super::reports::{ConformanceRunError, ConformanceRunReport, ConformanceSuiteReport};
use super::scenario::{ConformanceScenario, ConformanceSuite};

mod actions;
mod trace;

use actions::execute_action;
use trace::install_trace_recorders;

impl ConformanceSuite {
    pub fn run(&self) -> ConformanceSuiteReport {
        run_conformance_suite(self)
    }
}

pub fn run_conformance_scenario(
    scenario: &ConformanceScenario,
) -> Result<ConformanceRunReport, ConformanceRunError> {
    ConformanceRunner::new(scenario).run()
}

pub fn run_conformance_suite(suite: &ConformanceSuite) -> ConformanceSuiteReport {
    let mut scenario_reports = Vec::new();
    let mut errors = Vec::new();

    for scenario in &suite.scenarios {
        match run_conformance_scenario(scenario) {
            Ok(report) => scenario_reports.push(report),
            Err(error) => errors.push(error),
        }
    }

    ConformanceSuiteReport {
        suite: suite.name.clone(),
        scenario_reports,
        errors,
    }
}

#[derive(Debug)]
pub struct ConformanceRunner<'a> {
    scenario: &'a ConformanceScenario,
}

impl<'a> ConformanceRunner<'a> {
    pub fn new(scenario: &'a ConformanceScenario) -> Self {
        Self { scenario }
    }

    pub fn run(&self) -> Result<ConformanceRunReport, ConformanceRunError> {
        let compiled = self.scenario.compiled();
        let setup = self.scenario.setup();
        let mut store = NodeGraphStore::new(
            setup.graph.clone(),
            setup.view_state.clone(),
            setup.editor_config.clone(),
        );
        let trace = Rc::new(RefCell::new(Vec::new()));
        install_trace_recorders(&mut store, setup.trace, trace.clone());

        for (index, action) in compiled.actions().iter().enumerate() {
            execute_action(&mut store, action).map_err(|message| ConformanceRunError {
                scenario: self.scenario.name.clone(),
                action_index: index,
                action_kind: action.kind().to_owned(),
                message,
            })?;
        }

        Ok(ConformanceRunReport::new(
            self.scenario.name.clone(),
            trace.borrow().clone(),
            compiled.expected_trace(),
        ))
    }
}
