//! Persisted and in-memory editor view state.

mod pure;
mod state;

pub use pure::NodeGraphPureViewState;
pub use state::NodeGraphViewState;

fn default_zoom() -> f32 {
    1.0
}
