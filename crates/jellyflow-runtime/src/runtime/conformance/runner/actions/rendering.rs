use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::CanvasSize;

use super::super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::AssertRenderingQuery {
            viewport_size,
            expected,
        } => assert_rendering_query(store, *viewport_size, expected),
        _ => return None,
    })
}

pub(super) fn assert_rendering_query(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
    expected: &crate::runtime::rendering::RenderingQueryResult,
) -> Result<(), String> {
    let actual = store.rendering_query(viewport_size);
    if &actual == expected {
        Ok(())
    } else {
        Err(format!(
            "rendering query resolved to {actual:?}, expected {expected:?}"
        ))
    }
}
