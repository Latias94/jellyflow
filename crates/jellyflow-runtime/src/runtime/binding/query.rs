use jellyflow_core::core::{
    BindingEndpoint, BindingId, CanvasPoint, CanvasRect, EdgeId, GraphLocalBindingTarget, GroupId,
    NodeId, StickyNoteId,
};

use crate::runtime::geometry::EdgePosition;

/// Options controlling runtime binding queries.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct BindingQueryOptions {
    pub include_hidden: bool,
    pub fallback_node_size: Option<jellyflow_core::core::CanvasSize>,
}

impl BindingQueryOptions {
    pub fn include_hidden(mut self, include_hidden: bool) -> Self {
        self.include_hidden = include_hidden;
        self
    }

    pub fn with_fallback_node_size(
        mut self,
        fallback_node_size: Option<jellyflow_core::core::CanvasSize>,
    ) -> Self {
        self.fallback_node_size = fallback_node_size;
        self
    }
}

/// Store-level binding facts derived from graph data and runtime geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingQueryResult {
    pub revision: u64,
    pub bindings: Vec<ResolvedBinding>,
}

impl BindingQueryResult {
    pub fn new(revision: u64, bindings: Vec<ResolvedBinding>) -> Self {
        Self { revision, bindings }
    }

    pub fn binding(&self, binding: BindingId) -> Option<&ResolvedBinding> {
        self.bindings.iter().find(|resolved| resolved.id == binding)
    }

    pub fn pinned_node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.bindings
            .iter()
            .flat_map(|binding| [&binding.subject, &binding.target])
            .filter_map(|endpoint| endpoint.pinnable_node())
    }
}

/// One resolved binding relationship.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBinding {
    pub id: BindingId,
    pub subject: ResolvedBindingEndpoint,
    pub target: ResolvedBindingEndpoint,
    pub kind: Option<String>,
}

impl ResolvedBinding {
    pub fn new(
        id: BindingId,
        subject: ResolvedBindingEndpoint,
        target: ResolvedBindingEndpoint,
        kind: Option<String>,
    ) -> Self {
        Self {
            id,
            subject,
            target,
            kind,
        }
    }
}

/// Resolved form of one binding endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBindingEndpoint {
    pub endpoint: BindingEndpoint,
    pub resolution: BindingEndpointResolution,
}

impl ResolvedBindingEndpoint {
    pub fn new(endpoint: BindingEndpoint, resolution: BindingEndpointResolution) -> Self {
        Self {
            endpoint,
            resolution,
        }
    }

    pub fn unresolved(endpoint: BindingEndpoint) -> Self {
        Self::new(endpoint, BindingEndpointResolution::Unresolved)
    }

    pub fn hidden(endpoint: BindingEndpoint) -> Self {
        Self::new(endpoint, BindingEndpointResolution::Hidden)
    }

    pub fn source(endpoint: BindingEndpoint) -> Self {
        Self::new(endpoint, BindingEndpointResolution::Source)
    }

    pub fn status(&self) -> BindingEndpointResolutionStatus {
        self.resolution.status()
    }

    pub fn pinnable_node(&self) -> Option<NodeId> {
        let BindingEndpoint::GraphLocal {
            target: GraphLocalBindingTarget::Node { id },
        } = self.endpoint
        else {
            return None;
        };
        self.resolution.is_resolved().then_some(id)
    }
}

/// Geometry or status resolved for a binding endpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BindingEndpointResolution {
    NodeRect {
        node: NodeId,
        rect: CanvasRect,
        center: CanvasPoint,
    },
    PortAnchor {
        node: NodeId,
        point: CanvasPoint,
    },
    EdgePosition {
        edge: EdgeId,
        position: EdgePosition,
    },
    GroupRect {
        group: GroupId,
        rect: CanvasRect,
        center: CanvasPoint,
    },
    StickyNoteRect {
        note: StickyNoteId,
        rect: CanvasRect,
        center: CanvasPoint,
    },
    Graph,
    Source,
    Hidden,
    Unresolved,
}

impl BindingEndpointResolution {
    pub fn status(self) -> BindingEndpointResolutionStatus {
        match self {
            Self::NodeRect { .. }
            | Self::PortAnchor { .. }
            | Self::EdgePosition { .. }
            | Self::GroupRect { .. }
            | Self::StickyNoteRect { .. }
            | Self::Graph
            | Self::Source => BindingEndpointResolutionStatus::Resolved,
            Self::Hidden => BindingEndpointResolutionStatus::Hidden,
            Self::Unresolved => BindingEndpointResolutionStatus::Unresolved,
        }
    }

    pub fn is_resolved(self) -> bool {
        matches!(self.status(), BindingEndpointResolutionStatus::Resolved)
    }
}

/// Coarse endpoint resolution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingEndpointResolutionStatus {
    Resolved,
    Hidden,
    Unresolved,
}
