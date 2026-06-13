use jellyflow_core::{CanvasSize, NodeId};
use serde::{Deserialize, Serialize};

use crate::engine::{
    LayoutDirection, LayoutEngineId, LayoutEngineRequest, LayoutOptions, LayoutRequest,
    LayoutScope, LayoutSpacing,
};

/// Builder for common layout presets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutPresetBuilder {
    request: LayoutEngineRequest,
}

impl Default for LayoutPresetBuilder {
    fn default() -> Self {
        Self::workflow()
    }
}

impl LayoutPresetBuilder {
    /// Returns a layered workflow preset.
    pub fn workflow() -> Self {
        Self::new(LayoutEngineId::dugong(), LayoutRequest::all())
    }

    /// Returns a tree-shaped layered preset.
    pub fn tree() -> Self {
        Self::new(LayoutEngineId::tidy_tree(), LayoutRequest::all()).with_options(LayoutOptions {
            direction: LayoutDirection::TopToBottom,
            spacing: LayoutSpacing {
                nodesep: 32.0,
                ranksep: 72.0,
                edgesep: 16.0,
            },
            ..LayoutOptions::default()
        })
    }

    /// Returns a radial mind-map preset.
    pub fn mind_map() -> Self {
        Self::new(LayoutEngineId::mind_map_radial(), LayoutRequest::all())
    }

    /// Returns a freeform mind-map preset.
    pub fn freeform() -> Self {
        Self::new(LayoutEngineId::mind_map_freeform(), LayoutRequest::all()).with_options(
            LayoutOptions {
                spacing: LayoutSpacing {
                    nodesep: 24.0,
                    ranksep: 24.0,
                    edgesep: 24.0,
                },
                ..LayoutOptions::default()
            },
        )
    }

    /// Creates a preset builder for a specific engine.
    pub fn new(engine: impl Into<LayoutEngineId>, layout: LayoutRequest) -> Self {
        Self {
            request: LayoutEngineRequest::new(engine, layout),
        }
    }

    /// Uses a different engine.
    pub fn with_engine(mut self, engine: impl Into<LayoutEngineId>) -> Self {
        self.request.engine = engine.into();
        self
    }

    /// Uses a different layout request.
    pub fn with_layout(mut self, layout: LayoutRequest) -> Self {
        self.request.layout = layout;
        self
    }

    /// Sets layout options.
    pub fn with_options(mut self, options: LayoutOptions) -> Self {
        self.request.layout.options = options;
        self
    }

    /// Uses a different layered layout direction.
    pub fn with_direction(mut self, direction: LayoutDirection) -> Self {
        self.request.layout.options.direction = direction;
        self
    }

    /// Uses a different layered spacing profile.
    pub fn with_spacing(mut self, spacing: LayoutSpacing) -> Self {
        self.request.layout.options.spacing = spacing;
        self
    }

    /// Uses a different margin.
    pub fn with_margin(mut self, margin: CanvasSize) -> Self {
        self.request.layout.options.margin = margin;
        self
    }

    /// Uses a different fallback node size.
    pub fn with_default_node_size(mut self, size: CanvasSize) -> Self {
        self.request.layout.options.default_node_size = size;
        self
    }

    /// Uses a different fallback node origin.
    pub fn with_node_origin(mut self, node_origin: (f32, f32)) -> Self {
        self.request.layout.options.node_origin = node_origin;
        self
    }

    /// Targets all visible nodes.
    pub fn all(mut self) -> Self {
        self.request.layout.scope = LayoutScope::All;
        self
    }

    /// Targets a selected set of nodes.
    pub fn nodes(mut self, nodes: impl IntoIterator<Item = NodeId>) -> Self {
        self.request.layout.scope = LayoutScope::Nodes {
            nodes: nodes.into_iter().collect(),
        };
        self
    }

    /// Uses a different request scope.
    pub fn with_scope(mut self, scope: LayoutScope) -> Self {
        self.request.layout.scope = scope;
        self
    }

    /// Adds request-local measured node sizes.
    pub fn with_measured_node_sizes(
        mut self,
        sizes: impl IntoIterator<Item = (NodeId, CanvasSize)>,
    ) -> Self {
        self.request.layout.measured_node_sizes.extend(sizes);
        self
    }

    /// Builds a layout engine request.
    pub fn build(self) -> LayoutEngineRequest {
        self.request
    }

    /// Returns the layout request without consuming the builder.
    pub fn layout_request(&self) -> LayoutRequest {
        self.request.layout.clone()
    }

    /// Returns the engine request without consuming the builder.
    pub fn engine_request(&self) -> LayoutEngineRequest {
        self.request.clone()
    }
}

impl From<LayoutPresetBuilder> for LayoutEngineRequest {
    fn from(value: LayoutPresetBuilder) -> Self {
        value.build()
    }
}
