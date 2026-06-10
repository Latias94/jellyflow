use crate::runtime::rendering::RenderingQueryResult;
use jellyflow_core::core::CanvasSize;

use super::ConformanceAction;

pub(super) fn kind(action: &ConformanceAction) -> Option<&'static str> {
    Some(match action {
        ConformanceAction::AssertRenderingQuery { .. } => "assert_rendering_query",
        _ => return None,
    })
}

impl ConformanceAction {
    pub fn assert_rendering_query(
        viewport_size: CanvasSize,
        expected: RenderingQueryResult,
    ) -> Self {
        Self::AssertRenderingQuery {
            viewport_size,
            expected,
        }
    }
}
