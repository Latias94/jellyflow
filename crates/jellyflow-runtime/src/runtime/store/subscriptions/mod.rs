//! Store subscription and selector internals.

use crate::runtime::events::{
    NodeGraphGestureEvent, NodeGraphStoreEvent, NodeGraphStoreSnapshot, SubscriptionToken,
};

use super::NodeGraphStore;
use super::snapshot::StoreSnapshotParts;

mod registry;
mod selectors;

pub(super) use self::registry::StoreSubscriptions;

impl NodeGraphStore {
    /// Subscribes to store events (graph commits + view-state changes).
    ///
    /// This is the minimal B-layer equivalent of XyFlow's store subscriptions.
    pub fn subscribe(
        &mut self,
        f: impl for<'a> FnMut(NodeGraphStoreEvent<'a>) + 'static,
    ) -> SubscriptionToken {
        self.subscriptions.subscribe_event(f)
    }

    pub(crate) fn subscribe_gesture_with_token(
        &mut self,
        token: SubscriptionToken,
        f: impl FnMut(NodeGraphGestureEvent) + 'static,
    ) {
        self.subscriptions.subscribe_gesture_with_token(token, f);
    }

    /// Subscribes to a derived projection of store state and only fires when the derived value
    /// changes (by `PartialEq`).
    ///
    /// This is the B-layer "selector subscription" pattern used by XyFlow.
    pub fn subscribe_selector<T>(
        &mut self,
        selector: impl for<'a> Fn(NodeGraphStoreSnapshot<'a>) -> T + 'static,
        mut on_change: impl FnMut(&T) + 'static,
    ) -> SubscriptionToken
    where
        T: PartialEq + 'static,
    {
        self.subscribe_selector_diff(selector, move |_prev, next| on_change(next))
    }

    /// Subscribes to a derived projection and receives both the previous and next values.
    pub fn subscribe_selector_diff<T>(
        &mut self,
        selector: impl for<'a> Fn(NodeGraphStoreSnapshot<'a>) -> T + 'static,
        on_change: impl FnMut(&T, &T) + 'static,
    ) -> SubscriptionToken
    where
        T: PartialEq + 'static,
    {
        let token = self.subscriptions.allocate_token();
        let initial = selector(self.snapshot());

        self.subscriptions
            .subscribe_selector_with_token(token, selector, initial, on_change);
        token
    }

    /// Removes a subscription.
    pub fn unsubscribe(&mut self, token: SubscriptionToken) -> bool {
        self.subscriptions.unsubscribe(token)
    }

    pub(super) fn notify_selectors(&mut self) {
        if !self.subscriptions.has_selectors() {
            return;
        }

        let snapshot_parts = StoreSnapshotParts::from_store_fields(
            &self.graph,
            self.graph_revision,
            &self.view_state,
            &self.interaction,
            &self.runtime_tuning,
            &self.history,
        );
        self.subscriptions
            .notify_selectors(snapshot_parts.snapshot());
    }
}
