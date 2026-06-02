//! Headless conformance fixture vocabulary for runtime and adapter checks.
//!
//! These types describe renderer-free scenarios that can be replayed against the runtime store.

mod approval;
mod fixtures;
mod reports;
mod runner;
mod scenario;

pub use approval::{
    ConformanceApprovalError, ConformanceFixtureDirectoryApprovalReport,
    ConformanceScenarioApprovalReport, ConformanceSuiteApproval, ConformanceSuiteApprovalReport,
    ConformanceSuiteFileApprovalReport,
};
pub use fixtures::{
    ConformanceFixtureDirectory, ConformanceFixtureDirectoryReport, ConformanceFixtureFileError,
    ConformanceSuiteFile, ConformanceSuiteFileReport,
};
pub use reports::{
    ConformanceRunError, ConformanceRunReport, ConformanceSuiteReport, ConformanceTraceMismatch,
};
pub use runner::{ConformanceRunner, run_conformance_scenario, run_conformance_suite};
#[cfg(test)]
pub(crate) use scenario::ConformanceCallbackTraceRecorder;
pub use scenario::{
    CONFORMANCE_FIXTURE_SCHEMA_VERSION, ConformanceAction, ConformanceCallbackEvent,
    ConformanceScenario, ConformanceSetup, ConformanceSuite, ConformanceTraceConfig,
    ConformanceTraceEvent, ConformanceViewChange,
};
