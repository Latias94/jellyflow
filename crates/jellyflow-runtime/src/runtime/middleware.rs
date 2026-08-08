//! Middleware hooks for authoritative graph commands.
//!
//! This module is intentionally headless-safe (no `fret-ui` dependency).

use crate::profile::ApplyPipelineError;
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::{GraphSnapshot, HistoryState, RuntimePatch};

/// Owned graph-runtime state exposed to middleware without borrowing live runtime storage.
#[derive(Debug, Clone)]
pub struct GraphRuntimeMiddlewareSnapshot {
    graph: GraphSnapshot,
    history: HistoryState,
}

impl GraphRuntimeMiddlewareSnapshot {
    pub(crate) fn new(graph: GraphSnapshot, history: HistoryState) -> Self {
        Self { graph, history }
    }

    pub fn graph(&self) -> &GraphSnapshot {
        &self.graph
    }

    pub fn history(&self) -> HistoryState {
        self.history
    }
}

/// Runtime-owned command hooks used by every externally initiated graph mutation path.
pub trait GraphRuntimeMiddleware: 'static {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        Ok(())
    }

    fn after_command(&mut self, _snapshot: GraphRuntimeMiddlewareSnapshot, _patch: &RuntimePatch) {}
}
