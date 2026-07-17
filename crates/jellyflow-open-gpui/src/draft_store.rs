use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow::core::NodeId;
use jellyflow::runtime::{
    GraphRevision, NodeIncarnation, NodeInstanceKey, ViewId, ViewSession, ViewSessionError,
    ViewSessionEvent, ViewSubscriptionToken,
};

use crate::draft::{DraftPhase, DraftRestore, ExternalizedDraft, RevisionedDraft};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DraftStoreKey {
    instance: NodeInstanceKey,
    field: String,
}

impl DraftStoreKey {
    fn new(instance: NodeInstanceKey, field: impl Into<String>) -> Self {
        Self {
            instance,
            field: field.into(),
        }
    }
}

#[derive(Debug)]
struct DraftStoreState<T> {
    entries: BTreeMap<(ViewId, NodeId), BTreeMap<DraftStoreKey, ExternalizedDraft<T>>>,
    len: usize,
}

impl<T> Default for DraftStoreState<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            len: 0,
        }
    }
}

/// Application-owned overflow storage for retained native fields of one value type.
///
/// The adapter owns identity retirement and restore reconciliation while the application owns this
/// handle and decides which product fields use it. The store never retries an interrupted command.
#[derive(Clone, Debug)]
pub struct ExternalizedDraftStore<T> {
    state: Rc<RefCell<DraftStoreState<T>>>,
}

impl<T> Default for ExternalizedDraftStore<T> {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(DraftStoreState::default())),
        }
    }
}

impl<T> ExternalizedDraftStore<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.state.borrow().len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes one externalized field after its retained owner reacquires protection.
    pub fn discard_externalized(&self, instance: &NodeInstanceKey, field: &str) -> bool {
        self.state
            .borrow_mut()
            .remove(&DraftStoreKey::new(instance.clone(), field))
    }

    /// Binds identity cleanup to one view capability.
    pub fn bind(&self, view: &ViewSession) -> Result<ViewSubscriptionToken, ViewSessionError>
    where
        T: 'static,
    {
        let view_id = view.id();
        let store = Self {
            state: Rc::clone(&self.state),
        };
        view.subscribe(move |event| match event {
            ViewSessionEvent::RuntimePatched { patch } => {
                let mut state = store.state.borrow_mut();
                for (node, current) in patch.node_incarnation_changes() {
                    state.reconcile_node_incarnation(view_id, node, current);
                }
            }
            ViewSessionEvent::Closed { .. } => {
                store.state.borrow_mut().clear_view(view_id);
            }
            ViewSessionEvent::ViewportChanged { .. }
            | ViewSessionEvent::SelectionChanged { .. }
            | ViewSessionEvent::LayoutPublished { .. } => {}
        })
    }
}

impl<T> ExternalizedDraftStore<T>
where
    T: Clone + PartialEq,
{
    /// Captures one unsaved draft after the host's protected-owner budget overflows.
    pub fn externalize_overflow(
        &self,
        instance: &NodeInstanceKey,
        field: impl Into<String>,
        draft: &RevisionedDraft<T>,
    ) -> bool {
        let key = DraftStoreKey::new(instance.clone(), field);
        let mut state = self.state.borrow_mut();
        if draft.phase() == DraftPhase::Clean && draft.pending_attempt().is_none() {
            return state.remove(&key);
        }
        state.insert(key, draft.externalized_snapshot());
        true
    }

    /// Restores an exact field identity after first retiring older renderer/incarnation identities.
    pub fn restore(
        &self,
        instance: &NodeInstanceKey,
        field: impl Into<String>,
        revision: GraphRevision,
        authoritative: T,
    ) -> DraftRestore<T> {
        let key = DraftStoreKey::new(instance.clone(), field);
        let snapshot = {
            let mut state = self.state.borrow_mut();
            state.reconcile_node_identity(instance);
            state.take(&key)
        };
        match snapshot {
            Some(snapshot) => snapshot.restore_against(revision, authoritative),
            None => DraftRestore::fresh(revision, authoritative),
        }
    }
}

impl<T> DraftStoreState<T> {
    fn remove(&mut self, key: &DraftStoreKey) -> bool {
        let bucket_key = (key.instance.view(), key.instance.node());
        let Some(entries) = self.entries.get_mut(&bucket_key) else {
            return false;
        };
        let removed = entries.remove(key).is_some();
        let empty = entries.is_empty();
        if empty {
            self.entries.remove(&bucket_key);
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
        mut keep: impl FnMut(&DraftStoreKey) -> bool,
    ) -> usize {
        let bucket_key = (view, node);
        let Some(entries) = self.entries.get_mut(&bucket_key) else {
            return 0;
        };
        let before = entries.len();
        entries.retain(|key, _| keep(key));
        let removed = before.saturating_sub(entries.len());
        if entries.is_empty() {
            self.entries.remove(&bucket_key);
        }
        self.len = self.len.saturating_sub(removed);
        removed
    }

    fn insert(
        &mut self,
        key: DraftStoreKey,
        draft: ExternalizedDraft<T>,
    ) -> Option<ExternalizedDraft<T>> {
        let bucket_key = (key.instance.view(), key.instance.node());
        let previous = self
            .entries
            .entry(bucket_key)
            .or_default()
            .insert(key, draft);
        if previous.is_none() {
            self.len = self.len.saturating_add(1);
        }
        previous
    }

    fn take(&mut self, key: &DraftStoreKey) -> Option<ExternalizedDraft<T>> {
        let bucket_key = (key.instance.view(), key.instance.node());
        let entries = self.entries.get_mut(&bucket_key)?;
        let draft = entries.remove(key);
        if entries.is_empty() {
            self.entries.remove(&bucket_key);
        }
        if draft.is_some() {
            self.len = self.len.saturating_sub(1);
        }
        draft
    }
}
