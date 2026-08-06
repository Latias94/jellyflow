use std::collections::BTreeMap;
use std::sync::Arc;

use jellyflow_core::PortId;
use jellyflow_runtime::graph::Diagnostic;

/// Adapter-local level of detail used to present one native node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OpenGpuiNodeLod {
    /// The ordinary native component tree is visible and graph-interactive.
    #[default]
    Detail,
    /// Canonical lightweight graph chrome replaces the native component tree.
    Overview,
    /// Native controls remain visible at fixed logical pixels while graph geometry stays compact.
    PinnedDetail,
}

impl OpenGpuiNodeLod {
    /// Returns whether the ordinary graph-interactive detail presentation is active.
    #[must_use]
    pub const fn is_detail(self) -> bool {
        matches!(self, Self::Detail)
    }

    /// Returns whether native controls remain mounted for this presentation.
    #[must_use]
    pub const fn retains_native_controls(self) -> bool {
        matches!(self, Self::Detail | Self::PinnedDetail)
    }

    /// Returns whether this presentation may publish graph-interactive native geometry.
    #[must_use]
    pub const fn publishes_native_graph_geometry(self) -> bool {
        matches!(self, Self::Detail)
    }
}

/// Adapter-local command-settlement state visible in graph chrome.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OpenGpuiSettlementState {
    /// No command owned by this presentation is waiting for a receipt.
    #[default]
    Idle,
    /// Optimistic presentation remains visible while a command awaits settlement.
    Pending,
}

impl OpenGpuiSettlementState {
    /// Returns whether optimistic presentation is awaiting an authoritative receipt.
    #[must_use]
    pub const fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }
}

/// Composable adapter-local interaction state for one native node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OpenGpuiNodeInteractionState {
    selected: bool,
    hovered: bool,
    dragging: bool,
    focused: bool,
    disabled: bool,
    lod: OpenGpuiNodeLod,
    settlement: OpenGpuiSettlementState,
}

impl OpenGpuiNodeInteractionState {
    /// Returns an idle detail presentation.
    #[must_use]
    pub const fn idle() -> Self {
        Self {
            selected: false,
            hovered: false,
            dragging: false,
            focused: false,
            disabled: false,
            lod: OpenGpuiNodeLod::Detail,
            settlement: OpenGpuiSettlementState::Idle,
        }
    }

    #[must_use]
    pub const fn selected(self) -> bool {
        self.selected
    }

    #[must_use]
    pub const fn hovered(self) -> bool {
        self.hovered
    }

    #[must_use]
    pub const fn dragging(self) -> bool {
        self.dragging
    }

    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn disabled(self) -> bool {
        self.disabled
    }

    #[must_use]
    pub const fn lod(self) -> OpenGpuiNodeLod {
        self.lod
    }

    #[must_use]
    pub const fn settlement(self) -> OpenGpuiSettlementState {
        self.settlement
    }

    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub const fn with_hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    #[must_use]
    pub const fn with_dragging(mut self, dragging: bool) -> Self {
        self.dragging = dragging;
        self
    }

    #[must_use]
    pub const fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub const fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub const fn with_lod(mut self, lod: OpenGpuiNodeLod) -> Self {
        self.lod = lod;
        self
    }

    #[must_use]
    pub const fn with_settlement(mut self, settlement: OpenGpuiSettlementState) -> Self {
        self.settlement = settlement;
        self
    }

    /// Returns concise state text suitable for an accessible description.
    #[must_use]
    pub fn accessible_state_text(self) -> String {
        let mut states = Vec::with_capacity(5);
        if self.selected {
            states.push("selected");
        }
        if self.dragging {
            states.push("dragging");
        }
        if self.disabled {
            states.push("disabled");
        }
        match self.lod {
            OpenGpuiNodeLod::Detail => {}
            OpenGpuiNodeLod::Overview => states.push("overview"),
            OpenGpuiNodeLod::PinnedDetail => states.push("native detail pinned"),
        }
        if self.settlement.is_pending() {
            states.push("pending");
        }
        accessible_state_text(states)
    }
}

/// Connection-candidate role currently presented by one port.
#[derive(Debug, Clone, Default)]
pub enum OpenGpuiPortCandidateState {
    /// The port is not participating in the active connection gesture.
    #[default]
    Idle,
    /// The port owns the fixed end of the active connection gesture.
    Source,
    /// The port is the current compatible connection target.
    ValidTarget,
    /// The port is the current incompatible connection target.
    InvalidTarget {
        /// Structured runtime diagnostics explaining the rejection.
        diagnostics: Arc<[Diagnostic]>,
    },
}

impl OpenGpuiPortCandidateState {
    /// Builds an invalid candidate while retaining structured runtime diagnostics.
    #[must_use]
    pub fn invalid(diagnostics: impl Into<Arc<[Diagnostic]>>) -> Self {
        Self::InvalidTarget {
            diagnostics: diagnostics.into(),
        }
    }

    #[must_use]
    pub const fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    #[must_use]
    pub const fn is_source(&self) -> bool {
        matches!(self, Self::Source)
    }

    #[must_use]
    pub const fn is_valid_target(&self) -> bool {
        matches!(self, Self::ValidTarget)
    }

    #[must_use]
    pub const fn is_invalid_target(&self) -> bool {
        matches!(self, Self::InvalidTarget { .. })
    }

    /// Returns every structured rejection diagnostic for an invalid target.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::InvalidTarget { diagnostics } => diagnostics,
            Self::Idle | Self::Source | Self::ValidTarget => &[],
        }
    }

    /// Returns the first structured diagnostic used as the primary rejection reason.
    #[must_use]
    pub fn rejection_reason(&self) -> Option<&Diagnostic> {
        self.diagnostics().first()
    }

    /// Returns the primary human-readable rejection message.
    #[must_use]
    pub fn rejection_text(&self) -> Option<&str> {
        self.rejection_reason()
            .map(|diagnostic| diagnostic.message.as_str())
    }

    /// Returns concise state text suitable for an accessible description.
    #[must_use]
    pub fn accessible_state_text(&self) -> String {
        match self {
            Self::Idle => "idle".to_owned(),
            Self::Source => "connection source".to_owned(),
            Self::ValidTarget => "compatible connection target".to_owned(),
            Self::InvalidTarget { .. } => self.rejection_text().map_or_else(
                || "incompatible connection target".to_owned(),
                |reason| format!("incompatible connection target: {reason}"),
            ),
        }
    }
}

impl PartialEq for OpenGpuiPortCandidateState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Idle, Self::Idle)
            | (Self::Source, Self::Source)
            | (Self::ValidTarget, Self::ValidTarget) => true,
            (
                Self::InvalidTarget { diagnostics: left },
                Self::InvalidTarget { diagnostics: right },
            ) => diagnostics_equal(left, right),
            _ => false,
        }
    }
}

/// Composable adapter-local interaction state for one real graph port.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpenGpuiPortInteractionState {
    hovered: bool,
    connected: bool,
    focused: bool,
    disabled: bool,
    candidate: OpenGpuiPortCandidateState,
    settlement: OpenGpuiSettlementState,
}

impl OpenGpuiPortInteractionState {
    /// Returns an idle, enabled port presentation.
    #[must_use]
    pub const fn idle() -> Self {
        Self {
            hovered: false,
            connected: false,
            focused: false,
            disabled: false,
            candidate: OpenGpuiPortCandidateState::Idle,
            settlement: OpenGpuiSettlementState::Idle,
        }
    }

    #[must_use]
    pub const fn hovered(&self) -> bool {
        self.hovered
    }

    #[must_use]
    pub const fn connected(&self) -> bool {
        self.connected
    }

    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn disabled(&self) -> bool {
        self.disabled
    }

    #[must_use]
    pub const fn candidate(&self) -> &OpenGpuiPortCandidateState {
        &self.candidate
    }

    #[must_use]
    pub const fn settlement(&self) -> OpenGpuiSettlementState {
        self.settlement
    }

    #[must_use]
    pub const fn with_hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    #[must_use]
    pub const fn with_connected(mut self, connected: bool) -> Self {
        self.connected = connected;
        self
    }

    #[must_use]
    pub const fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub const fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn with_candidate(mut self, candidate: OpenGpuiPortCandidateState) -> Self {
        self.candidate = candidate;
        self
    }

    #[must_use]
    pub const fn with_settlement(mut self, settlement: OpenGpuiSettlementState) -> Self {
        self.settlement = settlement;
        self
    }

    /// Returns the primary structured rejection reason, when this is an invalid target.
    #[must_use]
    pub fn rejection_reason(&self) -> Option<&Diagnostic> {
        self.candidate.rejection_reason()
    }

    /// Returns concise state text suitable for an accessible description.
    #[must_use]
    pub fn accessible_state_text(&self) -> String {
        let mut states = Vec::with_capacity(4);
        if self.connected {
            states.push("connected".to_owned());
        }
        if !self.candidate.is_idle() {
            states.push(self.candidate.accessible_state_text());
        }
        if self.disabled {
            states.push("disabled".to_owned());
        }
        if self.settlement.is_pending() {
            states.push("pending".to_owned());
        }
        if states.is_empty() {
            "idle".to_owned()
        } else {
            states.join(", ")
        }
    }
}

/// Immutable adapter-local node and port presentation delivered to native components.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpenGpuiNodePresentationSnapshot {
    node: OpenGpuiNodeInteractionState,
    ports: Arc<BTreeMap<PortId, OpenGpuiPortInteractionState>>,
}

impl OpenGpuiNodePresentationSnapshot {
    #[must_use]
    pub(crate) fn new(
        node: OpenGpuiNodeInteractionState,
        ports: impl IntoIterator<Item = (PortId, OpenGpuiPortInteractionState)>,
    ) -> Self {
        Self {
            node,
            ports: Arc::new(ports.into_iter().collect()),
        }
    }

    #[must_use]
    pub const fn node(&self) -> OpenGpuiNodeInteractionState {
        self.node
    }

    #[must_use]
    pub fn port(&self, port: PortId) -> Option<&OpenGpuiPortInteractionState> {
        self.ports.get(&port)
    }

    pub fn ports(
        &self,
    ) -> impl ExactSizeIterator<Item = (PortId, &OpenGpuiPortInteractionState)> + '_ {
        self.ports.iter().map(|(port, state)| (*port, state))
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn with_node(mut self, node: OpenGpuiNodeInteractionState) -> Self {
        self.node = node;
        self
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn with_port(mut self, port: PortId, state: OpenGpuiPortInteractionState) -> Self {
        Arc::make_mut(&mut self.ports).insert(port, state);
        self
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn without_port(mut self, port: PortId) -> Self {
        Arc::make_mut(&mut self.ports).remove(&port);
        self
    }
}

fn accessible_state_text(states: Vec<&'static str>) -> String {
    if states.is_empty() {
        "idle".to_owned()
    } else {
        states.join(", ")
    }
}

fn diagnostics_equal(left: &[Diagnostic], right: &[Diagnostic]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.key == right.key
                && left.severity == right.severity
                && left.target == right.target
                && left.message == right.message
        })
}

#[cfg(test)]
mod tests {
    use jellyflow_runtime::{graph::DiagnosticSeverity, graph::DiagnosticTarget};

    use super::*;

    #[test]
    fn node_state_composes_without_collapsing_independent_facts() {
        let state = OpenGpuiNodeInteractionState::idle()
            .with_selected(true)
            .with_hovered(true)
            .with_dragging(true)
            .with_focused(true)
            .with_disabled(true)
            .with_lod(OpenGpuiNodeLod::PinnedDetail)
            .with_settlement(OpenGpuiSettlementState::Pending);

        assert!(state.selected());
        assert!(state.hovered());
        assert!(state.dragging());
        assert!(state.focused());
        assert!(state.disabled());
        assert_eq!(state.lod(), OpenGpuiNodeLod::PinnedDetail);
        assert!(state.lod().retains_native_controls());
        assert!(!state.lod().publishes_native_graph_geometry());
        assert!(state.settlement().is_pending());
        assert_eq!(
            state.accessible_state_text(),
            "selected, dragging, disabled, native detail pinned, pending"
        );
    }

    #[test]
    fn invalid_candidate_retains_and_compares_structured_diagnostics() {
        let first = rejection("connect.type_mismatch", "Integer cannot connect to text");
        let same = rejection("connect.type_mismatch", "Integer cannot connect to text");
        let different = rejection("connect.capacity", "Input already has a connection");

        let state = OpenGpuiPortCandidateState::invalid(vec![first]);
        assert_eq!(state, OpenGpuiPortCandidateState::invalid(vec![same]));
        assert_ne!(state, OpenGpuiPortCandidateState::invalid(vec![different]));
        assert_eq!(
            state.rejection_text(),
            Some("Integer cannot connect to text")
        );
        assert_eq!(
            state.accessible_state_text(),
            "incompatible connection target: Integer cannot connect to text"
        );
    }

    #[test]
    fn port_state_combines_connection_candidate_and_settlement_text() {
        let state = OpenGpuiPortInteractionState::idle()
            .with_hovered(true)
            .with_connected(true)
            .with_focused(true)
            .with_candidate(OpenGpuiPortCandidateState::ValidTarget)
            .with_settlement(OpenGpuiSettlementState::Pending);

        assert!(state.hovered());
        assert!(state.connected());
        assert!(state.focused());
        assert!(state.candidate().is_valid_target());
        assert_eq!(
            state.accessible_state_text(),
            "connected, compatible connection target, pending"
        );
    }

    #[test]
    fn snapshot_clones_cheaply_and_updates_without_mutating_the_source() {
        let first = PortId::from_u128(1);
        let second = PortId::from_u128(2);
        let original = OpenGpuiNodePresentationSnapshot::new(
            OpenGpuiNodeInteractionState::idle(),
            [(first, OpenGpuiPortInteractionState::idle())],
        );
        let changed = original
            .clone()
            .with_node(OpenGpuiNodeInteractionState::idle().with_selected(true))
            .with_port(
                second,
                OpenGpuiPortInteractionState::idle().with_connected(true),
            )
            .without_port(first);

        assert!(!original.node().selected());
        assert!(original.port(first).is_some());
        assert!(original.port(second).is_none());
        assert!(changed.node().selected());
        assert!(changed.port(first).is_none());
        assert!(changed.port(second).is_some_and(|state| state.connected()));
        assert_eq!(changed.ports().len(), 1);
    }

    fn rejection(key: &str, message: &str) -> Diagnostic {
        Diagnostic {
            key: key.to_owned(),
            severity: DiagnosticSeverity::Error,
            target: DiagnosticTarget::Port {
                id: PortId::from_u128(9),
            },
            message: message.to_owned(),
        }
    }
}
