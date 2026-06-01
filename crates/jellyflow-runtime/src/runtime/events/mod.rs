//! B-layer store event model (subscriptions).
//!
//! This is intentionally small and headless-safe.

mod connection;
mod gesture;
mod node_drag;
mod snapshot;
mod store;
mod token;
mod view;
mod viewport;

pub use connection::{ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart};
pub use gesture::NodeGraphGestureEvent;
pub use node_drag::{NodeDragEnd, NodeDragEndOutcome, NodeDragStart, NodeDragUpdate};
pub use snapshot::{NodeGraphDocumentSnapshot, NodeGraphStoreSnapshot};
pub use store::NodeGraphStoreEvent;
pub use token::SubscriptionToken;
pub use view::ViewChange;
pub use viewport::{
    ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome, ViewportMoveKind, ViewportMoveStart,
};
