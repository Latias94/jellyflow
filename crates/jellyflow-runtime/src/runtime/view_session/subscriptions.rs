use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use jellyflow_core::core::NodeId;

use crate::runtime::unwind::{PanicCollector, PanicPayload};

use super::{NodeBindingEvent, NodeBindingKey, ViewCloseReason, ViewId, ViewSessionEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewSubscriptionToken {
    view: ViewId,
    local: u64,
}

struct EventEntry {
    active: Cell<bool>,
    callback: RefCell<Box<dyn FnMut(ViewSessionEvent)>>,
}

struct NodeEntry {
    active: Cell<bool>,
    key: NodeBindingKey,
    callback: RefCell<Box<dyn FnMut(NodeBindingEvent)>>,
}

struct EventDrainGuard<'a>(&'a Cell<bool>);

impl Drop for EventDrainGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

pub(super) struct EventBatchGuard<'a> {
    hub: &'a ViewEventHub,
    active: bool,
}

impl EventBatchGuard<'_> {
    pub(super) fn finish(mut self) -> Option<PanicPayload> {
        self.active = false;
        self.hub.finish_event_batch()
    }
}

impl Drop for EventBatchGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.hub.release_event_batch();
        }
    }
}

pub(super) struct ViewEventHub {
    owner: ViewId,
    next: Cell<u64>,
    publishing_events: Cell<bool>,
    deferred_events: Cell<u32>,
    terminal: Cell<bool>,
    pending_events: RefCell<VecDeque<ViewSessionEvent>>,
    events: RefCell<BTreeMap<ViewSubscriptionToken, Rc<EventEntry>>>,
    nodes: RefCell<BTreeMap<NodeId, BTreeMap<ViewSubscriptionToken, Rc<NodeEntry>>>>,
    node_tokens: RefCell<BTreeMap<ViewSubscriptionToken, NodeId>>,
}

impl ViewEventHub {
    pub(super) fn new(owner: ViewId) -> Self {
        Self {
            owner,
            next: Cell::new(1),
            publishing_events: Cell::new(false),
            deferred_events: Cell::new(0),
            terminal: Cell::new(false),
            pending_events: RefCell::new(VecDeque::new()),
            events: RefCell::new(BTreeMap::new()),
            nodes: RefCell::new(BTreeMap::new()),
            node_tokens: RefCell::new(BTreeMap::new()),
        }
    }

    pub(super) fn subscribe_event(
        &self,
        callback: impl FnMut(ViewSessionEvent) + 'static,
    ) -> Option<ViewSubscriptionToken> {
        let token = self.allocate_token()?;
        self.events.borrow_mut().insert(
            token,
            Rc::new(EventEntry {
                active: Cell::new(true),
                callback: RefCell::new(Box::new(callback)),
            }),
        );
        Some(token)
    }

    pub(super) fn defer_events(&self) -> EventBatchGuard<'_> {
        let depth = self.deferred_events.get();
        self.deferred_events.set(depth.saturating_add(1));
        EventBatchGuard {
            hub: self,
            active: true,
        }
    }

    pub(super) fn subscribe_node(
        &self,
        key: NodeBindingKey,
        callback: impl FnMut(NodeBindingEvent) + 'static,
    ) -> Option<ViewSubscriptionToken> {
        let token = self.allocate_token()?;
        self.nodes
            .borrow_mut()
            .entry(key.node())
            .or_default()
            .insert(
                token,
                Rc::new(NodeEntry {
                    active: Cell::new(true),
                    key,
                    callback: RefCell::new(Box::new(callback)),
                }),
            );
        self.node_tokens.borrow_mut().insert(token, key.node());
        Some(token)
    }

    pub(super) fn unsubscribe(&self, token: ViewSubscriptionToken) -> bool {
        if token.view != self.owner {
            return false;
        }
        if let Some(entry) = self.events.borrow_mut().remove(&token) {
            entry.active.set(false);
            return true;
        }

        let Some(node) = self.node_tokens.borrow_mut().remove(&token) else {
            return false;
        };
        let mut nodes = self.nodes.borrow_mut();
        let Some(entries) = nodes.get_mut(&node) else {
            return false;
        };
        let Some(entry) = entries.remove(&token) else {
            return false;
        };
        entry.active.set(false);
        if entries.is_empty() {
            nodes.remove(&node);
        }
        true
    }

    pub(super) fn publish_event(&self, event: ViewSessionEvent) -> Option<PanicPayload> {
        if self.terminal.get() {
            return None;
        }
        self.pending_events.borrow_mut().push_back(event);
        self.drain_events()
    }

    pub(super) fn publish_terminal(&self, reason: ViewCloseReason) -> Option<PanicPayload> {
        if self.terminal.replace(true) {
            return None;
        }
        let mut pending = self.pending_events.borrow_mut();
        pending.clear();
        pending.push_back(ViewSessionEvent::Closed { reason });
        drop(pending);
        self.drain_events()
    }

    fn finish_event_batch(&self) -> Option<PanicPayload> {
        self.release_event_batch();
        self.drain_events()
    }

    fn release_event_batch(&self) {
        let depth = self.deferred_events.get();
        debug_assert!(depth > 0);
        self.deferred_events.set(depth.saturating_sub(1));
    }

    fn drain_events(&self) -> Option<PanicPayload> {
        if self.deferred_events.get() > 0 {
            return None;
        }
        if self.publishing_events.replace(true) {
            return None;
        }
        let guard = EventDrainGuard(&self.publishing_events);
        let mut panics = PanicCollector::default();
        let mut panicked_entries = BTreeSet::new();
        loop {
            let Some(event) = self.pending_events.borrow_mut().pop_front() else {
                break;
            };
            let entries = self
                .events
                .borrow()
                .iter()
                .map(|(token, entry)| (*token, entry.clone()))
                .collect::<Vec<_>>();
            for (token, entry) in entries {
                if entry.active.get() && !panicked_entries.contains(&token) {
                    let callback_panic =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            (entry.callback.borrow_mut())(event.clone());
                        }))
                        .err();
                    if callback_panic.is_some() {
                        panicked_entries.insert(token);
                    }
                    panics.record(callback_panic);
                }
            }
        }
        if self.terminal.get() {
            self.clear();
        }
        drop(guard);
        panics.into_payload()
    }

    pub(super) fn publish_node(
        &self,
        node: NodeId,
        patch: &crate::runtime::graph_runtime::RuntimePatch,
    ) -> (usize, Option<PanicPayload>) {
        let entries = self
            .nodes
            .borrow()
            .get(&node)
            .map(|entries| entries.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut notified = 0;
        let mut panics = PanicCollector::default();
        for entry in entries {
            if entry.active.get() {
                notified += 1;
                panics.catch(|| {
                    (entry.callback.borrow_mut())(NodeBindingEvent {
                        key: entry.key,
                        patch: patch.clone(),
                    });
                });
            }
        }
        (notified, panics.into_payload())
    }

    pub(super) fn retire_node(&self, node: NodeId) {
        let Some(entries) = self.nodes.borrow_mut().remove(&node) else {
            return;
        };
        let mut node_tokens = self.node_tokens.borrow_mut();
        for (token, entry) in entries {
            entry.active.set(false);
            node_tokens.remove(&token);
        }
    }

    fn clear(&self) {
        for entry in self.events.borrow_mut().values() {
            entry.active.set(false);
        }
        for entries in self.nodes.borrow_mut().values() {
            for entry in entries.values() {
                entry.active.set(false);
            }
        }
        self.events.borrow_mut().clear();
        self.nodes.borrow_mut().clear();
        self.node_tokens.borrow_mut().clear();
        self.pending_events.borrow_mut().clear();
    }

    fn allocate_token(&self) -> Option<ViewSubscriptionToken> {
        let value = self.next.get();
        let next = value.checked_add(1)?;
        self.next.set(next);
        Some(ViewSubscriptionToken {
            view: self.owner,
            local: value,
        })
    }
}
