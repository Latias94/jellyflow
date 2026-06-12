//! Renderer-neutral binding queries for knowledge-canvas adapters.

mod query;
mod resolve;

pub use query::{
    BindingEndpointResolution, BindingEndpointResolutionStatus, BindingQueryOptions,
    BindingQueryResult, ResolvedBinding, ResolvedBindingEndpoint,
};
pub use resolve::resolve_binding_query;

use crate::runtime::layout::LayoutContext;
use crate::runtime::store::NodeGraphStore;

impl NodeGraphStore {
    /// Reads renderer-neutral binding facts for the current store state.
    pub fn binding_query(&self) -> BindingQueryResult {
        self.binding_query_with_options(BindingQueryOptions::default())
    }

    /// Reads renderer-neutral binding facts with explicit geometry options.
    pub fn binding_query_with_options(&self, options: BindingQueryOptions) -> BindingQueryResult {
        crate::runtime::query::binding_query(self, options)
    }

    /// Builds layout context and pins nodes with resolvable binding geometry.
    pub fn layout_context_with_binding_pins(&self) -> LayoutContext {
        let pinned = self.binding_query().pinned_node_ids().collect::<Vec<_>>();
        self.layout_context().with_pinned_nodes(pinned)
    }
}
