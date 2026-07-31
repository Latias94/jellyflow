use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::mem;
use std::rc::Rc;

use jellyflow_runtime::view::NodeInstanceKey;
use open_gpui::{HitboxId, Window};
use thiserror::Error;

use crate::panic_sink::OpenGpuiPanicAccumulator;

/// A native resource or interaction that remains owned by a retained node.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OpenGpuiActivityKind {
    Focus,
    Ime,
    GraphInteraction,
    Overlay,
    Task,
    Subscription,
    Draft,
    Mutation,
}

impl OpenGpuiActivityKind {
    fn protects_retention(self) -> bool {
        !matches!(self, Self::Task | Self::Subscription)
    }

    fn requires_window_cleanup(self) -> bool {
        matches!(
            self,
            Self::Focus | Self::Ime | Self::GraphInteraction | Self::Overlay
        )
    }

    fn requires_interaction_island(self) -> bool {
        matches!(self, Self::Focus | Self::Ime | Self::Overlay)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivityEntryKind {
    Activity(OpenGpuiActivityKind),
    PointerCapture(HitboxId),
}

impl ActivityEntryKind {
    fn protects_retention(self) -> bool {
        match self {
            Self::Activity(kind) => kind.protects_retention(),
            Self::PointerCapture(_) => true,
        }
    }

    fn requires_window_cleanup(self) -> bool {
        match self {
            Self::Activity(kind) => kind.requires_window_cleanup(),
            Self::PointerCapture(_) => true,
        }
    }

    fn requires_interaction_island(self) -> bool {
        match self {
            Self::Activity(kind) => kind.requires_interaction_island(),
            Self::PointerCapture(_) => true,
        }
    }

    fn pointer_capture(self) -> Option<HitboxId> {
        match self {
            Self::PointerCapture(hitbox) => Some(hitbox),
            Self::Activity(_) => None,
        }
    }
}

/// Failure to acquire an activity lease.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OpenGpuiActivityAcquireError {
    #[error("the node activity registry has been revoked")]
    RegistryRevoked,
    #[error("the node activity registry exhausted its lease identity space")]
    IdentityExhausted,
    #[error("the view already protects its maximum of {capacity} native node instances")]
    ProtectedCapacityExceeded { capacity: usize },
}

type RevocationCallback = Box<dyn FnOnce() + 'static>;

#[derive(Clone)]
pub(crate) struct OpenGpuiActivityCoordinator {
    state: Rc<RefCell<ActivityCoordinatorState>>,
}

struct ActivityCoordinatorState {
    capacity: usize,
    protected: BTreeMap<NodeInstanceKey, usize>,
    window_owned_leases: usize,
    protected_high_water: usize,
    overflows: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OpenGpuiActivityCoordinatorStats {
    pub(crate) protected_instances: usize,
    pub(crate) protected_leases: usize,
    pub(crate) protected_high_water: usize,
    pub(crate) overflows: u64,
}

impl OpenGpuiActivityCoordinator {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(ActivityCoordinatorState {
                capacity,
                protected: BTreeMap::new(),
                window_owned_leases: 0,
                protected_high_water: 0,
                overflows: 0,
            })),
        }
    }

    fn acquire(
        &self,
        instance: &NodeInstanceKey,
        kind: ActivityEntryKind,
    ) -> Result<(), OpenGpuiActivityAcquireError> {
        let mut state = self.state.borrow_mut();
        if let Some(count) = state.protected.get_mut(instance) {
            *count = count.saturating_add(1);
            if kind.requires_window_cleanup() {
                state.window_owned_leases = state.window_owned_leases.saturating_add(1);
            }
            return Ok(());
        }
        if state.protected.len() >= state.capacity {
            state.overflows = state.overflows.saturating_add(1);
            return Err(OpenGpuiActivityAcquireError::ProtectedCapacityExceeded {
                capacity: state.capacity,
            });
        }
        state.protected.insert(instance.clone(), 1);
        if kind.requires_window_cleanup() {
            state.window_owned_leases = state.window_owned_leases.saturating_add(1);
        }
        state.protected_high_water = state.protected_high_water.max(state.protected.len());
        Ok(())
    }

    fn release(&self, instance: &NodeInstanceKey, leases: usize, window_owned_leases: usize) {
        if leases == 0 {
            return;
        }
        let mut state = self.state.borrow_mut();
        let Some(count) = state.protected.get_mut(instance) else {
            return;
        };
        if *count <= leases {
            state.protected.remove(instance);
        } else {
            *count -= leases;
        }
        state.window_owned_leases = state
            .window_owned_leases
            .saturating_sub(window_owned_leases);
    }

    pub(crate) fn requires_window_cleanup(&self) -> bool {
        self.state.borrow().window_owned_leases > 0
    }

    pub(crate) fn stats(&self) -> OpenGpuiActivityCoordinatorStats {
        let state = self.state.borrow();
        OpenGpuiActivityCoordinatorStats {
            protected_instances: state.protected.len(),
            protected_leases: state.protected.values().copied().sum(),
            protected_high_water: state.protected_high_water,
            overflows: state.overflows,
        }
    }
}

#[derive(Clone)]
struct CoordinatedInstance {
    instance: NodeInstanceKey,
    coordinator: OpenGpuiActivityCoordinator,
}

struct ActivityEntry {
    kind: ActivityEntryKind,
    revoke: Option<RevocationCallback>,
}

struct ActivityState {
    next_id: Option<u64>,
    revoked: bool,
    entries: BTreeMap<u64, ActivityEntry>,
    coordinated_instance: Option<CoordinatedInstance>,
}

impl Default for ActivityState {
    fn default() -> Self {
        Self {
            next_id: Some(1),
            revoked: false,
            entries: BTreeMap::new(),
            coordinated_instance: None,
        }
    }
}

/// A single-threaded activity ledger shared by a retained node and its leases.
#[derive(Clone)]
pub(crate) struct OpenGpuiNodeActivityRegistry {
    state: Rc<RefCell<ActivityState>>,
}

/// Renderer-owned capability for acquiring and inspecting native activity leases.
///
/// Revocation remains owned by the scene host, so a custom node cannot invalidate sibling
/// resources that share its mount-scoped activity ledger.
///
/// ```compile_fail
/// use jellyflow_open_gpui::activity::OpenGpuiNodeActivityHandle;
///
/// fn revoke_host_activity(activity: &OpenGpuiNodeActivityHandle) {
///     activity.revoke_all();
/// }
/// ```
#[derive(Clone)]
pub struct OpenGpuiNodeActivityHandle {
    registry: OpenGpuiNodeActivityRegistry,
}

impl OpenGpuiNodeActivityHandle {
    pub fn acquire(
        &self,
        kind: OpenGpuiActivityKind,
    ) -> Result<OpenGpuiActivityLease, OpenGpuiActivityAcquireError> {
        self.registry.acquire(kind)
    }

    pub fn acquire_with_revocation(
        &self,
        kind: OpenGpuiActivityKind,
        revoke: impl FnOnce() + 'static,
    ) -> Result<OpenGpuiActivityLease, OpenGpuiActivityAcquireError> {
        self.registry.acquire_with_revocation(kind, revoke)
    }

    /// Captures the window pointer for one exact hitbox and registers that ownership with the host.
    ///
    /// The returned lease keeps the node resident and lets terminal host cleanup release only this
    /// capture. Call [`OpenGpuiPointerCaptureLease::release`] when the component finishes normally.
    pub fn capture_pointer(
        &self,
        hitbox: HitboxId,
        window: &mut Window,
    ) -> Result<OpenGpuiPointerCaptureLease, OpenGpuiActivityAcquireError> {
        let activity = self.registry.acquire_pointer_capture(hitbox)?;
        window.capture_pointer(hitbox);
        Ok(OpenGpuiPointerCaptureLease {
            hitbox,
            activity: Some(activity),
        })
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.registry.active_count()
    }

    /// Returns whether this retained node currently owns an activity lease of `kind`.
    #[must_use]
    pub fn has_activity(&self, kind: OpenGpuiActivityKind) -> bool {
        self.registry.has_activity(kind)
    }

    #[must_use]
    pub fn protected_count(&self) -> usize {
        self.registry.protected_count()
    }

    #[must_use]
    pub fn is_active(&self, id: u64) -> bool {
        self.registry.is_active(id)
    }

    #[must_use]
    pub fn is_revoked(&self) -> bool {
        self.registry.is_revoked()
    }
}

impl fmt::Debug for OpenGpuiNodeActivityHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenGpuiNodeActivityHandle")
            .field("active_count", &self.active_count())
            .field("protected_count", &self.protected_count())
            .field("revoked", &self.is_revoked())
            .finish()
    }
}

impl OpenGpuiNodeActivityRegistry {
    fn detached() -> Self {
        Self {
            state: Rc::new(RefCell::new(ActivityState::default())),
        }
    }

    pub(crate) fn for_host(
        instance: NodeInstanceKey,
        coordinator: OpenGpuiActivityCoordinator,
    ) -> Self {
        let registry = Self::detached();
        registry.state.borrow_mut().coordinated_instance = Some(CoordinatedInstance {
            instance,
            coordinator,
        });
        registry
    }

    pub(crate) fn renderer_handle(&self) -> OpenGpuiNodeActivityHandle {
        OpenGpuiNodeActivityHandle {
            registry: self.clone(),
        }
    }

    pub(crate) fn acquire(
        &self,
        kind: OpenGpuiActivityKind,
    ) -> Result<OpenGpuiActivityLease, OpenGpuiActivityAcquireError> {
        self.acquire_inner(ActivityEntryKind::Activity(kind), None)
    }

    pub(crate) fn acquire_with_revocation(
        &self,
        kind: OpenGpuiActivityKind,
        revoke: impl FnOnce() + 'static,
    ) -> Result<OpenGpuiActivityLease, OpenGpuiActivityAcquireError> {
        self.acquire_inner(ActivityEntryKind::Activity(kind), Some(Box::new(revoke)))
    }

    fn acquire_pointer_capture(
        &self,
        hitbox: HitboxId,
    ) -> Result<OpenGpuiActivityLease, OpenGpuiActivityAcquireError> {
        self.acquire_inner(ActivityEntryKind::PointerCapture(hitbox), None)
    }

    pub(crate) fn active_count(&self) -> usize {
        self.state.borrow().entries.len()
    }

    pub(crate) fn has_activity(&self, kind: OpenGpuiActivityKind) -> bool {
        self.state
            .borrow()
            .entries
            .values()
            .any(|entry| entry.kind == ActivityEntryKind::Activity(kind))
    }

    pub(crate) fn protected_count(&self) -> usize {
        self.state
            .borrow()
            .entries
            .values()
            .filter(|entry| entry.kind.protects_retention())
            .count()
    }

    pub(crate) fn requires_interaction_island(&self) -> bool {
        self.state
            .borrow()
            .entries
            .values()
            .any(|entry| entry.kind.requires_interaction_island())
    }

    pub(crate) fn release_pointer_capture_if_owned(&self, window: &mut Window) -> bool {
        let Some(captured) = window.captured_hitbox() else {
            return false;
        };
        let owns_capture = self
            .state
            .borrow()
            .entries
            .values()
            .any(|entry| entry.kind.pointer_capture() == Some(captured));
        if owns_capture {
            window.release_pointer();
        }
        owns_capture
    }

    pub(crate) fn is_active(&self, id: u64) -> bool {
        self.state.borrow().entries.contains_key(&id)
    }

    pub(crate) fn is_revoked(&self) -> bool {
        self.state.borrow().revoked
    }

    /// Permanently closes the registry and revokes every currently active lease.
    ///
    /// Callbacks run in acquisition order and outside the registry's `RefCell`
    /// borrow, so they may safely query the registry or release other leases.
    pub(crate) fn revoke_all(&self) -> usize {
        let (entries, coordinated_instance) = {
            let mut state = self.state.borrow_mut();
            if state.revoked {
                return 0;
            }

            state.revoked = true;
            (
                mem::take(&mut state.entries),
                state.coordinated_instance.clone(),
            )
        };
        let revoked_count = entries.len();
        let protected_count = entries
            .values()
            .filter(|entry| entry.kind.protects_retention())
            .count();
        let window_owned_count = entries
            .values()
            .filter(|entry| entry.kind.requires_window_cleanup())
            .count();

        if let Some(coordinated_instance) = coordinated_instance {
            coordinated_instance.coordinator.release(
                &coordinated_instance.instance,
                protected_count,
                window_owned_count,
            );
        }

        let mut panics = OpenGpuiPanicAccumulator::default();
        for entry in entries.into_values() {
            if let Some(revoke) = entry.revoke {
                panics.capture(revoke);
            }
        }
        panics.resume();
        revoked_count
    }

    fn acquire_inner(
        &self,
        kind: ActivityEntryKind,
        revoke: Option<RevocationCallback>,
    ) -> Result<OpenGpuiActivityLease, OpenGpuiActivityAcquireError> {
        let mut revoke = revoke;
        let mut state = self.state.borrow_mut();
        if state.revoked {
            drop(state);
            invoke_failed_acquisition_callback(revoke);
            return Err(OpenGpuiActivityAcquireError::RegistryRevoked);
        }

        let Some(id) = state.next_id else {
            drop(state);
            invoke_failed_acquisition_callback(revoke);
            return Err(OpenGpuiActivityAcquireError::IdentityExhausted);
        };
        if kind.protects_retention()
            && let Some(coordinated_instance) = &state.coordinated_instance
            && let Err(error) = coordinated_instance
                .coordinator
                .acquire(&coordinated_instance.instance, kind)
        {
            drop(state);
            invoke_failed_acquisition_callback(revoke);
            return Err(error);
        }
        state.next_id = id.checked_add(1);
        state.entries.insert(
            id,
            ActivityEntry {
                kind,
                revoke: revoke.take(),
            },
        );
        drop(state);

        Ok(OpenGpuiActivityLease {
            id,
            state: Some(self.state.clone()),
        })
    }
}

impl fmt::Debug for OpenGpuiNodeActivityRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenGpuiNodeActivityRegistry")
            .field("active_count", &self.active_count())
            .field("protected_count", &self.protected_count())
            .field("revoked", &self.is_revoked())
            .finish()
    }
}

/// An owned activity lease that unregisters itself when dropped.
pub struct OpenGpuiActivityLease {
    id: u64,
    state: Option<Rc<RefCell<ActivityState>>>,
}

/// Exact pointer-capture ownership registered to one retained node mount.
#[must_use = "pointer capture must remain registered until it is released or the host retires it"]
pub struct OpenGpuiPointerCaptureLease {
    hitbox: HitboxId,
    activity: Option<OpenGpuiActivityLease>,
}

impl OpenGpuiPointerCaptureLease {
    #[must_use]
    pub fn hitbox(&self) -> HitboxId {
        self.hitbox
    }

    /// Transfers this still-owned capture to a replacement hitbox after native redraw.
    ///
    /// Open GPUI assigns a fresh hitbox identity during prepaint. Retained controls should call
    /// this method before registering listeners for the replacement hitbox. The transfer fails
    /// without changing either owner when the lease was revoked, its ledger entry no longer
    /// matches, or another control has replaced the window capture.
    #[must_use]
    pub fn rebind(&mut self, hitbox: HitboxId, window: &mut Window) -> bool {
        if window.captured_hitbox() != Some(self.hitbox) {
            return false;
        }
        let Some(activity) = self.activity.as_ref() else {
            return false;
        };
        let Some(state) = activity.state.as_ref() else {
            return false;
        };
        let mut state = state.borrow_mut();
        let Some(entry) = state.entries.get_mut(&activity.id) else {
            return false;
        };
        if entry.kind.pointer_capture() != Some(self.hitbox) {
            return false;
        }

        window.capture_pointer(hitbox);
        entry.kind = ActivityEntryKind::PointerCapture(hitbox);
        self.hitbox = hitbox;
        true
    }

    /// Releases this pointer capture when it is still current, then unregisters host ownership.
    pub fn release(mut self, window: &mut Window) -> bool {
        let released = if window.captured_hitbox() == Some(self.hitbox) {
            window.release_pointer();
            true
        } else {
            false
        };
        drop(self.activity.take());
        released
    }
}

impl fmt::Debug for OpenGpuiPointerCaptureLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenGpuiPointerCaptureLease")
            .field("hitbox", &self.hitbox)
            .field("active", &self.activity.is_some())
            .finish()
    }
}

impl OpenGpuiActivityLease {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl fmt::Debug for OpenGpuiActivityLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenGpuiActivityLease")
            .field("id", &self.id)
            .field("active", &self.state.is_some())
            .finish()
    }
}

impl Drop for OpenGpuiActivityLease {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };

        let (entry, coordinated_instance, window_owned) = {
            let mut state = state.borrow_mut();
            let entry = state.entries.remove(&self.id);
            let coordinated_instance = entry
                .as_ref()
                .filter(|entry| entry.kind.protects_retention())
                .and(state.coordinated_instance.clone());
            let window_owned = entry
                .as_ref()
                .is_some_and(|entry| entry.kind.requires_window_cleanup());
            (entry, coordinated_instance, window_owned)
        };
        if let Some(coordinated_instance) = coordinated_instance {
            coordinated_instance.coordinator.release(
                &coordinated_instance.instance,
                1,
                usize::from(window_owned),
            );
        }
        drop(entry);
    }
}

fn invoke_failed_acquisition_callback(revoke: Option<RevocationCallback>) {
    if let Some(revoke) = revoke {
        revoke();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn identity_exhaustion_is_reported_after_the_last_identity() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        registry.state.borrow_mut().next_id = Some(u64::MAX);

        let last = registry.acquire(OpenGpuiActivityKind::Task).unwrap();

        assert_eq!(last.id(), u64::MAX);
        assert_eq!(
            registry.acquire(OpenGpuiActivityKind::Task).unwrap_err(),
            OpenGpuiActivityAcquireError::IdentityExhausted,
        );
    }

    #[test]
    fn revocation_callback_can_reenter_registry() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        let callback_registry = registry.clone();
        let _lease = registry
            .acquire_with_revocation(OpenGpuiActivityKind::Focus, move || {
                assert!(callback_registry.is_revoked());
                assert_eq!(callback_registry.active_count(), 0);
            })
            .unwrap();

        assert_eq!(registry.revoke_all(), 1);
    }

    #[test]
    fn activity_leases_protect_retention_until_their_raii_guards_drop() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        let focus = registry.acquire(OpenGpuiActivityKind::Focus).unwrap();
        let task = registry.acquire(OpenGpuiActivityKind::Task).unwrap();
        let draft = registry.acquire(OpenGpuiActivityKind::Draft).unwrap();

        assert_eq!(registry.active_count(), 3);
        assert_eq!(registry.protected_count(), 2);
        assert!(registry.is_active(focus.id()));
        assert!(registry.is_active(task.id()));

        drop(focus);
        assert_eq!(registry.active_count(), 2);
        assert_eq!(registry.protected_count(), 1);
        drop(task);
        drop(draft);
        assert_eq!(registry.active_count(), 0);
        assert_eq!(registry.protected_count(), 0);
    }

    #[test]
    fn activity_kind_queries_follow_exact_lease_lifetimes() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        let handle = registry.renderer_handle();
        let ime = handle.acquire(OpenGpuiActivityKind::Ime).unwrap();
        let overlay = handle.acquire(OpenGpuiActivityKind::Overlay).unwrap();

        assert!(handle.has_activity(OpenGpuiActivityKind::Ime));
        assert!(handle.has_activity(OpenGpuiActivityKind::Overlay));
        assert!(!handle.has_activity(OpenGpuiActivityKind::Focus));

        drop(ime);
        assert!(!handle.has_activity(OpenGpuiActivityKind::Ime));
        assert!(handle.has_activity(OpenGpuiActivityKind::Overlay));
        drop(overlay);
        assert_eq!(handle.active_count(), 0);
    }

    #[test]
    fn hard_retirement_revokes_resources_once_in_acquisition_order() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        let revoked = Rc::new(RefCell::new(Vec::new()));
        let first_revoked = revoked.clone();
        let first = registry
            .acquire_with_revocation(OpenGpuiActivityKind::GraphInteraction, move || {
                first_revoked.borrow_mut().push("capture");
            })
            .unwrap();
        let second_revoked = revoked.clone();
        let second = registry
            .acquire_with_revocation(OpenGpuiActivityKind::Overlay, move || {
                second_revoked.borrow_mut().push("overlay");
            })
            .unwrap();

        assert_eq!(registry.revoke_all(), 2);
        assert_eq!(&*revoked.borrow(), &["capture", "overlay"]);
        assert!(registry.is_revoked());
        assert_eq!(registry.active_count(), 0);
        assert_eq!(registry.revoke_all(), 0);

        drop(first);
        drop(second);
        assert_eq!(&*revoked.borrow(), &["capture", "overlay"]);
        assert_eq!(
            registry.acquire(OpenGpuiActivityKind::Focus).unwrap_err(),
            OpenGpuiActivityAcquireError::RegistryRevoked,
        );
    }

    #[test]
    fn panicking_revocation_does_not_skip_later_resources() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        let revoked = Rc::new(RefCell::new(Vec::new()));
        let mut leases = Vec::new();
        for (label, should_panic) in [("first", false), ("second", true), ("third", false)] {
            let callback_revoked = revoked.clone();
            leases.push(
                registry
                    .acquire_with_revocation(OpenGpuiActivityKind::Task, move || {
                        callback_revoked.borrow_mut().push(label);
                        assert!(!should_panic, "revocation panic");
                    })
                    .unwrap(),
            );
        }

        let panic =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| registry.revoke_all()));

        assert!(panic.is_err());
        assert_eq!(&*revoked.borrow(), &["first", "second", "third"]);
        assert!(registry.is_revoked());
        assert_eq!(registry.active_count(), 0);
        drop(leases);
    }

    #[test]
    fn unprotected_tasks_and_subscriptions_do_not_count_as_retention_protection() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        let _task = registry.acquire(OpenGpuiActivityKind::Task).unwrap();
        let _subscription = registry
            .acquire(OpenGpuiActivityKind::Subscription)
            .unwrap();

        assert_eq!(registry.active_count(), 2);
        assert_eq!(registry.protected_count(), 0);
    }

    #[test]
    fn pointer_capture_requires_an_interaction_island_until_release() {
        let registry = OpenGpuiNodeActivityRegistry::detached();
        let capture = registry
            .acquire_pointer_capture(HitboxId::placeholder())
            .unwrap();

        assert!(registry.requires_interaction_island());
        drop(capture);
        assert!(!registry.requires_interaction_island());
    }
}
