mod resolve;
mod types;

pub use resolve::{edge_position, handle_anchor_position, handle_center_position};
pub use types::{
    EdgeEndpointInput, EdgeEndpointPosition, EdgePosition, HandleBounds, HandlePosition,
};

#[cfg(test)]
mod tests;
