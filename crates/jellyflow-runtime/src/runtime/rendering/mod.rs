//! Renderer-neutral ordering and visibility reads.
//!
//! Adapters still own painting, widgets, and GPU/UI details. The public adapter seam is
//! `NodeGraphStore`'s store-level rendering read methods, especially
//! `NodeGraphStore::rendering_query`. The resolver functions stay inside the crate so the runtime
//! can change lookup/cache/index implementations without making adapters depend on that shape.

pub(crate) mod order;
pub(crate) mod query;
mod store;
pub(crate) mod visibility;

pub use query::RenderingQueryResult;
