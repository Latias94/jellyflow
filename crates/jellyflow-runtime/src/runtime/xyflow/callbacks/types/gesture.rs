use jellyflow_core::core::CanvasPoint;

/// Viewport move gesture kind (UI-driven).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportMoveKind {
    /// Pointer-drag panning (mouse/touch drag).
    PanDrag,
    /// Inertial/momentum panning after releasing a pan drag.
    PanInertia,
    /// Panning via scroll wheel / trackpad scroll when `pan_on_scroll` is enabled.
    PanScroll,
    /// Zooming via scroll wheel (e.g. Ctrl+wheel).
    ZoomWheel,
    /// Zooming via pinch gesture (trackpad pinch).
    ZoomPinch,
    /// Zooming via double-click gesture.
    ZoomDoubleClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportMoveEndOutcome {
    Ended,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportMoveStart {
    pub kind: ViewportMoveKind,
    pub pan: CanvasPoint,
    pub zoom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportMoveEnd {
    pub kind: ViewportMoveKind,
    pub pan: CanvasPoint,
    pub zoom: f32,
    pub outcome: ViewportMoveEndOutcome,
}
