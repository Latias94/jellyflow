use std::collections::BTreeMap;
use std::ops::Index;
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, EdgeId, NodeId, PortId};
use rpds::{RedBlackTreeMapSync, RedBlackTreeSetSync};

use crate::runtime::geometry::{EdgeRouteFacts, HandlePosition};
use crate::runtime::graph_runtime::{EdgeInstanceKey, NodeLifetimeKey};
use crate::runtime::view_session::ViewProvenance;

use super::RendererStamp;

#[derive(Debug, Clone)]
pub struct PublishedEdgeRoute {
    provenance: ViewProvenance,
    instance: EdgeInstanceKey,
    facts: EdgeRouteFacts,
    publication_incarnation: Arc<()>,
}

impl PublishedEdgeRoute {
    pub(crate) fn new(
        provenance: ViewProvenance,
        instance: EdgeInstanceKey,
        facts: EdgeRouteFacts,
    ) -> Option<Self> {
        (provenance.runtime() == instance.provenance()).then_some(Self {
            provenance,
            instance,
            facts,
            publication_incarnation: Arc::new(()),
        })
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(edge: EdgeId, facts: EdgeRouteFacts) -> Self {
        Self::new(
            ViewProvenance::unavailable(),
            EdgeInstanceKey::unavailable(edge),
            facts,
        )
        .expect("test view and edge provenance must match")
    }

    pub fn provenance(&self) -> ViewProvenance {
        self.provenance
    }

    pub fn edge(&self) -> EdgeId {
        self.instance.edge()
    }

    /// Returns the unforgeable runtime identity of this exact edge lifetime.
    pub fn instance(&self) -> EdgeInstanceKey {
        self.instance
    }

    pub fn facts(&self) -> &EdgeRouteFacts {
        &self.facts
    }

    pub(crate) fn with_selected(mut self, selected: bool) -> Self {
        self.facts.interaction.selected = selected;
        self
    }

    pub(crate) fn same_publication(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.publication_incarnation,
            &other.publication_incarnation,
        )
    }
}

impl PartialEq for PublishedEdgeRoute {
    fn eq(&self, other: &Self) -> bool {
        self.provenance == other.provenance
            && self.instance == other.instance
            && self.facts == other.facts
    }
}

/// Published canvas-space geometry for one real graph port.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedPortGeometry {
    port: PortId,
    node: NodeId,
    visual_bounds: CanvasRect,
    hit_bounds: Option<CanvasRect>,
    route_anchor: CanvasPoint,
    outward: HandlePosition,
}

impl PublishedPortGeometry {
    pub(crate) fn new(
        port: PortId,
        node: NodeId,
        visual_bounds: CanvasRect,
        hit_bounds: Option<CanvasRect>,
        route_anchor: CanvasPoint,
        outward: HandlePosition,
    ) -> Self {
        Self {
            port,
            node,
            visual_bounds,
            hit_bounds,
            route_anchor,
            outward,
        }
    }

    pub fn port(&self) -> PortId {
        self.port
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Bounds of the native element that visually represents the port.
    pub fn visual_bounds(&self) -> CanvasRect {
        self.visual_bounds
    }

    /// Effective visible bounds that may participate in connection probing.
    ///
    /// Route geometry remains available when this is `None`, allowing existing edges to stay
    /// attached while a port is fully clipped by native UI content.
    pub fn hit_bounds(&self) -> Option<CanvasRect> {
        self.hit_bounds
    }

    /// Exact point where the routed edge meets the port.
    pub fn route_anchor(&self) -> CanvasPoint {
        self.route_anchor
    }

    /// Cardinal direction in which the route leaves the anchor.
    pub fn outward(&self) -> HandlePosition {
        self.outward
    }

    #[cfg(test)]
    pub(crate) fn with_hit_bounds_for_test(mut self, hit_bounds: Option<CanvasRect>) -> Self {
        self.hit_bounds = hit_bounds;
        self
    }
}

/// Published layout availability for one real graph port.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishedPortOutcome {
    Rendered(PublishedPortGeometry),
    Hidden,
    Culled,
    Unavailable,
}

impl PublishedPortOutcome {
    pub fn rendered(&self) -> Option<&PublishedPortGeometry> {
        match self {
            Self::Rendered(geometry) => Some(geometry),
            Self::Hidden | Self::Culled | Self::Unavailable => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublishedNodeGeometry {
    lifetime: NodeLifetimeKey,
    renderer: RendererStamp,
    layout_generation: u64,
    fresh: bool,
    bounds: CanvasRect,
    ports: BTreeMap<PortId, PublishedPortGeometry>,
    port_outcomes: BTreeMap<PortId, PublishedPortOutcome>,
}

impl PublishedNodeGeometry {
    pub(crate) fn new(
        lifetime: NodeLifetimeKey,
        renderer: RendererStamp,
        layout_generation: u64,
        bounds: CanvasRect,
        port_outcomes: BTreeMap<PortId, PublishedPortOutcome>,
    ) -> Self {
        let ports = rendered_ports(&port_outcomes);
        Self {
            lifetime,
            renderer,
            layout_generation,
            fresh: true,
            bounds,
            ports,
            port_outcomes,
        }
    }

    pub fn node(&self) -> NodeId {
        self.lifetime.node()
    }

    pub fn lifetime(&self) -> NodeLifetimeKey {
        self.lifetime
    }

    pub fn renderer(&self) -> &RendererStamp {
        &self.renderer
    }

    pub fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    pub fn is_fresh(&self) -> bool {
        self.fresh
    }

    pub fn bounds(&self) -> CanvasRect {
        self.bounds
    }

    pub fn ports(&self) -> &BTreeMap<PortId, PublishedPortGeometry> {
        &self.ports
    }

    /// Complete outcome map for every real port covered by the latest publication.
    pub fn port_outcomes(&self) -> &BTreeMap<PortId, PublishedPortOutcome> {
        &self.port_outcomes
    }

    pub(crate) fn translate(&mut self, x: f32, y: f32) {
        self.bounds.origin.x += x;
        self.bounds.origin.y += y;
        for outcome in self.port_outcomes.values_mut() {
            if let PublishedPortOutcome::Rendered(port) = outcome {
                port.visual_bounds.origin.x += x;
                port.visual_bounds.origin.y += y;
                if let Some(bounds) = port.hit_bounds.as_mut() {
                    bounds.origin.x += x;
                    bounds.origin.y += y;
                }
                port.route_anchor.x += x;
                port.route_anchor.y += y;
            }
        }
        self.ports = rendered_ports(&self.port_outcomes);
    }

    pub(crate) fn mark_stale(&mut self, ports: impl IntoIterator<Item = PortId>) -> bool {
        let outcomes_changed =
            self.replace_port_outcomes(ports, || PublishedPortOutcome::Unavailable);
        let changed = self.fresh || outcomes_changed;
        self.fresh = false;
        changed
    }

    fn replace_port_outcomes(
        &mut self,
        ports: impl IntoIterator<Item = PortId>,
        outcome: impl Fn() -> PublishedPortOutcome,
    ) -> bool {
        let next = ports
            .into_iter()
            .map(|port| (port, outcome()))
            .collect::<BTreeMap<_, _>>();
        if self.port_outcomes == next {
            return false;
        }
        self.port_outcomes = next;
        self.ports = rendered_ports(&self.port_outcomes);
        true
    }

    pub(crate) fn approximately_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.lifetime == other.lifetime
            && self.renderer == other.renderer
            && self.layout_generation == other.layout_generation
            && self.fresh == other.fresh
            && rect_approximately_eq(self.bounds, other.bounds, epsilon)
            && self.port_outcomes.len() == other.port_outcomes.len()
            && self.port_outcomes.iter().all(|(port, current)| {
                other.port_outcomes.get(port).is_some_and(|candidate| {
                    port_outcomes_approximately_eq(current, candidate, epsilon)
                })
            })
    }
}

fn rendered_ports(
    outcomes: &BTreeMap<PortId, PublishedPortOutcome>,
) -> BTreeMap<PortId, PublishedPortGeometry> {
    outcomes
        .iter()
        .filter_map(|(port, outcome)| {
            outcome
                .rendered()
                .cloned()
                .map(|geometry| (*port, geometry))
        })
        .collect()
}

fn port_outcomes_approximately_eq(
    left: &PublishedPortOutcome,
    right: &PublishedPortOutcome,
    epsilon: f32,
) -> bool {
    match (left, right) {
        (PublishedPortOutcome::Hidden, PublishedPortOutcome::Hidden)
        | (PublishedPortOutcome::Culled, PublishedPortOutcome::Culled)
        | (PublishedPortOutcome::Unavailable, PublishedPortOutcome::Unavailable) => true,
        (PublishedPortOutcome::Rendered(left), PublishedPortOutcome::Rendered(right)) => {
            left.port == right.port
                && left.node == right.node
                && left.outward == right.outward
                && rect_approximately_eq(left.visual_bounds, right.visual_bounds, epsilon)
                && optional_rect_approximately_eq(left.hit_bounds, right.hit_bounds, epsilon)
                && point_approximately_eq(left.route_anchor, right.route_anchor, epsilon)
        }
        _ => false,
    }
}

fn optional_rect_approximately_eq(
    left: Option<CanvasRect>,
    right: Option<CanvasRect>,
    epsilon: f32,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => rect_approximately_eq(left, right, epsilon),
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

pub(crate) type SharedGeometryMap<K, V> = RedBlackTreeMapSync<K, Arc<V>>;
pub(crate) type SharedGeometryOwnerMap<K> = RedBlackTreeMapSync<K, NodeId>;
pub(crate) type SharedFrameCullSet = RedBlackTreeSetSync<NodeId>;
type SharedGeometryMapIter<'a, K, V> = <&'a SharedGeometryMap<K, V> as IntoIterator>::IntoIter;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GeometryVisibleCounts {
    pub nodes: usize,
    pub ports: usize,
    pub port_outcomes: usize,
}

#[derive(Debug)]
struct GeometryVisibility<'a, K: Ord> {
    owners: &'a SharedGeometryOwnerMap<K>,
    frame_culled_nodes: &'a SharedFrameCullSet,
}

impl<K: Ord> Copy for GeometryVisibility<'_, K> {}

impl<K: Ord> Clone for GeometryVisibility<'_, K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: Ord> GeometryVisibility<'_, K> {
    fn includes(self, key: &K) -> bool {
        self.owners
            .get(key)
            .is_some_and(|node| !self.frame_culled_nodes.contains(node))
    }
}

/// Read-only view over one published geometry collection.
///
/// The wrapper keeps the persistent storage and shared value ownership private while exposing
/// deterministic map-like reads to native adapters.
#[derive(Debug, Clone, Copy)]
pub struct GeometryElements<'a, K: Ord, V> {
    entries: &'a SharedGeometryMap<K, V>,
    visibility: Option<GeometryVisibility<'a, K>>,
    len: usize,
}

/// Iterator over published geometry ids and values.
pub struct GeometryElementIter<'a, K: Ord, V> {
    inner: SharedGeometryMapIter<'a, K, V>,
    visibility: Option<GeometryVisibility<'a, K>>,
    remaining: usize,
}

/// Iterator over published geometry ids.
pub struct GeometryElementKeys<'a, K: Ord, V> {
    inner: GeometryElementIter<'a, K, V>,
}

/// Iterator over published geometry values.
pub struct GeometryElementValues<'a, K: Ord, V> {
    inner: GeometryElementIter<'a, K, V>,
}

impl<'a, K: Ord, V> GeometryElements<'a, K, V> {
    pub(crate) fn new(entries: &'a SharedGeometryMap<K, V>) -> Self {
        Self {
            entries,
            visibility: None,
            len: entries.size(),
        }
    }

    pub(crate) fn visible(
        entries: &'a SharedGeometryMap<K, V>,
        owners: &'a SharedGeometryOwnerMap<K>,
        frame_culled_nodes: &'a SharedFrameCullSet,
        len: usize,
    ) -> Self {
        Self {
            entries,
            visibility: Some(GeometryVisibility {
                owners,
                frame_culled_nodes,
            }),
            len,
        }
    }

    fn includes(&self, key: &K) -> bool {
        self.visibility
            .is_none_or(|visibility| visibility.includes(key))
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.includes(key) && self.entries.contains_key(key)
    }

    pub fn get(&self, key: &K) -> Option<&'a V> {
        if !self.includes(key) {
            return None;
        }
        self.entries.get(key).map(Arc::as_ref)
    }

    pub fn iter(&self) -> GeometryElementIter<'a, K, V> {
        GeometryElementIter {
            inner: self.entries.into_iter(),
            visibility: self.visibility,
            remaining: self.len,
        }
    }

    pub fn keys(&self) -> GeometryElementKeys<'a, K, V> {
        GeometryElementKeys { inner: self.iter() }
    }

    pub fn values(&self) -> GeometryElementValues<'a, K, V> {
        GeometryElementValues { inner: self.iter() }
    }
}

impl<K: Ord, V> Index<&K> for GeometryElements<'_, K, V> {
    type Output = V;

    fn index(&self, key: &K) -> &Self::Output {
        self.get(key).expect("no published geometry for key")
    }
}

impl<'a, K: Ord, V> IntoIterator for GeometryElements<'a, K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = GeometryElementIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K: Ord, V> Iterator for GeometryElementIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        for (key, value) in self.inner.by_ref() {
            if self
                .visibility
                .is_some_and(|visibility| !visibility.includes(key))
            {
                continue;
            }
            self.remaining = self.remaining.saturating_sub(1);
            return Some((key, value.as_ref()));
        }
        debug_assert_eq!(self.remaining, 0);
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<K: Ord, V> DoubleEndedIterator for GeometryElementIter<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        while let Some((key, value)) = self.inner.next_back() {
            if self
                .visibility
                .is_some_and(|visibility| !visibility.includes(key))
            {
                continue;
            }
            self.remaining = self.remaining.saturating_sub(1);
            return Some((key, value.as_ref()));
        }
        debug_assert_eq!(self.remaining, 0);
        None
    }
}

impl<K: Ord, V> ExactSizeIterator for GeometryElementIter<'_, K, V> {}

impl<'a, K: Ord, V> Iterator for GeometryElementKeys<'a, K, V> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(key, _)| key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K: Ord, V> DoubleEndedIterator for GeometryElementKeys<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|(key, _)| key)
    }
}

impl<K: Ord, V> ExactSizeIterator for GeometryElementKeys<'_, K, V> {}

impl<'a, K: Ord, V> Iterator for GeometryElementValues<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, value)| value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K: Ord, V> DoubleEndedIterator for GeometryElementValues<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|(_, value)| value)
    }
}

impl<K: Ord, V> ExactSizeIterator for GeometryElementValues<'_, K, V> {}

#[derive(Debug)]
struct GeometrySnapshotInner {
    provenance: ViewProvenance,
    layout_revision: u64,
    nodes: SharedGeometryMap<NodeId, PublishedNodeGeometry>,
    node_owners: SharedGeometryOwnerMap<NodeId>,
    ports: SharedGeometryMap<PortId, PublishedPortGeometry>,
    port_outcomes: SharedGeometryMap<PortId, PublishedPortOutcome>,
    port_owners: SharedGeometryOwnerMap<PortId>,
    frame_culled_nodes: SharedFrameCullSet,
    visible_counts: GeometryVisibleCounts,
    routes: SharedGeometryMap<EdgeId, PublishedEdgeRoute>,
}

/// Immutable geometry visible to one published layout revision.
///
/// Whole-node frame culling is represented as snapshot visibility, not as synthetic per-port
/// [`PublishedPortOutcome::Culled`] values. A culled node and all of its ports are absent from
/// point and collection queries in snapshots created after that frame. Previously acquired
/// snapshots retain their original visibility.
#[derive(Debug, Clone)]
pub struct GeometrySnapshot {
    inner: Arc<GeometrySnapshotInner>,
}

fn rect_approximately_eq(left: CanvasRect, right: CanvasRect, epsilon: f32) -> bool {
    (left.origin.x - right.origin.x).abs() <= epsilon
        && (left.origin.y - right.origin.y).abs() <= epsilon
        && (left.size.width - right.size.width).abs() <= epsilon
        && (left.size.height - right.size.height).abs() <= epsilon
}

fn point_approximately_eq(left: CanvasPoint, right: CanvasPoint, epsilon: f32) -> bool {
    (left.x - right.x).abs() <= epsilon && (left.y - right.y).abs() <= epsilon
}

impl GeometrySnapshot {
    pub(crate) fn new(
        provenance: ViewProvenance,
        layout_revision: u64,
        nodes: SharedGeometryMap<NodeId, PublishedNodeGeometry>,
        node_owners: SharedGeometryOwnerMap<NodeId>,
        ports: SharedGeometryMap<PortId, PublishedPortGeometry>,
        port_outcomes: SharedGeometryMap<PortId, PublishedPortOutcome>,
        port_owners: SharedGeometryOwnerMap<PortId>,
        frame_culled_nodes: SharedFrameCullSet,
        visible_counts: GeometryVisibleCounts,
        routes: SharedGeometryMap<EdgeId, PublishedEdgeRoute>,
    ) -> Self {
        Self {
            inner: Arc::new(GeometrySnapshotInner {
                provenance,
                layout_revision,
                nodes,
                node_owners,
                ports,
                port_outcomes,
                port_owners,
                frame_culled_nodes,
                visible_counts,
                routes,
            }),
        }
    }

    pub fn provenance(&self) -> ViewProvenance {
        self.inner.provenance
    }

    pub fn layout_revision(&self) -> u64 {
        self.inner.layout_revision
    }

    pub fn node(&self, node: NodeId) -> Option<&PublishedNodeGeometry> {
        if self.inner.frame_culled_nodes.contains(&node) {
            return None;
        }
        self.inner.nodes.get(&node).map(Arc::as_ref)
    }

    pub fn port(&self, port: PortId) -> Option<&PublishedPortGeometry> {
        let is_visible = self
            .inner
            .port_owners
            .get(&port)
            .is_some_and(|node| !self.inner.frame_culled_nodes.contains(node));
        if !is_visible {
            return None;
        }
        self.inner.ports.get(&port).map(Arc::as_ref)
    }

    pub fn nodes(&self) -> GeometryElements<'_, NodeId, PublishedNodeGeometry> {
        GeometryElements::visible(
            &self.inner.nodes,
            &self.inner.node_owners,
            &self.inner.frame_culled_nodes,
            self.inner.visible_counts.nodes,
        )
    }

    pub fn ports(&self) -> GeometryElements<'_, PortId, PublishedPortGeometry> {
        GeometryElements::visible(
            &self.inner.ports,
            &self.inner.port_owners,
            &self.inner.frame_culled_nodes,
            self.inner.visible_counts.ports,
        )
    }

    pub fn port_outcome(&self, port: PortId) -> Option<&PublishedPortOutcome> {
        let is_visible = self
            .inner
            .port_owners
            .get(&port)
            .is_some_and(|node| !self.inner.frame_culled_nodes.contains(node));
        if !is_visible {
            return None;
        }
        self.inner.port_outcomes.get(&port).map(Arc::as_ref)
    }

    pub fn port_outcomes(&self) -> GeometryElements<'_, PortId, PublishedPortOutcome> {
        GeometryElements::visible(
            &self.inner.port_outcomes,
            &self.inner.port_owners,
            &self.inner.frame_culled_nodes,
            self.inner.visible_counts.port_outcomes,
        )
    }

    pub fn route(&self, edge: EdgeId) -> Option<&PublishedEdgeRoute> {
        self.inner.routes.get(&edge).map(Arc::as_ref)
    }

    pub fn routes(&self) -> GeometryElements<'_, EdgeId, PublishedEdgeRoute> {
        GeometryElements::new(&self.inner.routes)
    }
}
