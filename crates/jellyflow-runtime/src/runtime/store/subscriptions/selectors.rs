use std::any::Any;

use crate::runtime::events::{NodeGraphStoreSnapshot, SubscriptionToken};

type SelectorValue = dyn Any;
type BoxedSelectorValue = Box<SelectorValue>;

pub(super) struct SelectorSubscription {
    token: SubscriptionToken,
    compute: Box<dyn for<'a> Fn(NodeGraphStoreSnapshot<'a>) -> BoxedSelectorValue>,
    equals: Box<dyn Fn(&SelectorValue, &SelectorValue) -> bool>,
    callback: Box<dyn FnMut(&SelectorValue, &SelectorValue)>,
    last: BoxedSelectorValue,
}

impl SelectorSubscription {
    pub(super) fn new<T>(
        token: SubscriptionToken,
        selector: impl for<'a> Fn(NodeGraphStoreSnapshot<'a>) -> T + 'static,
        initial: T,
        mut on_change: impl FnMut(&T, &T) + 'static,
    ) -> Self
    where
        T: PartialEq + 'static,
    {
        Self {
            token,
            compute: Box::new(move |snapshot| Box::new(selector(snapshot)) as BoxedSelectorValue),
            equals: Box::new(|a, b| {
                let a = typed_selector_value::<T>(a);
                let b = typed_selector_value::<T>(b);
                a == b
            }),
            callback: Box::new(move |prev, next| {
                let prev = typed_selector_value::<T>(prev);
                let next = typed_selector_value::<T>(next);
                on_change(prev, next);
            }),
            last: Box::new(initial),
        }
    }

    pub(super) fn token(&self) -> SubscriptionToken {
        self.token
    }

    pub(super) fn notify_if_changed(&mut self, snapshot: NodeGraphStoreSnapshot<'_>) {
        let next = (self.compute)(snapshot);
        let changed = !(self.equals)(&*self.last, &*next);
        if !changed {
            return;
        }
        (self.callback)(&*self.last, &*next);
        self.last = next;
    }
}

fn typed_selector_value<T: 'static>(value: &SelectorValue) -> &T {
    value.downcast_ref::<T>().expect("selector type mismatch")
}
