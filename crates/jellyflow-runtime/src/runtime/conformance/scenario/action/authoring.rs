use super::ConformanceAction;

pub(super) fn kind(action: &ConformanceAction) -> Option<&'static str> {
    Some(match action {
        ConformanceAction::AssertNodeActionAvailability { .. } => "assert_node_action_availability",
        _ => return None,
    })
}

impl ConformanceAction {
    pub fn assert_node_action_available(action: crate::schema::NodeActionDescriptor) -> Self {
        Self::AssertNodeActionAvailability {
            action,
            expect_enabled: true,
        }
    }

    pub fn assert_node_action_disabled(action: crate::schema::NodeActionDescriptor) -> Self {
        Self::AssertNodeActionAvailability {
            action,
            expect_enabled: false,
        }
    }
}
