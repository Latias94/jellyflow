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
        let token = self.allocate_subscription_token();
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
        let token = self.allocate_subscription_token();
        let initial = selector(self.snapshot());

        self.selector_subscriptions.push(SelectorSubscription::new(
            token, selector, initial, on_change,
        ));

        token
    }

    /// Removes a subscription.
    pub fn unsubscribe(&mut self, token: SubscriptionToken) -> bool {
        let mut removed = false;

        removed |= remove_subscription_token(&mut self.event_subscriptions, token);
        removed |= remove_subscription_token(&mut self.gesture_subscriptions, token);

        removed |= remove_selector_subscription(&mut self.selector_subscriptions, token);

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

    fn allocate_subscription_token(&mut self) -> SubscriptionToken {
        let token = SubscriptionToken::new(self.next_subscription);
        self.next_subscription = self.next_subscription.saturating_add(1).max(1);
        token
    }
}

fn remove_subscription_token<T>(
    subscriptions: &mut Vec<(SubscriptionToken, T)>,
    token: SubscriptionToken,
) -> bool {
    let before = subscriptions.len();
    subscriptions.retain(|(subscription_token, _)| *subscription_token != token);
    before != subscriptions.len()
}

fn remove_selector_subscription(
    subscriptions: &mut Vec<SelectorSubscription>,
    token: SubscriptionToken,
) -> bool {
    let before = subscriptions.len();
    subscriptions.retain(|subscription| subscription.token() != token);
    before != subscriptions.len()
}
