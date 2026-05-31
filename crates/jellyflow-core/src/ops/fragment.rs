//! Deterministic graph fragments for clipboard, duplication, and merges.
//!
//! A fragment is a self-contained subset of a graph that can be serialized and pasted into another
//! graph by remapping IDs.

mod clipboard;
mod collect;
mod model;
mod paste;
mod remap;

pub use model::GraphFragment;
pub use paste::PasteTuning;
pub use remap::{IdRemapSeed, IdRemapper};
