use crate::runtime::adapter_protocol::ViewPoint;
use crate::runtime::view_session::ViewProvenance;

use super::{
    ViewportPanRequest, ViewportTransform, ViewportZoomRequest, pan_viewport, zoom_viewport,
};

/// Opaque revision of one view generation's live viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewportRevision(u64);

impl ViewportRevision {
    pub(crate) const fn initial() -> Self {
        Self(0)
    }

    pub(crate) const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn exhausted_for_test() -> Self {
        Self(u64::MAX)
    }
}

/// Exact viewport state issued by one live view generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSnapshot {
    provenance: ViewProvenance,
    revision: ViewportRevision,
    transform: ViewportTransform,
}

impl ViewportSnapshot {
    pub(crate) const fn new(
        provenance: ViewProvenance,
        revision: ViewportRevision,
        transform: ViewportTransform,
    ) -> Self {
        Self {
            provenance,
            revision,
            transform,
        }
    }

    pub const fn provenance(self) -> ViewProvenance {
        self.provenance
    }

    pub const fn revision(self) -> ViewportRevision {
        self.revision
    }

    pub const fn transform(self) -> ViewportTransform {
        self.transform
    }
}

/// A raw native pointer point bound to the viewport that sampled it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportPointerSample {
    snapshot: ViewportSnapshot,
    raw: ViewPoint,
}

impl ViewportPointerSample {
    pub(crate) fn new(snapshot: ViewportSnapshot, raw: ViewPoint) -> Option<Self> {
        raw.is_finite().then_some(Self { snapshot, raw })
    }

    pub const fn snapshot(self) -> ViewportSnapshot {
        self.snapshot
    }

    pub const fn raw(self) -> ViewPoint {
        self.raw
    }
}

/// Viewport change resolved atomically against the current live transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportIntent {
    Pan(ViewportPanRequest),
    Zoom(ViewportZoomRequest),
}

impl ViewportIntent {
    pub const fn pan(request: ViewportPanRequest) -> Self {
        Self::Pan(request)
    }

    pub const fn zoom(request: ViewportZoomRequest) -> Self {
        Self::Zoom(request)
    }

    pub(crate) fn resolve(self, current: ViewportTransform) -> Option<ViewportTransform> {
        match self {
            Self::Pan(request) => pan_viewport(current, request),
            Self::Zoom(request) => zoom_viewport(current, request),
        }
    }
}
