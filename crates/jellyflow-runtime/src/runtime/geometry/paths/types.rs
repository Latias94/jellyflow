use jellyflow_core::core::CanvasPoint;

/// Renderer-neutral path command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    MoveTo(CanvasPoint),
    LineTo(CanvasPoint),
    CubicTo {
        control1: CanvasPoint,
        control2: CanvasPoint,
        to: CanvasPoint,
    },
}

/// Label placement derived from an edge path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePathLabel {
    pub point: CanvasPoint,
    pub offset_x: f32,
    pub offset_y: f32,
}

/// Renderer-neutral edge path.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePath {
    pub commands: Vec<PathCommand>,
    pub label: EdgePathLabel,
}

impl EdgePath {
    pub(crate) fn is_finite(&self) -> bool {
        self.commands.iter().all(|command| match *command {
            PathCommand::MoveTo(point) | PathCommand::LineTo(point) => point.is_finite(),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => control1.is_finite() && control2.is_finite() && to.is_finite(),
        }) && self.label.point.is_finite()
            && self.label.offset_x.is_finite()
            && self.label.offset_y.is_finite()
    }
}
