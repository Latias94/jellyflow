use jellyflow_core::PortId;

use crate::runtime::graph_runtime::{PortConnectionQueryResult, PortConnectionSummaryBatch};

use super::{ViewSession, ViewSessionError, WeakViewSession};

impl ViewSession {
    /// Resolves ordered summaries and exact work under one authoritative runtime-state borrow.
    pub fn port_connection_summaries(
        &self,
        ports: &[PortId],
    ) -> Result<PortConnectionQueryResult<PortConnectionSummaryBatch>, ViewSessionError> {
        self.runtime()?
            .port_connection_summaries(ports)
            .map_err(Into::into)
    }
}

impl WeakViewSession {
    /// Resolves ordered summaries and exact work without reviving either capability.
    pub fn port_connection_summaries(
        &self,
        ports: &[PortId],
    ) -> Result<PortConnectionQueryResult<PortConnectionSummaryBatch>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.port_connection_summaries(ports).map_err(Into::into)
    }
}
