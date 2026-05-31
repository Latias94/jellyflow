//! Store subscription and selector internals.

use crate::runtime::events::{
    NodeGraphGestureEvent, NodeGraphStoreEvent, NodeGraphStoreSnapshot, SubscriptionToken,
};

use super::NodeGraphStore;
use super::snapshot::StoreSnapshotParts;

mod selectors;

pub(crate) use self::selectors::SelectorSubscription;

impl NodeGraphStore {
    /// Subscribes to store events (graph commits + view-state changes).
    ///
    /// This is the minimal B-layer equivalent of XyFlow's store subscriptions.
    pub fn subscribe(
        &mut self,
        f: impl for<'a> FnMut(NodeGraphStoreEvent<'a>) + 'static,
    ) -> SubscriptionToken {
        let token = SubscriptionToken::new(self.next_subscription);
        self.next_subscription = self.next_subscription.saturating_add(1).max(1);
        self.event_subscriptions.push((token, Box::new(f)));
        token
    }

    pub(crate) fn subscribe_gesture_with_token(
        &mut self,
        token: SubscriptionToken,
        f: impl FnMut(NodeGraphGestureEvent) + 'static,
    ) {
        self.gesture_subscriptions.push((token, Box::new(f)));
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
        T: Clone + PartialEq + 'static,
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
        T: Clone + PartialEq + 'static,
    {
        let token = SubscriptionToken::new(self.next_subscription);
        self.next_subscription = self.next_subscription.saturating_add(1).max(1);

        let initial = selector(self.snapshot());

        self.selector_subscriptions.push(SelectorSubscription::new(
            token, selector, initial, on_change,
        ));

        token
    }

    /// Removes a subscription.
    pub fn unsubscribe(&mut self, token: SubscriptionToken) -> bool {
        let mut removed = false;

        let before = self.event_subscriptions.len();
        self.event_subscriptions.retain(|(t, _)| *t != token);
        removed |= before != self.event_subscriptions.len();

        let before = self.gesture_subscriptions.len();
        self.gesture_subscriptions.retain(|(t, _)| *t != token);
        removed |= before != self.gesture_subscriptions.len();

        let before = self.selector_subscriptions.len();
        self.selector_subscriptions.retain(|s| s.token() != token);
        removed |= before != self.selector_subscriptions.len();

        removed
    }

    pub(super) fn notify_selectors(&mut self) {
        if self.selector_subscriptions.is_empty() {
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
        for sub in &mut self.selector_subscriptions {
            sub.notify_if_changed(snapshot_parts.snapshot());
        }
    }
}
