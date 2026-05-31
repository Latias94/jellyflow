//! B-layer store event model (subscriptions).
//!
//! This is intentionally small and headless-safe.

mod connection;
mod snapshot;
mod store;
mod token;
mod view;

pub use connection::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeGraphGestureEvent,
};
pub use snapshot::{NodeGraphDocumentSnapshot, NodeGraphStoreSnapshot};
pub use store::NodeGraphStoreEvent;
pub use token::SubscriptionToken;
pub use view::ViewChange;
