use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::rc::{Rc, Weak};

use jellyflow_core::NodeId;
use jellyflow_runtime::{
    graph::GraphRevision, graph::NodeLifetimeKey, view::NodeInstanceKey, view::ViewId,
    view::ViewProvenance, view::ViewSessionError, view::ViewSessionEvent,
    view::ViewSubscriptionToken, view::WeakViewSession,
};

use crate::draft::{DraftPhase, DraftRestore, ExternalizedDraft, RevisionedDraft};

/// Default maximum number of externalized field identities retained by one store.
pub const OPEN_GPUI_DEFAULT_RETAINED_FIELD_STORE_CAPACITY: usize = 1_024;

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
    provenance: ViewProvenance,
    view: WeakViewSession,
    subscription: ViewSubscriptionToken,
}

struct RetainedFieldStoreState<T> {
    bindings: BTreeMap<ViewId, RetainedFieldStoreBinding>,
    entries:
        BTreeMap<(ViewProvenance, NodeId), BTreeMap<RetainedFieldStoreKey, ExternalizedDraft<T>>>,
    len: usize,
    capacity: usize,
}

impl<T> Default for RetainedFieldStoreState<T> {
    fn default() -> Self {
        Self {
            bindings: BTreeMap::new(),
            entries: BTreeMap::new(),
            len: 0,
            capacity: OPEN_GPUI_DEFAULT_RETAINED_FIELD_STORE_CAPACITY,
        }
    }
}

impl<T> RetainedFieldStoreState<T> {
    fn with_capacity(capacity: NonZeroUsize) -> Self {
        Self {
            bindings: BTreeMap::new(),
            entries: BTreeMap::new(),
            len: 0,
            capacity: capacity.get(),
        }
    }

    fn handle_view_event(&mut self, provenance: ViewProvenance, event: ViewSessionEvent) {
        match event {
            ViewSessionEvent::RuntimePatched { patch } => {
                for node in patch.affected_nodes().iter().copied() {
                    if patch.node_lifetime_change(node).is_some() {
                        self.reconcile_node_lifetime(
                            provenance,
                            node,
                            patch.snapshot().node_lifetime_key(node),
                        );
                    }
                }
            }
            ViewSessionEvent::Closed { .. } => {
                self.clear_view(provenance);
                if self
                    .bindings
                    .get(&provenance.view())
                    .is_some_and(|binding| binding.provenance == provenance)
                {
                    self.bindings.remove(&provenance.view());
                }
            }
            ViewSessionEvent::ViewportChanged { .. }
            | ViewSessionEvent::SelectionChanged { .. }
            | ViewSessionEvent::LayoutPublished { .. } => {}
        }
    }

    fn ensure_bound_instance(
        &self,
        instance: &NodeInstanceKey,
    ) -> Result<ViewProvenance, RetainedFieldStoreError> {
        self.bindings
            .get(&instance.view())
            .map(|binding| binding.provenance)
            .filter(|provenance| provenance.runtime() == instance.lifetime().provenance())
            .ok_or(RetainedFieldStoreError::NotBound {
                view: instance.view(),
            })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::rc::Rc;

    use jellyflow_core::{
        CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    };
    use jellyflow_runtime::{
        graph::GraphRuntime, view::FrameRequest, view::NodeInstanceKey, view::RendererStamp,
        view::ViewSession, view::ViewSessionOptions,
    };

    use super::{RetainedFieldStore, RetainedFieldStoreError};
    use crate::draft::{DraftPhase, RevisionedDraft};

    fn node_id() -> NodeId {
        NodeId::from_u128(0x0057_07e1)
    }

    fn runtime_with_node() -> GraphRuntime {
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x0057_07e2));
        graph.insert_node(
            node_id(),
            Node {
                kind: NodeKindKey::new("test.retained-field-store"),
                kind_version: 1,
                pos: CanvasPoint::default(),
                origin: None,
                selectable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: Some(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                }),
                hidden: false,
                collapsed: false,
                ports: Vec::new(),
                data: serde_json::Value::Null,
            },
        );
        GraphRuntime::builder(Default::default())
            .build(graph.build().expect("retained field store graph"))
            .expect("retained field store runtime")
    }

    fn instance_for(view: &ViewSession) -> NodeInstanceKey {
        let frame = view
            .frame(FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint { x: -10.0, y: -10.0 },
                    size: CanvasSize {
                        width: 300.0,
                        height: 200.0,
                    },
                },
                RendererStamp::new("test.retained-field-store", 1),
            ))
            .expect("retained field store frame");
        frame.nodes()[0].instance().clone()
    }

    fn dirty_draft(local: &str) -> RevisionedDraft<String> {
        let mut draft = RevisionedDraft::new(1, "remote".to_owned());
        draft.edit_local(local.to_owned());
        draft
    }

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
            .ensure_bound(&view.downgrade())
            .expect("bind retained field store");
        store
            .ensure_bound(&view.downgrade())
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
            .ensure_bound(&first.downgrade())
            .expect("bind first view");
        store
            .ensure_bound(&second.downgrade())
            .expect("bind second view");
        assert_eq!(store.state.borrow().bindings.len(), 2);

        first.close().expect("close first view");
        assert_eq!(store.state.borrow().bindings.len(), 1);
        assert!(store.state.borrow().bindings.contains_key(&second.id()));

        second.close().expect("close second view");
        assert!(store.state.borrow().bindings.is_empty());
    }

    #[test]
    fn binding_is_scoped_to_exact_runtime_provenance_even_when_local_view_ids_match() {
        let first_runtime = runtime_with_node();
        let first = first_runtime
            .attach_view(ViewSessionOptions::default())
            .expect("first retained field store view");
        let second_runtime = runtime_with_node();
        let second = second_runtime
            .attach_view(ViewSessionOptions::default())
            .expect("second retained field store view");
        assert_eq!(first.id().get(), second.id().get());

        let store = RetainedFieldStore::new();
        store
            .ensure_bound(&first.downgrade())
            .expect("bind first runtime view");
        let second_instance = instance_for(&second);
        assert!(matches!(
            store.externalize_overflow(
                &second_instance,
                "field",
                &dirty_draft("foreign runtime local"),
            ),
            Err(RetainedFieldStoreError::NotBound { view }) if view == second.id()
        ));
        assert!(store.is_empty());
    }

    #[test]
    fn capacity_rejects_a_new_identity_without_evicting_existing_draft() {
        let runtime = runtime_with_node();
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("retained field store view");
        let instance = instance_for(&view);
        let store = RetainedFieldStore::with_capacity(NonZeroUsize::new(1).unwrap());
        store
            .ensure_bound(&view.downgrade())
            .expect("bind retained field store");

        assert!(
            store
                .externalize_overflow(&instance, "first", &dirty_draft("first local"))
                .expect("externalize first draft")
        );
        assert_eq!(store.len(), 1);
        assert_eq!(store.capacity(), 1);
        assert_eq!(
            store
                .externalize_overflow(&instance, "second", &dirty_draft("second local"))
                .expect_err("a new identity must not evict the first draft"),
            RetainedFieldStoreError::CapacityExceeded { capacity: 1 }
        );
        assert_eq!(store.len(), 1);

        let absent = store
            .restore(&instance, "second", 2, "second remote".to_owned())
            .expect("restore absent second field")
            .into_draft();
        assert_eq!(absent.local(), "second remote");
        assert_eq!(absent.phase(), DraftPhase::Clean);
        assert_eq!(store.len(), 1);

        let preserved = store
            .restore(&instance, "first", 2, "first remote".to_owned())
            .expect("restore preserved first field")
            .into_draft();
        assert_eq!(preserved.local(), "first local");
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn exact_identity_refresh_is_allowed_at_capacity_and_restores_latest_draft() {
        let runtime = runtime_with_node();
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("retained field store view");
        let instance = instance_for(&view);
        let store = RetainedFieldStore::with_capacity(NonZeroUsize::new(1).unwrap());
        store
            .ensure_bound(&view.downgrade())
            .expect("bind retained field store");

        store
            .externalize_overflow(&instance, "field", &dirty_draft("first local"))
            .expect("externalize first draft");
        store
            .externalize_overflow(&instance, "field", &dirty_draft("latest local"))
            .expect("refresh exact field at capacity");
        assert_eq!(store.len(), 1);

        let restored = store
            .restore(&instance, "field", 2, "remote".to_owned())
            .expect("restore latest draft")
            .into_draft();
        assert_eq!(restored.local(), "latest local");
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn repeated_externalize_restore_cycles_keep_capacity_accounting_exact() {
        let runtime = runtime_with_node();
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("retained field store view");
        let instance = instance_for(&view);
        let store = RetainedFieldStore::with_capacity(NonZeroUsize::new(1).unwrap());
        store
            .ensure_bound(&view.downgrade())
            .expect("bind retained field store");

        for cycle in 0..64 {
            let local = format!("local {cycle}");
            store
                .externalize_overflow(&instance, "field", &dirty_draft(&local))
                .expect("externalize cycle draft");
            assert_eq!(store.len(), 1);

            let restored = store
                .restore(&instance, "field", 2, "remote".to_owned())
                .expect("restore cycle draft")
                .into_draft();
            assert_eq!(restored.local(), &local);
            assert!(store.is_empty());
        }
    }

    #[test]
    fn terminal_view_retirement_releases_capacity_for_another_view() {
        let runtime = runtime_with_node();
        let first = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("first retained field store view");
        let second = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("second retained field store view");
        let first_instance = instance_for(&first);
        let second_instance = instance_for(&second);
        let store = RetainedFieldStore::with_capacity(NonZeroUsize::new(1).unwrap());
        store
            .ensure_bound(&first.downgrade())
            .expect("bind first retained field store view");
        store
            .ensure_bound(&second.downgrade())
            .expect("bind second retained field store view");

        store
            .externalize_overflow(&first_instance, "field", &dirty_draft("first local"))
            .expect("externalize first view draft");
        assert!(matches!(
            store.externalize_overflow(&second_instance, "field", &dirty_draft("second local"),),
            Err(RetainedFieldStoreError::CapacityExceeded { capacity: 1 })
        ));

        first.close().expect("retire first view");
        assert!(store.is_empty());
        store
            .externalize_overflow(&second_instance, "field", &dirty_draft("second local"))
            .expect("reuse capacity after first view retirement");
        assert_eq!(store.len(), 1);

        second.close().expect("retire second view");
        assert!(store.is_empty());
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
    #[error("retained field store capacity {capacity} is exhausted")]
    CapacityExceeded { capacity: usize },
}

/// Application-owned overflow storage for retained native fields of one value type.
///
/// A mounted retained field binds its weak view capability exactly once per view generation. Every
/// clone shares those bindings. Entries are bucketed by the complete [`ViewProvenance`], so a
/// terminal callback cannot clear or revive drafts owned by another runtime or generation. The
/// final store clone automatically unsubscribes every remaining live binding. Capacity is global
/// across those clones and views, and defaults to
/// [`OPEN_GPUI_DEFAULT_RETAINED_FIELD_STORE_CAPACITY`]. The store never silently evicts an unsaved
/// draft and never revives framework tasks, IME state, or interrupted commands.
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
            .field("capacity", &state.capacity)
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a store with an explicit global externalized-field capacity.
    ///
    /// Capacity is shared by every clone and bound view. At capacity, refreshing an existing
    /// exact field identity remains valid, while adding another identity returns
    /// [`RetainedFieldStoreError::CapacityExceeded`] without evicting an unsaved draft.
    #[must_use]
    pub fn with_capacity(capacity: NonZeroUsize) -> Self {
        Self {
            state: Rc::new(RefCell::new(RetainedFieldStoreState::with_capacity(
                capacity,
            ))),
        }
    }

    pub(crate) fn ensure_bound(&self, view: &WeakViewSession) -> Result<(), ViewSessionError> {
        let provenance = view.provenance()?;
        if self
            .state
            .borrow()
            .bindings
            .get(&provenance.view())
            .is_some_and(|binding| binding.provenance == provenance)
        {
            return Ok(());
        }

        let weak_state: Weak<RefCell<RetainedFieldStoreState<T>>> = Rc::downgrade(&self.state);
        let subscription = view.subscribe(move |event| {
            let Some(state) = weak_state.upgrade() else {
                return;
            };
            state.borrow_mut().handle_view_event(provenance, event);
        })?;
        let mut state = self.state.borrow_mut();
        if state
            .bindings
            .get(&provenance.view())
            .is_some_and(|binding| binding.provenance == provenance)
        {
            let _ = view.unsubscribe(subscription);
            return Ok(());
        }
        if state.bindings.contains_key(&provenance.view()) {
            drop(state);
            let _ = view.unsubscribe(subscription);
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        state.bindings.insert(
            provenance.view(),
            RetainedFieldStoreBinding {
                provenance,
                view: view.clone(),
                subscription,
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.state.borrow().len
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.state.borrow().capacity
    }

    #[must_use]
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
        let provenance = state.ensure_bound_instance(instance)?;
        Ok(state.remove(
            provenance,
            &RetainedFieldStoreKey::new(instance.clone(), field),
        ))
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
        let provenance = state.ensure_bound_instance(instance)?;
        if draft.phase() == DraftPhase::Clean && draft.pending_attempt().is_none() {
            return Ok(state.remove(provenance, &key));
        }
        state.insert(provenance, key, draft.externalized_snapshot())?;
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
            let provenance = state.ensure_bound_instance(instance)?;
            state.reconcile_node_identity(provenance, instance);
            state.take(provenance, &key)
        };
        Ok(match snapshot {
            Some(snapshot) => snapshot.restore_against(revision, authoritative),
            None => DraftRestore::fresh(revision, authoritative),
        })
    }
}

impl<T> RetainedFieldStoreState<T> {
    fn remove(&mut self, provenance: ViewProvenance, key: &RetainedFieldStoreKey) -> bool {
        let bucket = (provenance, key.instance.node());
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

    fn reconcile_node_identity(
        &mut self,
        provenance: ViewProvenance,
        current: &NodeInstanceKey,
    ) -> usize {
        self.retain_node_bucket(provenance, current.node(), |key| key.instance == *current)
    }

    fn reconcile_node_lifetime(
        &mut self,
        provenance: ViewProvenance,
        node: NodeId,
        current: Option<NodeLifetimeKey>,
    ) -> usize {
        self.retain_node_bucket(provenance, node, |key| {
            current.is_some_and(|current| key.instance.lifetime() == current)
        })
    }

    fn clear_view(&mut self, provenance: ViewProvenance) -> usize {
        let mut removed = 0;
        self.entries.retain(|(entry_view, _), entries| {
            if *entry_view == provenance {
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
        provenance: ViewProvenance,
        node: NodeId,
        mut keep: impl FnMut(&RetainedFieldStoreKey) -> bool,
    ) -> usize {
        let bucket = (provenance, node);
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
        provenance: ViewProvenance,
        key: RetainedFieldStoreKey,
        draft: ExternalizedDraft<T>,
    ) -> Result<Option<ExternalizedDraft<T>>, RetainedFieldStoreError> {
        let bucket = (provenance, key.instance.node());
        let already_present = self
            .entries
            .get(&bucket)
            .is_some_and(|entries| entries.contains_key(&key));
        if !already_present && self.len >= self.capacity {
            return Err(RetainedFieldStoreError::CapacityExceeded {
                capacity: self.capacity,
            });
        }
        let previous = self.entries.entry(bucket).or_default().insert(key, draft);
        if previous.is_none() {
            self.len = self.len.saturating_add(1);
        }
        Ok(previous)
    }

    fn take(
        &mut self,
        provenance: ViewProvenance,
        key: &RetainedFieldStoreKey,
    ) -> Option<ExternalizedDraft<T>> {
        let bucket = (provenance, key.instance.node());
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
