use std::fmt;

use serde::{Deserialize, Serialize};

use super::capability::{ConformanceCapabilityGap, ConformanceCapabilityMatrix};
use super::scenario::ConformanceTraceEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceRunReport {
    pub scenario: String,
    pub actual_trace: Vec<ConformanceTraceEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<ConformanceTraceMismatch>,
}

impl ConformanceRunReport {
    pub fn new(
        scenario: impl Into<String>,
        actual_trace: Vec<ConformanceTraceEvent>,
        expected_trace: &[ConformanceTraceEvent],
    ) -> Self {
        let mismatches = trace_mismatches(expected_trace, &actual_trace);
        Self {
            scenario: scenario.into(),
            actual_trace,
            mismatches,
        }
    }

    pub fn is_match(&self) -> bool {
        self.mismatches.is_empty()
    }

    pub fn actual_trace(&self) -> &[ConformanceTraceEvent] {
        &self.actual_trace
    }

    pub fn mismatches(&self) -> &[ConformanceTraceMismatch] {
        &self.mismatches
    }
}

impl fmt::Display for ConformanceRunReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_match() {
            return write!(
                f,
                "conformance scenario `{}` matched {} trace events",
                self.scenario,
                self.actual_trace.len()
            );
        }

        writeln!(
            f,
            "conformance trace mismatch for scenario `{}` ({} mismatch(es))",
            self.scenario,
            self.mismatches.len()
        )?;
        for mismatch in self.mismatches.iter().take(8) {
            writeln!(
                f,
                "  [{}] expected: {:?}; actual: {:?}",
                mismatch.index, mismatch.expected, mismatch.actual
            )?;
        }
        if self.mismatches.len() > 8 {
            writeln!(f, "  ... {} more", self.mismatches.len() - 8)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteReport {
    pub suite: String,
    #[serde(default, skip_serializing_if = "ConformanceCapabilityMatrix::is_empty")]
    pub capabilities: ConformanceCapabilityMatrix,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_gaps: Vec<ConformanceCapabilityGap>,
    pub scenario_reports: Vec<ConformanceRunReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ConformanceRunError>,
}

impl ConformanceSuiteReport {
    pub fn is_match(&self) -> bool {
        self.errors.is_empty()
            && self.capability_gaps.is_empty()
            && self
                .scenario_reports
                .iter()
                .all(ConformanceRunReport::is_match)
    }

    pub fn failed_scenarios(&self) -> usize {
        self.errors.len()
            + self
                .scenario_reports
                .iter()
                .filter(|report| !report.is_match())
                .count()
    }

    pub fn scenario_count(&self) -> usize {
        self.scenario_reports.len() + self.errors.len()
    }
}

impl fmt::Display for ConformanceSuiteReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_match() {
            write!(
                f,
                "conformance suite `{}` matched {} scenario(s)",
                self.suite,
                self.scenario_count()
            )?;
            if let Some(adapter) = self.capabilities.adapter.as_deref() {
                write!(f, " for adapter `{adapter}`")?;
            }
            return Ok(());
        }

        writeln!(
            f,
            "conformance suite `{}` failed: {} scenario(s), {} execution error(s), {} capability gap(s)",
            self.suite,
            self.failed_scenarios(),
            self.errors.len(),
            self.capability_gaps.len()
        )?;
        for gap in self.capability_gaps.iter().take(8) {
            writeln!(
                f,
                "  capability {:?} required {:?}, actual {:?}",
                gap.capability, gap.required, gap.actual
            )?;
        }
        for report in self
            .scenario_reports
            .iter()
            .filter(|report| !report.is_match())
            .take(8)
        {
            writeln!(
                f,
                "  scenario `{}` mismatched {} trace event(s)",
                report.scenario,
                report.mismatches.len()
            )?;
        }
        for error in self.errors.iter().take(8) {
            writeln!(
                f,
                "  scenario `{}` errored at action {} ({}): {}",
                error.scenario, error.action_index, error.action_kind, error.message
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceTraceMismatch {
    pub index: usize,
    pub expected: Option<ConformanceTraceEvent>,
    pub actual: Option<ConformanceTraceEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error(
    "conformance scenario `{scenario}` failed at action {action_index} ({action_kind}): {message}"
)]
pub struct ConformanceRunError {
    pub scenario: String,
    pub action_index: usize,
    pub action_kind: String,
    pub message: String,
}

fn trace_mismatches(
    expected: &[ConformanceTraceEvent],
    actual: &[ConformanceTraceEvent],
) -> Vec<ConformanceTraceMismatch> {
    let len = expected.len().max(actual.len());
    (0..len)
        .filter_map(|index| {
            let expected = expected.get(index);
            let actual = actual.get(index);
            (expected != actual).then(|| ConformanceTraceMismatch {
                index,
                expected: expected.cloned(),
                actual: actual.cloned(),
            })
        })
        .collect()
}
