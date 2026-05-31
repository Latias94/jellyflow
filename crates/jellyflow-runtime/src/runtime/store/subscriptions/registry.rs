use crate::runtime::events::{
    NodeGraphGestureEvent, NodeGraphStoreEvent, NodeGraphStoreSnapshot, SubscriptionToken,
};

use super::selectors::SelectorSubscription;

type EventSubscription = (
    SubscriptionToken,
    Box<dyn for<'a> FnMut(NodeGraphStoreEvent<'a>)>,
);
type GestureSubscription = (SubscriptionToken, Box<dyn FnMut(NodeGraphGestureEvent)>);

pub(in crate::runtime::store) struct StoreSubscriptions {
    next: u64,
    events: Vec<EventSubscription>,
    gestures: Vec<GestureSubscription>,
    selectors: Vec<SelectorSubscription>,
}

impl Default for StoreSubscriptions {
    fn default() -> Self {
        Self {
            next: 1,
            events: Vec::new(),
            gestures: Vec::new(),
            selectors: Vec::new(),
        }
    }
}

impl StoreSubscriptions {
    pub(in crate::runtime::store) fn allocate_token(&mut self) -> SubscriptionToken {
        let token = SubscriptionToken::new(self.next);
        self.next = self.next.saturating_add(1).max(1);
        token
    }

    pub(in crate::runtime::store) fn subscribe_event(
        &mut self,
        f: impl for<'a> FnMut(NodeGraphStoreEvent<'a>) + 'static,
    ) -> SubscriptionToken {
        let token = self.allocate_token();
        self.events.push((token, Box::new(f)));
        token
    }

    pub(in crate::runtime::store) fn subscribe_gesture_with_token(
        &mut self,
        token: SubscriptionToken,
        f: impl FnMut(NodeGraphGestureEvent) + 'static,
    ) {
        self.gestures.push((token, Box::new(f)));
    }

    pub(in crate::runtime::store) fn subscribe_selector_with_token<T>(
        &mut self,
        token: SubscriptionToken,
        selector: impl for<'a> Fn(NodeGraphStoreSnapshot<'a>) -> T + 'static,
        initial: T,
        on_change: impl FnMut(&T, &T) + 'static,
    ) where
        T: PartialEq + 'static,
    {
        self.selectors.push(SelectorSubscription::new(
            token, selector, initial, on_change,
        ));
    }

    pub(in crate::runtime::store) fn unsubscribe(&mut self, token: SubscriptionToken) -> bool {
        let mut removed = false;

        removed |= remove_subscription_token(&mut self.events, token);
        removed |= remove_subscription_token(&mut self.gestures, token);
        removed |= remove_selector_subscription(&mut self.selectors, token);

        removed
    }

    pub(in crate::runtime::store) fn notify_selectors(
        &mut self,
        snapshot: NodeGraphStoreSnapshot<'_>,
    ) {
        for sub in &mut self.selectors {
            sub.notify_if_changed(snapshot);
        }
    }

    pub(in crate::runtime::store) fn emit_event(&mut self, event: NodeGraphStoreEvent<'_>) {
        for (_, sub) in &mut self.events {
            sub(event);
        }
    }

    pub(in crate::runtime::store) fn emit_gesture(&mut self, event: NodeGraphGestureEvent) {
        for (_, sub) in &mut self.gestures {
            sub(event.clone());
        }
    }

    pub(in crate::runtime::store) fn has_selectors(&self) -> bool {
        !self.selectors.is_empty()
    }

    pub(in crate::runtime::store) fn event_subscription_count(&self) -> usize {
        self.events.len()
    }

    pub(in crate::runtime::store) fn gesture_subscription_count(&self) -> usize {
        self.gestures.len()
    }

    pub(in crate::runtime::store) fn selector_subscription_count(&self) -> usize {
        self.selectors.len()
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
