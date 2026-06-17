//! Immediate-mode egui adapter for Jellyflow.
//!
//! This crate keeps rendering, windowing, and pointer capture in egui while delegating graph
//! semantics, layout, and mutation rules to the headless Jellyflow crates.

#![deny(unsafe_code)]

pub use eframe::egui;

pub mod app;
pub mod bridge;
pub mod input;
pub mod samples;
pub mod state;
pub mod ui;

pub use app::JellyflowEguiApp;
pub use bridge::{JellyflowEguiBridge, NodeRendererStyle, RendererCatalog};
pub use samples::{SampleGraphError, SampleGraphKind};
pub use state::{
    ActiveCanvasInteraction, CanvasSnapshot, CanvasTool, HoverTarget, InspectorState,
    JellyflowEguiState, LayoutPresetChoice,
};

#[cfg(test)]
mod tests {
    use super::{
        ActiveCanvasInteraction, JellyflowEguiApp, JellyflowEguiBridge, NodeRendererStyle,
        RendererCatalog, SampleGraphKind,
    };
    use jellyflow::core::{CanvasPoint, CanvasSize, GraphOp, GraphTransaction, PortDirection};
    use jellyflow::runtime::runtime::drag::NodeNudgeDirection;

    #[test]
    fn demo_app_builds_without_windowing() {
        let app = JellyflowEguiApp::demo().expect("demo app builds");

        assert!(!app.bridge.store().graph().nodes().is_empty());
        assert!(app.bridge.descriptors().len() >= 4);
    }

    #[test]
    fn all_sample_graphs_build_with_nodes_edges_and_default_layouts() {
        for sample in SampleGraphKind::ALL {
            let app = JellyflowEguiApp::sample(sample).expect("sample app builds");

            assert_eq!(app.state.selected_sample, sample);
            assert_eq!(app.state.selected_layout_preset, sample.default_layout());
            assert!(
                app.bridge.store().graph().nodes().len() >= 4,
                "{sample:?} should contain multiple nodes"
            );
            assert!(
                !app.bridge.store().graph().edges().is_empty(),
                "{sample:?} should contain edges"
            );
        }
    }

    #[test]
    fn renderer_catalog_uses_registered_styles_and_fallback() {
        let mut catalog = RendererCatalog::new();
        catalog.register("custom-card", NodeRendererStyle::task());

        assert_eq!(
            catalog.style_for_key("custom-card"),
            NodeRendererStyle::task()
        );
        assert_eq!(
            catalog.style_for_key("unknown"),
            NodeRendererStyle::fallback()
        );

        let bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let descriptor = bridge
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.renderer_key == "task-card")
            .expect("demo registry exposes task renderer");
        assert_eq!(
            RendererCatalog::default().style_for_descriptor(&descriptor),
            NodeRendererStyle::task()
        );
        let topic = bridge
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.renderer_key == "topic-card")
            .expect("demo registry exposes topic renderer");
        assert_eq!(
            RendererCatalog::default().style_for_descriptor(&topic),
            NodeRendererStyle::topic()
        );
    }

    #[test]
    fn default_handle_bounds_follow_explicit_node_size() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let node = bridge
            .store()
            .graph()
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.task").then_some(*node))
            .expect("demo task node exists");
        let from_size = bridge.store().graph().nodes()[&node].size;
        let to_size = CanvasSize {
            width: 260.0,
            height: 120.0,
        };
        bridge
            .store_mut()
            .dispatch_transaction(&GraphTransaction::from_ops([GraphOp::SetNodeSize {
                id: node,
                from: from_size,
                to: Some(to_size),
            }]))
            .expect("size update dispatches");

        let output = bridge
            .default_handle_bounds(node)
            .into_iter()
            .find(|(handle, _)| handle.direction == PortDirection::Out)
            .expect("task output handle exists");

        assert_eq!(output.1.rect.origin.x, to_size.width - 5.0);
        assert_eq!(output.1.rect.origin.y, to_size.height * 0.5 - 5.0);
    }

    #[test]
    fn start_node_drag_preserves_existing_multi_selection_for_drag_plan() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let mut nodes = bridge
            .store()
            .graph()
            .nodes()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        nodes.sort();
        let primary = nodes[0];
        let secondary = nodes[1];

        bridge
            .store_mut()
            .set_selection(vec![primary, secondary], Vec::new(), Vec::new());
        bridge.start_node_drag(primary, false);

        assert_eq!(
            bridge.store().view_state().selected_nodes,
            vec![primary, secondary]
        );

        let plan = bridge
            .plan_node_drag(primary, CanvasPoint { x: 32.0, y: 18.0 })
            .expect("selected nodes produce a drag plan");
        let planned_nodes = plan
            .items()
            .iter()
            .map(|item| item.node)
            .collect::<Vec<_>>();
        assert!(planned_nodes.contains(&primary));
        assert!(planned_nodes.contains(&secondary));
    }

    #[test]
    fn bridge_nudge_selection_commits_keyboard_move() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let node = *bridge
            .store()
            .graph()
            .nodes()
            .keys()
            .next()
            .expect("demo node exists");
        let before = bridge.store().graph().nodes()[&node].pos;
        bridge.select_node(node, false);

        let outcome = bridge
            .nudge_selection(NodeNudgeDirection::Right, false)
            .expect("nudge dispatch succeeds")
            .expect("nudge commits");

        assert!(!outcome.dispatch().committed().ops().is_empty());
        assert!(bridge.store().graph().nodes()[&node].pos.x > before.x);
        assert_eq!(bridge.store().graph().nodes()[&node].pos.y, before.y);
    }

    #[test]
    fn bridge_nudge_selection_is_noop_without_selection() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        bridge.clear_selection();

        let outcome = bridge
            .nudge_selection(NodeNudgeDirection::Right, false)
            .expect("nudge no-selection path succeeds");

        assert!(outcome.is_none());
    }

    #[test]
    fn bridge_pan_interaction_commit_does_not_apply_delta_twice() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        bridge.pan_by_screen_delta(CanvasPoint { x: 40.0, y: -20.0 });
        let after_live_pan = bridge.store().view_state().pan;

        bridge
            .commit_interaction(ActiveCanvasInteraction::Pan {
                current_pointer: CanvasPoint { x: 40.0, y: -20.0 },
            })
            .expect("pan commit ends interaction");

        assert_eq!(bridge.store().view_state().pan, after_live_pan);
    }
}
