use jellyflow_core::core::CanvasSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeInclusion {
    /// Include nodes that intersect the query rect.
    Partial,
    /// Include nodes only when fully contained within the query rect.
    Full,
}

#[derive(Debug, Clone, Copy)]
pub struct GetNodesBoundsOptions {
    /// Node origin (anchor) used to interpret `Node.pos`.
    ///
    /// - `(0.0, 0.0)` means `pos` is top-left.
    /// - `(0.5, 0.5)` means `pos` is center.
    pub node_origin: (f32, f32),
    /// Whether to include hidden nodes.
    pub include_hidden: bool,
    /// Fallback size to use when a node has no explicit size.
    ///
    /// When `None`, nodes without a size are skipped.
    pub fallback_size: Option<CanvasSize>,
}

impl Default for GetNodesBoundsOptions {
    fn default() -> Self {
        Self {
            node_origin: (0.0, 0.0),
            include_hidden: false,
            fallback_size: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GetNodesInsideOptions {
    pub inclusion: NodeInclusion,
    pub node_origin: (f32, f32),
    pub include_hidden: bool,
    pub fallback_size: Option<CanvasSize>,
}

impl Default for GetNodesInsideOptions {
    fn default() -> Self {
        Self {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: false,
            fallback_size: None,
        }
    }
}
