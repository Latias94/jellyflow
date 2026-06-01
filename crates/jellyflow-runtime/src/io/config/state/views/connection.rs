use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::io::tuning::NodeGraphAutoPanTuning;
use crate::runtime::geometry::EdgeHitTestOptions;

use super::super::NodeGraphInteractionState;

/// Connection gesture and edge interaction settings resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphConnectionInteraction<'a> {
    pub nodes_connectable: bool,
    pub edges_reconnectable: bool,
    pub connection_mode: NodeGraphConnectionMode,
    pub connection_radius: f32,
    pub reconnect_radius: f32,
    pub reconnect_on_drop_empty: bool,
    pub connection_drag_threshold: f32,
    pub connect_on_click: bool,
    pub reroute_on_edge_double_click: bool,
    pub edge_insert_on_alt_drag: bool,
    pub edge_hit_test: EdgeHitTestOptions,
    pub auto_pan: &'a NodeGraphAutoPanTuning,
}

impl NodeGraphInteractionState {
    pub fn connection_interaction(&self) -> NodeGraphConnectionInteraction<'_> {
        NodeGraphConnectionInteraction {
            nodes_connectable: self.nodes_connectable,
            edges_reconnectable: self.edges_reconnectable,
            connection_mode: self.connection_mode,
            connection_radius: self.connection_radius,
            reconnect_radius: self.reconnect_radius,
            reconnect_on_drop_empty: self.reconnect_on_drop_empty,
            connection_drag_threshold: self.connection_drag_threshold,
            connect_on_click: self.connect_on_click,
            reroute_on_edge_double_click: self.reroute_on_edge_double_click,
            edge_insert_on_alt_drag: self.edge_insert_on_alt_drag,
            edge_hit_test: self.edge_hit_test_options(),
            auto_pan: &self.auto_pan,
        }
    }
}
