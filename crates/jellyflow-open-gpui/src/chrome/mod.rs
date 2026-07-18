//! Optional native graph chrome primitives and paint-plan facts.

mod edge;
mod edge_painter;
mod port;

pub use edge::{
    OpenGpuiEdgeBatchKey, OpenGpuiEdgeLayer, OpenGpuiEdgeStyle, OpenGpuiEdgeTone,
    OpenGpuiEdgeVisualState,
};
pub use edge_painter::{
    OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS, OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES,
    OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeLabelRecipe, OpenGpuiEdgeMarkerEndpoint,
    OpenGpuiEdgeMarkerRecipe, OpenGpuiEdgeMarkerShape, OpenGpuiEdgePaintFallbacks,
    OpenGpuiEdgePainterBatchKey, OpenGpuiEdgePainterRecipe, OpenGpuiEdgePainterRegistrationError,
    OpenGpuiEdgePainterRegistry, OpenGpuiEdgePainterRegistryBuilder, OpenGpuiEdgeStrokePattern,
    OpenGpuiResolvedEdgePaintRecipe,
};
pub use port::{
    OpenGpuiPortMarker, OpenGpuiPortMarkerCue, OpenGpuiPortMarkerStyle, OpenGpuiPortVisualState,
};
