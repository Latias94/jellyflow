//! Optional native graph chrome primitives and paint-plan facts.

mod edge;
mod edge_painter;
mod port;

fn positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

pub use edge::{
    OpenGpuiEdgeBatchKey, OpenGpuiEdgeHaloStyle, OpenGpuiEdgeLayer, OpenGpuiEdgeStyle,
    OpenGpuiEdgeTone, OpenGpuiEdgeVisualFacts, OpenGpuiEdgeWeightTier,
};
pub use edge_painter::{
    OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS, OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES,
    OPEN_GPUI_MAX_TYPE_STYLE_REGISTRATIONS, OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeLabelRecipe,
    OpenGpuiEdgeMarkerEndpoint, OpenGpuiEdgeMarkerRecipe, OpenGpuiEdgeMarkerShape,
    OpenGpuiEdgePaintFallbacks, OpenGpuiEdgePainterBatchKey, OpenGpuiEdgePainterRecipe,
    OpenGpuiEdgePainterRegistrationError, OpenGpuiEdgePainterRegistry,
    OpenGpuiEdgePainterRegistryBuilder, OpenGpuiEdgeStrokePattern, OpenGpuiResolvedEdgePaintRecipe,
};
pub(crate) use port::render_port_marker_element;
pub use port::{
    OpenGpuiPortCandidateVisual, OpenGpuiPortMarker, OpenGpuiPortMarkerCue,
    OpenGpuiPortMarkerShape, OpenGpuiPortMarkerStyle, OpenGpuiPortTone, OpenGpuiPortVisualFacts,
};
