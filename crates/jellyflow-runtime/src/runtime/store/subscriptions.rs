//! Store subscription and selector internals.

use crate::runtime::events::{
    NodeGraphGestureEvent, NodeGraphStoreEvent, NodeGraphStoreSnapshot, SubscriptionToken,
};

use super::NodeGraphStore;

pub(super) struct SelectorSubscription {
    token: SubscriptionToken,
    compute: Box<dyn for<'a> Fn(NodeGraphStoreSnapshot<'a>) -> Box<dyn std::any::Any>>,
    equals: Box<dyn Fn(&dyn std::any::Any, &dyn std::any::Any) -> bool>,
    callback: Box<dyn FnMut(&dyn std::any::Any, &dyn std::any::Any)>,
    last: Box<dyn std::any::Any>,
}

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
        mut on_change: impl FnMut(&T, &T) + 'static,
    ) -> SubscriptionToken
    where
        T: Clone + PartialEq + 'static,
    {
        let token = SubscriptionToken::new(self.next_subscription);
        self.next_subscription = self.next_subscription.saturating_add(1).max(1);

        let snapshot = NodeGraphStoreSnapshot {
            graph: &self.graph,
            graph_revision: self.graph_revision,
            view_state: &self.view_state,
            interaction: &self.interaction,
            runtime_tuning: &self.runtime_tuning,
            history: &self.history,
        };
        let initial = selector(snapshot);

        self.selector_subscriptions.push(SelectorSubscription {
            token,
            compute: Box::new(move |snapshot| {
                Box::new(selector(snapshot)) as Box<dyn std::any::Any>
            }),
            equals: Box::new(|a, b| {
                let a = a.downcast_ref::<T>().expect("selector type mismatch");
                let b = b.downcast_ref::<T>().expect("selector type mismatch");
                a == b
            }),
            callback: Box::new(move |prev, next| {
                let prev = prev.downcast_ref::<T>().expect("selector type mismatch");
                let next = next.downcast_ref::<T>().expect("selector type mismatch");
                on_change(prev, next);
            }),
            last: Box::new(initial),
        });

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
        self.selector_subscriptions.retain(|s| s.token != token);
        removed |= before != self.selector_subscriptions.len();

        removed
    }

    pub(super) fn notify_selectors(&mut self) {
        if self.selector_subscriptions.is_empty() {
            return;
        }

        let graph = &self.graph;
        let graph_revision = self.graph_revision;
        let view_state = &self.view_state;
        let history = &self.history;
        for sub in &mut self.selector_subscriptions {
            let snapshot = NodeGraphStoreSnapshot {
                graph,
                graph_revision,
                view_state,
                interaction: &self.interaction,
                runtime_tuning: &self.runtime_tuning,
                history,
            };
            let next = (sub.compute)(snapshot);
            let changed = !(sub.equals)(&*sub.last, &*next);
            if !changed {
                continue;
            }
            (sub.callback)(&*sub.last, &*next);
            sub.last = next;
        }
    }
}
