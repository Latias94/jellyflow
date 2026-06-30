use crate::runtime::store::NodeGraphStore;

use super::super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    _store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::AssertNodeActionAvailability {
            action,
            expect_enabled,
        } => {
            let actual = action.availability.is_enabled();
            if actual == *expect_enabled {
                Ok(())
            } else {
                Err(format!(
                    "action `{}` availability was enabled={actual}, expected enabled={expect_enabled}; disabled_reason={:?}",
                    action.key, action.availability.disabled_reason
                ))
            }
        }
        _ => return None,
    })
}
