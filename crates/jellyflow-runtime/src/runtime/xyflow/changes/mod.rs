//! XyFlow-style change model for editor runtimes.
//!
//! In XyFlow/ReactFlow, internal interactions produce "changes" that user code can apply to its
//! node/edge arrays via helpers like `applyNodeChanges`. In Jellyflow, the authoritative model
//! is a reversible `GraphTransaction` (undo/redo friendly). This module bridges the two worlds:
//! - Map `GraphTransaction` -> `(NodeChange, EdgeChange)` events (for callbacks).
//! - Map `(NodeChange, EdgeChange)` -> reversible `GraphTransaction` (for store dispatch).
//!
//! These change names are compatibility vocabulary. Use [`crate::runtime::policy`] when an adapter
//! needs effective interaction policy such as whether a node can be selected or an edge endpoint can
//! be reconnected.

pub use crate::runtime::commit::NodeGraphPatch;

mod edge;
mod model;
mod node;

pub use edge::EdgeChange;
pub use model::{ChangesToTransactionError, NodeGraphChanges};
pub use node::NodeChange;
