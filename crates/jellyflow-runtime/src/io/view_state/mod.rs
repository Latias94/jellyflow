//! Versioned, application-owned state for initializing a view session.

use std::collections::HashSet;
use std::hash::Hash;

use jellyflow_core::core::{EdgeId, Graph};

mod seed;

pub use seed::{
    ViewSelectionSeed, ViewSessionSeed, ViewSessionSeedGraphMismatch, ViewStackingSeed,
    ViewportSeed,
};

pub(crate) fn edge_is_visible(graph: &Graph, edge: EdgeId) -> bool {
    let Some(edge) = graph.edge(&edge) else {
        return false;
    };
    if edge.hidden {
        return false;
    }
    let Some(source) = graph.port(&edge.from) else {
        return false;
    };
    let Some(target) = graph.port(&edge.to) else {
        return false;
    };
    graph.node(&source.node).is_some_and(|node| !node.hidden)
        && graph.node(&target.node).is_some_and(|node| !node.hidden)
}

pub(crate) fn retain_first<T>(values: &mut Vec<T>)
where
    T: Copy + Eq + Hash,
{
    let mut seen = HashSet::with_capacity(values.len());
    values.retain(|value| seen.insert(*value));
}
