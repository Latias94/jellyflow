use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use jellyflow::core::NodeId;
use jellyflow::runtime::{
    GraphRevision, NodeIncarnation, NodeInstanceKey, ViewId, ViewSessionError, ViewSessionEvent,
    ViewSubscriptionToken, WeakViewSession,
};

use crate::draft::{DraftPhase, DraftRestore, ExternalizedDraft, RevisionedDraft};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct RetainedFieldStoreKey {
    instance: NodeInstanceKey,
    field: String,
}

impl RetainedFieldStoreKey {
    fn new(instance: NodeInstanceKey, field: impl Into<String>) -> Self {
        Self {
            instance,
            field: field.into(),
        }
    }
}

struct RetainedFieldStoreBinding {
    view: WeakViewSession,
    subscription: ViewSubscriptionToken,
}

struct RetainedFieldStoreState<T> {
    bindings: BTreeMap<ViewId, RetainedFieldStoreBinding>,
    entries: BTreeMap<(ViewId, NodeId), BTreeMap<RetainedFieldStoreKey, ExternalizedDraft<T>>>,
    len: usize,
}

impl<T> Default for RetainedFieldStoreState<T> {
    fn default() -> Self {
        Self {
            bindings: BTreeMap::new(),
            entries: BTreeMap::new(),
            len: 0,
        }
    }
}

impl<T> RetainedFieldStoreState<T> {
    fn handle_view_event(&mut self, view: ViewId, event: ViewSessionEvent) {
        match event {
            ViewSessionEvent::RuntimePatched { patch } => {
                for (node, current) in patch.node_incarnation_changes() {
                    self.reconcile_node_incarnation(view, node, current);
                }
            }
            ViewSessionEvent::Closed { .. } => {
                self.clear_view(view);
                self.bindings.remove(&view);
            }
            ViewSessionEvent::ViewportChanged { .. }
            | ViewSessionEvent::SelectionChanged { .. }
            | ViewSessionEvent::LayoutPublished { .. } => {}
        }
    }

    fn ensure_bound_instance(
        &self,
        instance: &NodeInstanceKey,
    ) -> Result<(), RetainedFieldStoreError> {
        if self.bindings.contains_key(&instance.view()) {
            Ok(())
        } else {
            Err(RetainedFieldStoreError::NotBound {
                view: instance.view(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use jellyflow::core::{GraphBuilder, GraphId};
    use jellyflow::runtime::{GraphRuntime, ViewSessionOptions};

    use super::RetainedFieldStore;

    #[test]
    fn clone_reuses_bindings_and_the_last_handle_releases_state() {
        let graph = GraphBuilder::new(GraphId::from_u128(0x5_707e))
            .build()
            .expect("retained field graph");
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("retained field runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("retained field view");
        let store = RetainedFieldStore::<String>::new();
        store
            .ensure_bound(view.id(), &view.downgrade())
            .expect("bind retained field store");
        store
            .ensure_bound(view.id(), &view.downgrade())
            .expect("reuse retained field store binding");
        assert_eq!(store.state.borrow().bindings.len(), 1);
        let shared = Rc::downgrade(&store.state);
        let clone = store.clone();

        drop(store);
        assert!(shared.upgrade().is_some());

        drop(clone);
        assert!(shared.upgrade().is_none());
    }

    #[test]
    fn closing_one_view_removes_only_that_views_binding() {
        let graph = GraphBuilder::new(GraphId::from_u128(0x5_707f))
            .build()
            .expect("retained field graph");
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("retained field runtime");
        let first = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("first retained field view");
        let second = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("second retained field view");
        let store = RetainedFieldStore::<String>::new();
        store
            .ensure_bound(first.id(), &first.downgrade())
            .expect("bind first view");
        store
            .ensure_bound(second.id(), &second.downgrade())
            .expect("bind second view");
        assert_eq!(store.state.borrow().bindings.len(), 2);

        first.close().expect("close first view");
        assert_eq!(store.state.borrow().bindings.len(), 1);
        assert!(store.state.borrow().bindings.contains_key(&second.id()));

        second.close().expect("close second view");
        assert!(store.state.borrow().bindings.is_empty());
    }
}

impl<T> Drop for RetainedFieldStoreState<T> {
    fn drop(&mut self) {
        for (_, binding) in std::mem::take(&mut self.bindings) {
            let _ = binding.view.unsubscribe(binding.subscription);
        }
    }
}

/// Access failure for an unbound retained field store view.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RetainedFieldStoreError {
    #[error("retained field store is not bound to view {view:?}")]
    NotBound { view: ViewId },
}

/// Application-owned overflow storage for retained native fields of one value type.
///
/// A mounted retained field binds its weak view capability exactly once per view. Every clone
/// shares those bindings. A terminal view event clears only that view's fields, and the final store
/// clone automatically unsubscribes every remaining live binding. The store never revives
/// framework tasks, IME state, or interrupted commands.
#[derive(Clone)]
pub struct RetainedFieldStore<T> {
    state: Rc<RefCell<RetainedFieldStoreState<T>>>,
}

impl<T> std::fmt::Debug for RetainedFieldStore<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.borrow();
        formatter
            .debug_struct("RetainedFieldStore")
            .field("bound_views", &state.bindings.len())
            .field("len", &state.len)
            .finish()
    }
}

impl<T> Default for RetainedFieldStore<T> {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(RetainedFieldStoreState::default())),
        }
    }
}

impl<T: 'static> RetainedFieldStore<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn ensure_bound(
        &self,
        view_id: ViewId,
        view: &WeakViewSession,
    ) -> Result<(), ViewSessionError> {
        if self.state.borrow().bindings.contains_key(&view_id) {
            return Ok(());
        }

        let weak_state: Weak<RefCell<RetainedFieldStoreState<T>>> = Rc::downgrade(&self.state);
        let subscription = view.subscribe(move |event| {
            let Some(state) = weak_state.upgrade() else {
                return;
            };
            state.borrow_mut().handle_view_event(view_id, event);
        })?;
        let mut state = self.state.borrow_mut();
        if state.bindings.contains_key(&view_id) {
            let _ = view.unsubscribe(subscription);
            return Ok(());
        }
        state.bindings.insert(
            view_id,
            RetainedFieldStoreBinding {
                view: view.clone(),
                subscription,
            },
        );
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.state.borrow().len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes one externalized field after its retained owner reacquires protection.
    pub(crate) fn discard_externalized(
        &self,
        instance: &NodeInstanceKey,
        field: &str,
    ) -> Result<bool, RetainedFieldStoreError> {
        let mut state = self.state.borrow_mut();
        state.ensure_bound_instance(instance)?;
        Ok(state.remove(&RetainedFieldStoreKey::new(instance.clone(), field)))
    }
}

impl<T> RetainedFieldStore<T>
where
    T: Clone + PartialEq + 'static,
{
    /// Captures one unsaved draft after the host's protected-owner budget overflows.
    pub(crate) fn externalize_overflow(
        &self,
        instance: &NodeInstanceKey,
        field: impl Into<String>,
        draft: &RevisionedDraft<T>,
    ) -> Result<bool, RetainedFieldStoreError> {
        let key = RetainedFieldStoreKey::new(instance.clone(), field);
        let mut state = self.state.borrow_mut();
        state.ensure_bound_instance(instance)?;
        if draft.phase() == DraftPhase::Clean && draft.pending_attempt().is_none() {
            return Ok(state.remove(&key));
        }
        state.insert(key, draft.externalized_snapshot());
        Ok(true)
    }

    /// Restores an exact field identity after first retiring older renderer/incarnation identities.
    pub(crate) fn restore(
        &self,
        instance: &NodeInstanceKey,
        field: impl Into<String>,
        revision: GraphRevision,
        authoritative: T,
    ) -> Result<DraftRestore<T>, RetainedFieldStoreError> {
        let key = RetainedFieldStoreKey::new(instance.clone(), field);
        let snapshot = {
            let mut state = self.state.borrow_mut();
            state.ensure_bound_instance(instance)?;
            state.reconcile_node_identity(instance);
            state.take(&key)
        };
        Ok(match snapshot {
            Some(snapshot) => snapshot.restore_against(revision, authoritative),
            None => DraftRestore::fresh(revision, authoritative),
        })
    }
}

impl<T> RetainedFieldStoreState<T> {
    fn remove(&mut self, key: &RetainedFieldStoreKey) -> bool {
        let bucket = (key.instance.view(), key.instance.node());
        let Some(entries) = self.entries.get_mut(&bucket) else {
            return false;
        };
        let removed = entries.remove(key).is_some();
        let empty = entries.is_empty();
        if empty {
            self.entries.remove(&bucket);
        }
        if removed {
            self.len = self.len.saturating_sub(1);
        }
        removed
    }

    fn reconcile_node_identity(&mut self, current: &NodeInstanceKey) -> usize {
        self.retain_node_bucket(current.view(), current.node(), |key| {
            key.instance == *current
        })
    }

    fn reconcile_node_incarnation(
        &mut self,
        view: ViewId,
        node: NodeId,
        current: Option<NodeIncarnation>,
    ) -> usize {
        self.retain_node_bucket(view, node, |key| {
            current.is_some_and(|current| key.instance.incarnation() == current)
        })
    }

    fn clear_view(&mut self, view: ViewId) -> usize {
        let mut removed = 0;
        self.entries.retain(|(entry_view, _), entries| {
            if *entry_view == view {
                removed += entries.len();
                false
            } else {
                true
            }
        });
        self.len = self.len.saturating_sub(removed);
        removed
    }

    fn retain_node_bucket(
        &mut self,
        view: ViewId,
        node: NodeId,
        mut keep: impl FnMut(&RetainedFieldStoreKey) -> bool,
    ) -> usize {
        let bucket = (view, node);
        let Some(entries) = self.entries.get_mut(&bucket) else {
            return 0;
        };
        let before = entries.len();
        entries.retain(|key, _| keep(key));
        let removed = before.saturating_sub(entries.len());
        if entries.is_empty() {
            self.entries.remove(&bucket);
        }
        self.len = self.len.saturating_sub(removed);
        removed
    }

    fn insert(
        &mut self,
        key: RetainedFieldStoreKey,
        draft: ExternalizedDraft<T>,
    ) -> Option<ExternalizedDraft<T>> {
        let bucket = (key.instance.view(), key.instance.node());
        let previous = self.entries.entry(bucket).or_default().insert(key, draft);
        if previous.is_none() {
            self.len = self.len.saturating_add(1);
        }
        previous
    }

    fn take(&mut self, key: &RetainedFieldStoreKey) -> Option<ExternalizedDraft<T>> {
        let bucket = (key.instance.view(), key.instance.node());
        let entries = self.entries.get_mut(&bucket)?;
        let draft = entries.remove(key);
        if entries.is_empty() {
            self.entries.remove(&bucket);
        }
        if draft.is_some() {
            self.len = self.len.saturating_sub(1);
        }
        draft
    }
}
