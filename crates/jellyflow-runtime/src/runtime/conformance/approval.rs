use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::fixtures::{
    ConformanceFixtureDirectory, ConformanceFixtureFileError, ConformanceSuiteFile,
};
use super::reports::{ConformanceRunError, ConformanceTraceMismatch};
use super::runner::run_conformance_scenario;
use super::scenario::ConformanceSuite;

impl ConformanceSuite {
    pub fn approve_actual_traces(&self) -> ConformanceSuiteApproval {
        let mut approved = self.clone();
        let mut scenario_reports = Vec::new();
        let mut errors = Vec::new();

        for scenario in &mut approved.scenarios {
            let compiled = scenario.compiled();
            match run_conformance_scenario(scenario) {
                Ok(report) => {
                    let expected_event_count = compiled.expected_event_count();
                    let actual_event_count = report.actual_trace.len();
                    let approved_expected_trace = compiled
                        .approval_expected_trace(&scenario.expected_trace, &report.actual_trace);
                    let changed = scenario.expected_trace != approved_expected_trace;
                    scenario.expected_trace = approved_expected_trace;
                    scenario_reports.push(ConformanceScenarioApprovalReport {
                        scenario: scenario.name.clone(),
                        changed,
                        expected_event_count,
                        actual_event_count,
                        mismatches: report.mismatches,
                    });
                }
                Err(error) => errors.push(error),
            }
        }

        ConformanceSuiteApproval {
            suite: approved,
            report: ConformanceSuiteApprovalReport {
                suite: self.name.clone(),
                scenario_reports,
                errors,
            },
        }
    }
}

impl ConformanceFixtureDirectory {
    pub fn approve_actual_traces_to_json(
        &self,
    ) -> Result<ConformanceFixtureDirectoryApprovalReport, ConformanceFixtureFileError> {
        let mut approvals = Vec::new();
        for file in &self.files {
            let approval = file.suite.approve_actual_traces();
            if !approval.is_approvable() {
                return Err(ConformanceFixtureFileError::Approve {
                    path: file.path.display().to_string(),
                    source: ConformanceApprovalError::from_report(approval.report),
                });
            }
            approvals.push((file.path.clone(), approval));
        }

        for (path, approval) in &approvals {
            approval.suite.save_json(path)?;
        }

        Ok(ConformanceFixtureDirectoryApprovalReport {
            root: self.root.clone(),
            reports: approvals
                .into_iter()
                .map(|(path, approval)| ConformanceSuiteFileApprovalReport {
                    path,
                    report: approval.report,
                })
                .collect(),
        })
    }
}

impl ConformanceSuiteFile {
    pub fn approve_actual_traces_to_json(
        &self,
    ) -> Result<ConformanceSuiteApproval, ConformanceFixtureFileError> {
        let approval = self.suite.approve_actual_traces();
        if !approval.is_approvable() {
            return Err(ConformanceFixtureFileError::Approve {
                path: self.path.display().to_string(),
                source: ConformanceApprovalError::from_report(approval.report),
            });
        }
        approval.suite.save_json(&self.path)?;
        Ok(approval)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteApproval {
    pub suite: ConformanceSuite,
    pub report: ConformanceSuiteApprovalReport,
}

impl ConformanceSuiteApproval {
    pub fn is_approvable(&self) -> bool {
        self.report.is_approvable()
    }

    pub fn has_changes(&self) -> bool {
        self.report.has_changes()
    }

    pub fn changed_scenarios(&self) -> usize {
        self.report.changed_scenarios()
    }

    pub fn error_count(&self) -> usize {
        self.report.error_count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteApprovalReport {
    pub suite: String,
    pub scenario_reports: Vec<ConformanceScenarioApprovalReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ConformanceRunError>,
}

impl ConformanceSuiteApprovalReport {
    pub fn is_approvable(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_changes(&self) -> bool {
        self.scenario_reports.iter().any(|report| report.changed)
    }

    pub fn changed_scenarios(&self) -> usize {
        self.scenario_reports
            .iter()
            .filter(|report| report.changed)
            .count()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceScenarioApprovalReport {
    pub scenario: String,
    pub changed: bool,
    pub expected_event_count: usize,
    pub actual_event_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<ConformanceTraceMismatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteFileApprovalReport {
    pub path: PathBuf,
    pub report: ConformanceSuiteApprovalReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceFixtureDirectoryApprovalReport {
    pub root: PathBuf,
    pub reports: Vec<ConformanceSuiteFileApprovalReport>,
}

impl ConformanceFixtureDirectoryApprovalReport {
    pub fn is_approvable(&self) -> bool {
        self.reports
            .iter()
            .all(|report| report.report.is_approvable())
    }

    pub fn has_changes(&self) -> bool {
        self.reports
            .iter()
            .any(|report| report.report.has_changes())
    }

    pub fn file_count(&self) -> usize {
        self.reports.len()
    }

    pub fn changed_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| report.report.has_changes())
            .count()
    }

    pub fn changed_scenarios(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.changed_scenarios())
            .sum()
    }

    pub fn error_count(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.error_count())
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("failed to approve conformance suite `{suite}` because {error_count} scenario(s) errored")]
pub struct ConformanceApprovalError {
    pub suite: String,
    pub error_count: usize,
    pub errors: Vec<ConformanceRunError>,
}

impl ConformanceApprovalError {
    pub fn from_report(report: ConformanceSuiteApprovalReport) -> Self {
        Self {
            suite: report.suite,
            error_count: report.errors.len(),
            errors: report.errors,
        }
    }
}
