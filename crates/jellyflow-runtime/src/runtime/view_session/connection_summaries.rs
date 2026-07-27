use jellyflow_core::PortId;

use crate::runtime::graph_runtime::PortConnectionSummaryBatch;

use super::{ViewSession, ViewSessionError, WeakViewSession};

impl ViewSession {
    /// Resolves ordered port connection summaries under one authoritative runtime-state borrow.
    pub fn port_connection_summaries(
        &self,
        ports: &[PortId],
    ) -> Result<PortConnectionSummaryBatch, ViewSessionError> {
        self.runtime()?
            .port_connection_summaries(ports)
            .map_err(Into::into)
    }
}

impl WeakViewSession {
    /// Resolves ordered port connection summaries without reviving either capability.
    pub fn port_connection_summaries(
        &self,
        ports: &[PortId],
    ) -> Result<PortConnectionSummaryBatch, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.port_connection_summaries(ports).map_err(Into::into)
    }
}
