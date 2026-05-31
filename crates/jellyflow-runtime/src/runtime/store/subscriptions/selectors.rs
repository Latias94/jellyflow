use std::any::Any;

use crate::runtime::events::{NodeGraphStoreSnapshot, SubscriptionToken};

type SelectorValue = dyn Any;

pub(super) struct SelectorSubscription {
    token: SubscriptionToken,
    compute: Box<dyn for<'a> Fn(NodeGraphStoreSnapshot<'a>) -> Box<SelectorValue>>,
    equals: Box<dyn Fn(&SelectorValue, &SelectorValue) -> bool>,
    callback: Box<dyn FnMut(&SelectorValue, &SelectorValue)>,
    last: Box<SelectorValue>,
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
            compute: Box::new(move |snapshot| Box::new(selector(snapshot)) as Box<SelectorValue>),
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
