use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use crate::runtime::view_session::{ViewGeneration, ViewId, ViewShared};

/// Owns live view registration independently from the borrow-heavy graph transaction state.
///
/// Runtime retirement can therefore synchronously revoke every view even when a middleware call
/// currently holds the canonical graph state mutably borrowed.
#[derive(Default)]
pub(super) struct RuntimeViewRegistry {
    sessions: RefCell<BTreeMap<ViewId, Weak<ViewShared>>>,
}

impl RuntimeViewRegistry {
    pub(super) fn register(&self, id: ViewId, view: &Rc<ViewShared>) {
        let previous = self.sessions.borrow_mut().insert(id, Rc::downgrade(view));
        debug_assert!(
            previous.is_none(),
            "view identities must be unique per runtime"
        );
    }

    pub(super) fn unregister(&self, id: ViewId) {
        self.sessions.borrow_mut().remove(&id);
    }

    /// Collects views that can receive the next patch without changing registration ownership.
    ///
    /// Closing views remain registered until explicit close completion or runtime retirement.
    /// Removing them here would prevent a concurrent runtime drop from installing the stronger
    /// `RuntimeDropped` terminal reason.
    pub(super) fn collect_open_for_patch(&self) -> Vec<(Rc<ViewShared>, ViewGeneration)> {
        let mut sessions = self.sessions.borrow_mut();
        let mut live = Vec::with_capacity(sessions.len());
        sessions.retain(|_, session| {
            let Some(session) = session.upgrade() else {
                return false;
            };
            if let Some(generation) = session.open_generation() {
                live.push((session, generation));
            }
            true
        });
        live
    }

    pub(super) fn take_registered(&self) -> Vec<Rc<ViewShared>> {
        std::mem::take(&mut *self.sessions.borrow_mut())
            .into_values()
            .filter_map(|session| session.upgrade())
            .collect()
    }

    pub(super) fn live_count(&self) -> usize {
        self.sessions
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .filter(|session| session.open_generation().is_some())
            .count()
    }
}
