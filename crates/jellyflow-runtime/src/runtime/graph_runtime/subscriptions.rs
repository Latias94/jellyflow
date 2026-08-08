use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::runtime::unwind::{PanicCollector, PanicPayload};

use super::{GraphRuntimeEvent, RuntimeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphRuntimeSubscriptionToken {
    runtime: RuntimeId,
    local: u64,
}

struct RuntimeEventEntry {
    active: Cell<bool>,
    callback: RefCell<Box<dyn FnMut(GraphRuntimeEvent)>>,
}

pub(super) struct RuntimeEventHub {
    owner: RuntimeId,
    next: Cell<u64>,
    events: RefCell<BTreeMap<GraphRuntimeSubscriptionToken, Rc<RuntimeEventEntry>>>,
}

impl RuntimeEventHub {
    pub(super) fn new(owner: RuntimeId) -> Self {
        Self {
            owner,
            next: Cell::new(1),
            events: RefCell::new(BTreeMap::new()),
        }
    }

    pub(super) fn subscribe(
        &self,
        callback: impl FnMut(GraphRuntimeEvent) + 'static,
    ) -> Option<GraphRuntimeSubscriptionToken> {
        let value = self.next.get();
        self.next.set(value.checked_add(1)?);
        let token = GraphRuntimeSubscriptionToken {
            runtime: self.owner,
            local: value,
        };
        self.events.borrow_mut().insert(
            token,
            Rc::new(RuntimeEventEntry {
                active: Cell::new(true),
                callback: RefCell::new(Box::new(callback)),
            }),
        );
        Some(token)
    }

    pub(super) fn unsubscribe(&self, token: GraphRuntimeSubscriptionToken) -> bool {
        if token.runtime != self.owner {
            return false;
        }
        let Some(entry) = self.events.borrow_mut().remove(&token) else {
            return false;
        };
        entry.active.set(false);
        true
    }

    pub(super) fn publish(&self, event: GraphRuntimeEvent) -> Option<PanicPayload> {
        let entries = self.events.borrow().values().cloned().collect::<Vec<_>>();
        let mut panics = PanicCollector::default();
        for entry in entries {
            if entry.active.get() {
                panics.catch(|| (entry.callback.borrow_mut())(event.clone()));
            }
        }
        panics.into_payload()
    }

    pub(super) fn clear(&self) {
        for entry in self.events.borrow_mut().values() {
            entry.active.set(false);
        }
        self.events.borrow_mut().clear();
    }
}
