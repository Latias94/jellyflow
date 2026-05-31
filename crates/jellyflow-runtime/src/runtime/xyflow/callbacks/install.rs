use std::cell::RefCell;
use std::rc::Rc;

use super::dispatch::{dispatch_gesture_callbacks, dispatch_store_event_callbacks};
use super::traits::NodeGraphCallbacks;
use crate::runtime::events::SubscriptionToken;
use crate::runtime::store::NodeGraphStore;

/// Installs callbacks into a store via a subscription.
pub fn install_callbacks(
    store: &mut NodeGraphStore,
    callbacks: impl NodeGraphCallbacks,
) -> SubscriptionToken {
    let callbacks: Rc<RefCell<Box<dyn NodeGraphCallbacks>>> =
        Rc::new(RefCell::new(Box::new(callbacks)));
    let event_callbacks = callbacks.clone();
    let token = store.subscribe(move |ev| {
        let mut callbacks = event_callbacks.borrow_mut();
        dispatch_store_event_callbacks(callbacks.as_mut(), ev);
    });

    let gesture_callbacks = callbacks;
    store.subscribe_gesture_with_token(token, move |ev| {
        let mut callbacks = gesture_callbacks.borrow_mut();
        dispatch_gesture_callbacks(callbacks.as_mut(), ev);
    });
    token
}
