use super::super::super::traits::NodeGraphCallbacks;
use super::super::super::types::ConnectionChange;

pub(super) fn dispatch_connection_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    changes: &[ConnectionChange],
) {
    for change in changes.iter().copied() {
        callbacks.on_connection_change(change);
        match change {
            ConnectionChange::Connected(conn) => callbacks.on_connect(conn),
            ConnectionChange::Disconnected(conn) => callbacks.on_disconnect(conn),
            ConnectionChange::Reconnected { edge, from, to } => {
                callbacks.on_reconnect(edge, from, to)
            }
        }
    }
}
