//! Immediate-mode egui adapter for Jellyflow.
//!
//! This crate keeps rendering, windowing, and pointer capture in egui while delegating graph
//! semantics, layout, and mutation rules to the headless Jellyflow crates.

#![deny(unsafe_code)]

pub use eframe::egui;

pub mod app;
pub mod bridge;
mod handle_layout;
pub mod input;
pub mod renderer;
pub mod samples;
pub mod state;
pub mod ui;

pub use app::JellyflowEguiApp;
pub use bridge::JellyflowEguiBridge;
pub use renderer::{
    EguiNodeControlEdit, EguiNodeRepeatableAction, EguiNodeWidgetRenderOutcome,
    EguiNodeWidgetRenderer, EguiRepeatableMoveDirection, FieldListNodeRenderer, NodeContentLevel,
    NodeInteractiveRegion, NodeRenderInput, NodeRenderLayout, NodeRendererState, NodeRendererStyle,
    NodeWidgetRenderInput, RendererCatalog, RichNodeRenderer,
};
pub use samples::{SampleGraphError, SampleGraphKind};
pub use state::{
    ActiveCanvasInteraction, CanvasSnapshot, CanvasTool, HoverTarget, InspectorState,
    JellyflowEguiState, LayoutPresetChoice,
};

#[cfg(test)]
mod tests {
    use super::{
        ActiveCanvasInteraction, CanvasSnapshot, EguiNodeControlEdit, EguiNodeRepeatableAction,
        EguiNodeWidgetRenderOutcome, EguiRepeatableMoveDirection, JellyflowEguiApp,
        JellyflowEguiBridge, NodeContentLevel, NodeRendererStyle, RendererCatalog, SampleGraphKind,
    };
    use eframe::egui::{Pos2, Rect, Vec2};
    use jellyflow::core::{
        CanvasPoint, CanvasRect, CanvasSize, DefaultTypeCompatibility, GraphOp, GraphTransaction,
        NodeId, PortDirection, PortKind, TypeCompatibility, TypeDesc,
    };
    use jellyflow::runtime::runtime::connection::{
        ConnectEdgeRequest, ConnectionHandleRef, ConnectionHandleValidity,
    };
    use jellyflow::runtime::runtime::drag::NodeNudgeDirection;
    use jellyflow::runtime::runtime::geometry::HandlePosition;
    use jellyflow::runtime::runtime::measurement::{
        NodeHandleMeasurementSource, NodeInternalsInvalidationReason, NodeMeasurementStatus,
    };
    use jellyflow::runtime::runtime::resize::NodeResizeDirection;
    use jellyflow::runtime::schema::NodeControlBinding;

    #[test]
    fn demo_app_builds_without_windowing() {
        let app = JellyflowEguiApp::demo().expect("demo app builds");

        assert!(!app.bridge.store().graph().nodes().is_empty());
        assert!(app.bridge.descriptors().len() >= 4);
        assert!(
            app.state.canvas.fit_view_requested,
            "new demo apps should fit once after the real canvas size is known"
        );
    }

    #[test]
    fn all_sample_graphs_build_with_nodes_edges_and_default_layouts() {
        for sample in SampleGraphKind::ALL {
            let app = JellyflowEguiApp::sample(sample).expect("sample app builds");

            assert_eq!(app.state.selected_sample, sample);
            assert_eq!(app.state.selected_layout_preset, sample.default_layout());
            assert!(app.state.status_message.is_none());
            assert!(
                app.bridge.store().graph().nodes().len() >= 3,
                "{sample:?} should contain multiple nodes"
            );
            assert!(
                !app.bridge.store().graph().edges().is_empty(),
                "{sample:?} should contain edges"
            );
        }
    }

    #[test]
    fn product_samples_reuse_edge_metadata_and_port_descriptors() {
        let app =
            JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder).expect("sample builds");

        assert!(app.bridge.store().graph().edges().values().any(|edge| {
            edge.view.label.as_deref() == Some("error")
                && edge.view.renderer_key.as_deref() == Some("sample-edge")
        }));
        assert!(app.bridge.store().graph().ports().values().any(|port| {
            port.kind == PortKind::Exec && port.dir == PortDirection::Out && port.key.0 == "yes"
        }));

        let erd = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let table_descriptor = erd
            .bridge
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.kind.0 == "demo.table")
            .expect("table descriptor exists");
        assert!(
            table_descriptor
                .ports
                .iter()
                .any(|port| port.view.anchor.as_deref() == Some("field.primary_key"))
        );
        assert!(erd.bridge.store().graph().edges().values().any(|edge| {
            edge.view.label.as_deref() == Some("1:N")
                && edge.data.get("label").and_then(|value| value.as_str()) == Some("1:N")
        }));

        let shader =
            JellyflowEguiApp::sample(SampleGraphKind::ShaderGraph).expect("shader sample builds");
        assert!(shader.bridge.store().graph().nodes().values().any(|node| {
            node.kind.0 == "demo.shader.mix"
                && node
                    .data
                    .get("config")
                    .and_then(|value| value.get("factor"))
                    .is_some()
        }));
        assert!(shader.bridge.store().graph().ports().values().any(|port| {
            port.kind == PortKind::Data && port.key.0 == "color" && port.dir == PortDirection::Out
        }));
        assert!(shader.bridge.store().graph().ports().values().any(|port| {
            port.key.0 == "factor" && port.ty.as_ref().is_some_and(|ty| *ty == TypeDesc::Float)
        }));
        assert!(shader.bridge.store().graph().ports().values().any(|port| {
            port.key.0 == "color" && port.ty.as_ref().is_some_and(|ty| *ty == shader_vec4())
        }));
        let shader_descriptor = shader
            .bridge
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.renderer_key == "shader-card")
            .expect("shader descriptor exists");
        assert!(
            shader_descriptor
                .surface_slots
                .iter()
                .any(|slot| slot.anchor.as_deref() == Some("rail.inputs"))
        );
    }

    #[test]
    fn shader_sample_rejects_incompatible_typed_connections_through_default_store_path() {
        let mut shader =
            JellyflowEguiApp::sample(SampleGraphKind::ShaderGraph).expect("shader sample builds");
        let graph = shader.bridge.store().graph();
        let color = graph
            .ports()
            .iter()
            .find_map(|(id, port)| {
                (port.key.0 == "color" && port.dir == PortDirection::Out).then_some(*id)
            })
            .expect("shader color output exists");
        let factor = graph
            .ports()
            .iter()
            .find_map(|(id, port)| {
                (port.key.0 == "factor" && port.dir == PortDirection::In).then_some(*id)
            })
            .expect("shader factor input exists");
        let mode = shader
            .bridge
            .store()
            .resolved_interaction_state()
            .connection_mode;

        let err = shader
            .bridge
            .store_mut()
            .apply_connect_edge(ConnectEdgeRequest::new(color, factor, mode))
            .expect_err("vec4 color output must not connect to float factor input");

        let diagnostics = err.diagnostics().expect("typed rejection diagnostics");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.key == "connect.type_mismatch")
        );
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
    fn handle_bounds_follow_port_view_descriptor_sides() {
        let bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let decision = bridge
            .store()
            .graph()
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.decision").then_some(*node))
            .expect("decision node exists");
        let handles = bridge.default_handle_bounds(decision);

        assert!(handles.iter().any(|(handle, bounds)| {
            handle.direction == PortDirection::In && bounds.position == HandlePosition::Left
        }));
        assert!(handles.iter().any(|(handle, bounds)| {
            bridge.store().graph().ports()[&handle.port].key.0 == "yes"
                && bounds.position == HandlePosition::Top
        }));
        assert!(handles.iter().any(|(handle, bounds)| {
            bridge.store().graph().ports()[&handle.port].key.0 == "no"
                && bounds.position == HandlePosition::Bottom
        }));
    }

    #[test]
    fn erd_snapshot_places_table_handles_on_field_anchor_regions() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let graph = app.bridge.store().graph();
        let table = graph
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.table").then_some(*node))
            .expect("table node exists");
        let mut pk = None;
        let mut fk = None;
        for port in &graph.nodes()[&table].ports {
            let record = &graph.ports()[port];
            let handle = ConnectionHandleRef::new(table, *port, record.dir);
            let bounds = snapshot
                .handle_bounds
                .get(&handle)
                .copied()
                .expect("handle bounds exist");
            match record.key.0.as_str() {
                "pk" => pk = Some(bounds),
                "fk" => fk = Some(bounds),
                _ => {}
            }
        }
        let pk = pk.expect("primary key handle exists");
        let fk = fk.expect("foreign key handle exists");

        assert_eq!(pk.position, HandlePosition::Right);
        assert_eq!(fk.position, HandlePosition::Left);
        assert!(
            pk.rect.origin.y < fk.rect.origin.y,
            "pk should align to the first table field and fk to a later field"
        );
    }

    #[test]
    fn erd_snapshot_reports_semantic_region_measurements_to_runtime() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let first = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let graph = app.bridge.store().graph();
        let table = graph
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.table").then_some(*node))
            .expect("table node exists");
        assert!(matches!(
            first.layout_facts.node_measurement_status(table),
            NodeMeasurementStatus::Fresh { .. }
        ));
        let pk_port = graph.nodes()[&table]
            .ports
            .iter()
            .copied()
            .find(|port| graph.ports()[port].key.0 == "pk")
            .expect("pk port exists");

        let measurement = app
            .bridge
            .store()
            .node_measurement(table)
            .expect("runtime measurement reported");
        assert!(
            measurement
                .slots
                .iter()
                .any(|slot| slot.key == "field.primary_key")
        );
        assert!(
            measurement
                .anchors
                .iter()
                .any(|anchor| anchor.anchor == "field.primary_key"
                    && anchor.port_key.as_ref().is_some_and(|key| key.0 == "pk"))
        );

        let source = app
            .bridge
            .store()
            .resolve_node_handle_measurement(ConnectionHandleRef::new(
                table,
                pk_port,
                graph.ports()[&pk_port].dir,
            ))
            .source;
        assert!(matches!(
            source,
            NodeHandleMeasurementSource::MeasuredHandle
                | NodeHandleMeasurementSource::MeasuredAnchor { .. }
        ));
    }

    #[test]
    fn shader_graph_typed_ports_reject_incompatible_hover_and_commit() {
        let mut app =
            JellyflowEguiApp::sample(SampleGraphKind::ShaderGraph).expect("shader sample builds");
        let _snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let graph = app.bridge.store().graph();
        let texture = graph
            .nodes()
            .iter()
            .find_map(|(node, record)| {
                (record.kind.0 == "demo.shader.texture_sample").then_some(*node)
            })
            .expect("texture node exists");
        let mix = graph
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.shader.mix").then_some(*node))
            .expect("mix node exists");
        let color = graph.nodes()[&texture]
            .ports
            .iter()
            .copied()
            .find(|port| {
                graph.ports()[port].key.0 == "color"
                    && graph.ports()[port].dir == PortDirection::Out
            })
            .expect("texture color port exists");
        let factor = graph.nodes()[&mix]
            .ports
            .iter()
            .copied()
            .find(|port| graph.ports()[port].key.0 == "factor")
            .expect("factor port exists");
        let color_handle = ConnectionHandleRef::new(texture, color, graph.ports()[&color].dir);
        let factor_handle = ConnectionHandleRef::new(mix, factor, graph.ports()[&factor].dir);
        let factor_bounds = app
            .bridge
            .store()
            .resolve_node_handle_measurement(factor_handle)
            .bounds
            .expect("factor bounds");
        let mix_pos = graph.nodes()[&mix].pos;
        let pointer = CanvasPoint {
            x: mix_pos.x + factor_bounds.rect.origin.x + factor_bounds.rect.size.width * 0.5,
            y: mix_pos.y + factor_bounds.rect.origin.y + factor_bounds.rect.size.height * 0.5,
        };

        let hover = app.bridge.resolve_connection_target(pointer, color_handle);

        assert_eq!(hover.feedback, ConnectionHandleValidity::Invalid);
        assert!(!hover.is_handle_valid);
        assert!(
            app.bridge
                .connect_handles(color_handle, factor_handle)
                .expect_err("float factor must not connect to vec4 albedo")
                .contains("type mismatch")
        );

        let mut compat = DefaultTypeCompatibility;
        assert!(
            !compat
                .compatible(&TypeDesc::Float, &shader_vec4())
                .is_compatible()
        );
    }

    #[test]
    fn snapshot_caches_node_render_layouts_for_visible_nodes() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );

        assert!(
            snapshot
                .visible_node_ids
                .iter()
                .all(|node| snapshot.node_render_layouts.contains_key(node))
        );
        assert!(snapshot.node_render_layouts.values().any(|layout| {
            layout
                .interactive_regions
                .iter()
                .any(|region| region.key == "field.primary_key")
        }));
    }

    #[test]
    fn table_resize_clamps_to_renderer_minimum_size() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let graph = app.bridge.store().graph();
        let table = graph
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.table").then_some(*node))
            .expect("table node exists");
        let from_size = graph.nodes()[&table].size;
        app.bridge
            .store_mut()
            .dispatch_transaction(&GraphTransaction::from_ops([GraphOp::SetNodeSize {
                id: table,
                from: from_size,
                to: Some(CanvasSize {
                    width: 360.0,
                    height: 240.0,
                }),
            }]))
            .expect("table can be enlarged for resize test");
        let snapshot = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let rect = snapshot
            .node_rects
            .get(&table)
            .copied()
            .expect("table rect exists");
        let min_size = snapshot
            .node_render_layouts
            .get(&table)
            .expect("table render layout exists")
            .min_size;
        let start = CanvasPoint {
            x: rect.origin.x + rect.size.width,
            y: rect.origin.y + rect.size.height,
        };
        let current = CanvasPoint {
            x: start.x - 1_000.0,
            y: start.y - 1_000.0,
        };

        let plan = app
            .bridge
            .plan_pointer_resize(table, start, current, NodeResizeDirection::BottomRight)
            .expect("overshot resize clamps to renderer minimum");

        assert_eq!(plan.to, min_size);
        app.bridge
            .commit_interaction(ActiveCanvasInteraction::NodeResize {
                node: table,
                direction: NodeResizeDirection::BottomRight,
                start_pointer: start,
                current_pointer: current,
                preview: Some(plan),
            })
            .expect("resize commit succeeds")
            .expect("resize commits");
        assert_eq!(
            app.bridge.store().graph().nodes()[&table].size,
            Some(min_size)
        );
    }

    #[test]
    fn resize_commit_remeasures_node_internals_on_next_snapshot() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let snapshot = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let graph = app.bridge.store().graph();
        let table = graph
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.table").then_some(*node))
            .expect("table node exists");
        assert!(matches!(
            app.bridge.store().node_measurement_status(table),
            NodeMeasurementStatus::Fresh { .. }
        ));
        let rect = snapshot.node_rects[&table];
        let start = CanvasPoint {
            x: rect.origin.x + rect.size.width,
            y: rect.origin.y + rect.size.height,
        };
        let current = CanvasPoint {
            x: start.x + 96.0,
            y: start.y + 32.0,
        };
        let plan = app
            .bridge
            .plan_pointer_resize(table, start, current, NodeResizeDirection::BottomRight)
            .expect("resize plan");

        app.bridge
            .commit_interaction(ActiveCanvasInteraction::NodeResize {
                node: table,
                direction: NodeResizeDirection::BottomRight,
                start_pointer: start,
                current_pointer: current,
                preview: Some(plan),
            })
            .expect("resize commit succeeds")
            .expect("resize commits");

        assert!(
            matches!(
                app.bridge.store().node_measurement_status(table),
                NodeMeasurementStatus::Dirty { .. }
            ),
            "adapter must mark resized node internals dirty so old field handles are not reused"
        );

        let next = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        assert!(
            matches!(
                app.bridge.store().node_measurement_status(table),
                NodeMeasurementStatus::Fresh { .. }
            ),
            "the next adapter snapshot must report current geometry before querying layout facts"
        );
        assert!(matches!(
            next.layout_facts.node_measurement_status(table),
            NodeMeasurementStatus::Fresh { .. }
        ));
        assert_eq!(
            next.node_rects[&table].size,
            app.bridge.store().graph().nodes()[&table].size.unwrap()
        );
    }

    #[test]
    fn zoom_change_invalidates_visible_node_internals_for_density_remeasurement() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder)
            .expect("automation sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let _snapshot = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let node = *app
            .bridge
            .store()
            .graph()
            .nodes()
            .keys()
            .next()
            .expect("node exists");
        assert!(matches!(
            app.bridge.store().node_measurement_status(node),
            NodeMeasurementStatus::Fresh { .. }
        ));

        assert!(
            app.bridge
                .zoom_at_screen(CanvasPoint { x: 400.0, y: 240.0 }, 0.55)
        );

        assert!(
            matches!(
                app.bridge.store().node_measurement_status(node),
                NodeMeasurementStatus::Dirty { .. }
            ),
            "zoom changes can change density/content slots, so adapter measurements must be dirty"
        );
    }

    #[test]
    fn data_change_notification_invalidates_node_internals_until_remeasured() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder)
            .expect("automation sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let snapshot = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let node = *app
            .bridge
            .store()
            .graph()
            .nodes()
            .keys()
            .next()
            .expect("node exists");
        assert!(matches!(
            app.bridge.store().node_measurement_status(node),
            NodeMeasurementStatus::Fresh { .. }
        ));

        app.bridge.notify_node_data_changed(node);

        assert!(matches!(
            app.bridge.store().node_measurement_status(node),
            NodeMeasurementStatus::Dirty {
                reason: NodeInternalsInvalidationReason::DataChanged,
                ..
            }
        ));

        let _stale = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        assert!(
            matches!(
                app.bridge.store().node_measurement_status(node),
                NodeMeasurementStatus::Fresh { revision } if revision > 0
            ),
            "current widget geometry reported during rebuild should clear data-change dirty state"
        );

        let _fresh = app.bridge.rebuild_snapshot(
            &_stale,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        assert!(matches!(
            app.bridge.store().node_measurement_status(node),
            NodeMeasurementStatus::Fresh { .. }
        ));
    }

    #[test]
    fn component_state_change_notification_uses_component_dirty_reason() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder)
            .expect("automation sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let _snapshot = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let node = *app
            .bridge
            .store()
            .graph()
            .nodes()
            .keys()
            .next()
            .expect("node exists");

        app.bridge.notify_node_component_state_changed(node);

        assert!(matches!(
            app.bridge.store().node_measurement_status(node),
            NodeMeasurementStatus::Dirty {
                reason: NodeInternalsInvalidationReason::ComponentStateChanged,
                ..
            }
        ));
    }

    #[test]
    fn node_widget_outcome_control_edit_updates_node_data_and_marks_dirty() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder)
            .expect("automation sample builds");
        let snapshot = app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let _snapshot = app.bridge.rebuild_snapshot(
            &snapshot,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        let node = app
            .bridge
            .store()
            .graph()
            .nodes()
            .iter()
            .find_map(|(id, node)| (node.kind.0 == "demo.llm").then_some(*id))
            .expect("automation llm node exists");

        app.bridge
            .apply_node_widget_outcome(
                node,
                EguiNodeWidgetRenderOutcome {
                    rendered: true,
                    changed: true,
                    control_edits: vec![EguiNodeControlEdit::new(
                        "control.prompt",
                        NodeControlBinding::data_path("fields.prompt"),
                        serde_json::json!("Updated prompt"),
                    )],
                    repeatable_actions: Vec::new(),
                },
            )
            .expect("control edit applies");

        let node_record = &app.bridge.store().graph().nodes()[&node];
        assert_eq!(node_record.data["fields"]["prompt"], "Updated prompt");
        assert!(matches!(
            app.bridge.store().node_measurement_status(node),
            NodeMeasurementStatus::Dirty {
                reason: NodeInternalsInvalidationReason::DataChanged,
                ..
            }
        ));
    }

    #[test]
    fn repeatable_actions_add_move_and_remove_items_through_store_transactions() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let node = app
            .bridge
            .store()
            .graph()
            .nodes()
            .iter()
            .find_map(|(id, node)| (node.kind.0 == "demo.table").then_some(*id))
            .expect("erd table node exists");
        let original_len = app.bridge.store().graph().nodes()[&node].data["columns"]
            .as_array()
            .expect("columns array exists")
            .len();

        app.bridge
            .apply_node_repeatable_action(
                node,
                EguiNodeRepeatableAction::Add {
                    collection_key: "table.columns".to_owned(),
                },
            )
            .expect("add applies");
        assert_eq!(
            app.bridge.store().graph().nodes()[&node].data["columns"]
                .as_array()
                .expect("columns array exists")
                .len(),
            original_len + 1
        );
        assert!(matches!(
            app.bridge.store().node_measurement_status(node),
            NodeMeasurementStatus::Dirty {
                reason: NodeInternalsInvalidationReason::DataChanged,
                ..
            }
        ));

        app.bridge
            .apply_node_repeatable_action(
                node,
                EguiNodeRepeatableAction::Move {
                    collection_key: "table.columns".to_owned(),
                    item_id: "id".to_owned(),
                    direction: EguiRepeatableMoveDirection::Down,
                },
            )
            .expect("move applies");
        assert_eq!(
            app.bridge.store().graph().nodes()[&node].data["columns"][1]["id"],
            "id"
        );

        app.bridge
            .apply_node_repeatable_action(
                node,
                EguiNodeRepeatableAction::Remove {
                    collection_key: "table.columns".to_owned(),
                    item_id: "email".to_owned(),
                },
            )
            .expect("remove applies");
        assert!(
            app.bridge.store().graph().nodes()[&node].data["columns"]
                .as_array()
                .expect("columns array exists")
                .iter()
                .all(|item| item["id"] != "email")
        );
    }

    #[test]
    fn dropped_wire_menu_is_backed_by_authoring_action_descriptors() {
        let app = JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder)
            .expect("automation sample builds");
        let (node, port) = app
            .bridge
            .store()
            .graph()
            .ports()
            .iter()
            .find_map(|(id, port)| {
                (port.dir == PortDirection::Out && port.key.0 == "completion")
                    .then_some((port.node, *id))
            })
            .expect("sample has a completion output port");
        let handle = ConnectionHandleRef::new(node, port, PortDirection::Out);

        let menu = app
            .bridge
            .dropped_wire_menu_for_handle(handle)
            .expect("dropped wire menu exists");

        assert_eq!(menu.key, "menu.dropped_wire");
        assert!(
            menu.action_keys
                .iter()
                .any(|key| key == "action.insert.llm")
        );
        let action = app
            .bridge
            .action_descriptor("action.insert.llm")
            .expect("insert action descriptor exists");
        assert!(action.is_enabled());
    }

    #[test]
    fn product_shape_snapshots_keep_authoring_regions_inside_nodes() {
        for (sample, label, expected_kind, expected_region) in [
            (
                SampleGraphKind::AutomationBuilder,
                "dify workflow",
                "demo.llm",
                Some("config.model"),
            ),
            (
                SampleGraphKind::ShaderGraph,
                "shader graph",
                "demo.shader.mix",
                Some("config.factor"),
            ),
            (
                SampleGraphKind::Erd,
                "erd table",
                "demo.table",
                Some("repeatable.table.columns.header"),
            ),
            (SampleGraphKind::MindMap, "mind map", "demo.topic", None),
        ] {
            let mut app = JellyflowEguiApp::sample(sample).expect("sample builds");
            let snapshot = rebuild_product_snapshot(&mut app, 1.0);
            let node = node_by_kind(&app, expected_kind);
            let layout = snapshot
                .node_render_layouts
                .get(&node)
                .unwrap_or_else(|| panic!("{label} layout exists"));
            if let Some(expected_region) = expected_region {
                assert!(
                    layout
                        .interactive_regions
                        .iter()
                        .any(|region| region.key == expected_region),
                    "{label} should expose `{expected_region}`"
                );
            } else {
                assert!(
                    snapshot
                        .handle_bounds
                        .keys()
                        .any(|handle| handle.node == node),
                    "{label} should preserve handle facts even without rich internals"
                );
            }
            assert_regions_inside_node(&snapshot, node, label);
            assert_regions_do_not_overlap_vertically(layout, label);
        }
    }

    #[test]
    fn rich_node_resize_keeps_regions_and_handles_aligned() {
        let mut app = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let before = rebuild_product_snapshot(&mut app, 1.0);
        let node = node_by_kind_with_region(&app, &before, "demo.table", "field.foreign_key");
        let from_size = app.bridge.store().graph().nodes()[&node].size;

        app.bridge
            .store_mut()
            .dispatch_transaction(&GraphTransaction::from_ops([GraphOp::SetNodeSize {
                id: node,
                from: from_size,
                to: Some(CanvasSize {
                    width: 124.0,
                    height: 72.0,
                }),
            }]))
            .expect("resize update dispatches");

        let snapshot = app.bridge.rebuild_snapshot(
            &before,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        assert_regions_inside_node(&snapshot, node, "resized erd table");
        assert_handle_tracks_anchor(&app, &snapshot, node, "pk", "field.primary_key");
        assert_handle_tracks_anchor(&app, &snapshot, node, "fk", "field.foreign_key");
    }

    #[test]
    fn repeatable_item_port_handles_follow_dynamic_item_regions() {
        let mut app =
            JellyflowEguiApp::sample(SampleGraphKind::ShaderGraph).expect("shader sample builds");

        let snapshot = rebuild_product_snapshot(&mut app, 1.0);
        let node = node_by_kind(&app, "demo.shader.mix");
        assert_handle_tracks_anchor(&app, &snapshot, node, "a", "rail.inputs.a.input");

        let graph = app.bridge.store().graph();
        let port = graph.nodes()[&node]
            .ports
            .iter()
            .copied()
            .find(|port| graph.ports()[port].key.0 == "a")
            .expect("dynamic input port exists");
        let handle = ConnectionHandleRef::new(node, port, graph.ports()[&port].dir);
        let measurement = app
            .bridge
            .store()
            .node_measurement(node)
            .expect("shader mix measurement exists");

        assert!(measurement.anchors.iter().any(|anchor| {
            anchor.anchor == "rail.inputs.a.input"
                && anchor
                    .port_key
                    .as_ref()
                    .is_some_and(|port_key| port_key.0 == "a")
        }));
        assert!(
            app.bridge
                .store()
                .resolve_node_handle_measurement(handle)
                .bounds
                .is_some()
        );
    }

    #[test]
    fn density_modes_have_regression_coverage_for_rich_nodes() {
        assert_eq!(NodeContentLevel::from_zoom(1.0), NodeContentLevel::Full);
        assert_eq!(NodeContentLevel::from_zoom(0.5), NodeContentLevel::Compact);
        assert_eq!(NodeContentLevel::from_zoom(0.12), NodeContentLevel::Shell);

        let mut app = JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder)
            .expect("automation sample builds");
        let full = rebuild_product_snapshot(&mut app, 1.0);
        let node = node_by_kind(&app, "demo.llm");
        assert_regions_inside_node(&full, node, "full density llm");

        let compact = rebuild_product_snapshot(&mut app, 0.5);
        assert!(
            compact
                .handle_bounds
                .keys()
                .any(|handle| handle.node == node),
            "compact density must keep visible handles even when details collapse"
        );
        assert_regions_inside_node(&compact, node, "compact density llm");

        let shell = rebuild_product_snapshot(&mut app, 0.12);
        assert!(
            shell.handle_bounds.keys().any(|handle| handle.node == node),
            "shell density must preserve handle facts for wires"
        );
    }

    #[test]
    fn authoring_interaction_states_have_regression_fixtures() {
        let mut shader =
            JellyflowEguiApp::sample(SampleGraphKind::ShaderGraph).expect("shader sample builds");
        let _snapshot = rebuild_product_snapshot(&mut shader, 1.0);
        assert_shader_invalid_hover(&mut shader);

        let automation = JellyflowEguiApp::sample(SampleGraphKind::AutomationBuilder)
            .expect("automation sample builds");
        let (node, port) = automation
            .bridge
            .store()
            .graph()
            .ports()
            .iter()
            .find_map(|(id, port)| {
                (port.dir == PortDirection::Out && port.key.0 == "completion")
                    .then_some((port.node, *id))
            })
            .expect("completion output port exists");
        let menu = automation
            .bridge
            .dropped_wire_menu_for_handle(ConnectionHandleRef::new(node, port, PortDirection::Out))
            .expect("dropped-wire menu exists");
        assert!(
            menu.action_keys
                .iter()
                .any(|key| key == "action.insert.llm")
        );

        let erd = JellyflowEguiApp::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let descriptor = erd
            .bridge
            .descriptor_for_node(node_by_kind(&erd, "demo.table"))
            .expect("table descriptor exists");
        assert!(!descriptor.inspectors.is_empty());
        assert!(!descriptor.menus.is_empty());
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

    fn rebuild_product_snapshot(app: &mut JellyflowEguiApp, zoom: f32) -> CanvasSnapshot {
        let pan = app.bridge.store().view_state().pan;
        app.bridge.store_mut().set_viewport(pan, zoom);
        app.bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        )
    }

    fn node_by_kind(app: &JellyflowEguiApp, kind: &str) -> NodeId {
        app.bridge
            .store()
            .graph()
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == kind).then_some(*node))
            .unwrap_or_else(|| panic!("node kind `{kind}` exists"))
    }

    fn node_by_kind_with_region(
        app: &JellyflowEguiApp,
        snapshot: &CanvasSnapshot,
        kind: &str,
        region_key: &str,
    ) -> NodeId {
        app.bridge
            .store()
            .graph()
            .nodes()
            .iter()
            .find_map(|(node, record)| {
                (record.kind.0 == kind
                    && snapshot
                        .node_render_layouts
                        .get(node)
                        .is_some_and(|layout| {
                            layout
                                .interactive_regions
                                .iter()
                                .any(|region| region.key == region_key)
                        }))
                .then_some(*node)
            })
            .unwrap_or_else(|| panic!("node kind `{kind}` with region `{region_key}` exists"))
    }

    fn assert_regions_inside_node(snapshot: &CanvasSnapshot, node: NodeId, label: &str) {
        let node_rect = snapshot
            .node_rects
            .get(&node)
            .unwrap_or_else(|| panic!("{label} node rect exists"));
        let layout = snapshot
            .node_render_layouts
            .get(&node)
            .unwrap_or_else(|| panic!("{label} render layout exists"));

        for region in &layout.interactive_regions {
            assert!(
                rect_inside_node(region.rect, node_rect.size, 0.5),
                "{label} region `{}` should stay inside node bounds: {:?} in {:?}",
                region.key,
                region.rect,
                node_rect
            );
        }
    }

    fn assert_regions_do_not_overlap_vertically(
        layout: &crate::renderer::NodeRenderLayout,
        label: &str,
    ) {
        let mut regions = layout
            .interactive_regions
            .iter()
            .filter(|region| region.z_index <= 1)
            .collect::<Vec<_>>();
        regions.sort_by(|a, b| a.rect.origin.y.total_cmp(&b.rect.origin.y));
        for pair in regions.windows(2) {
            let first = pair[0];
            let second = pair[1];
            let first_bottom = first.rect.origin.y + first.rect.size.height;
            assert!(
                first_bottom <= second.rect.origin.y + 0.5,
                "{label} regions `{}` and `{}` should not overlap vertically",
                first.key,
                second.key
            );
        }
    }

    fn assert_handle_tracks_anchor(
        app: &JellyflowEguiApp,
        snapshot: &CanvasSnapshot,
        node: NodeId,
        port_key: &str,
        anchor_key: &str,
    ) {
        let graph = app.bridge.store().graph();
        let port = graph.nodes()[&node]
            .ports
            .iter()
            .copied()
            .find(|port| graph.ports()[port].key.0 == port_key)
            .unwrap_or_else(|| panic!("port `{port_key}` exists"));
        let handle = ConnectionHandleRef::new(node, port, graph.ports()[&port].dir);
        let bounds = snapshot
            .handle_bounds
            .get(&handle)
            .unwrap_or_else(|| panic!("handle bounds for `{port_key}` exist"));
        let anchor = snapshot
            .node_render_layouts
            .get(&node)
            .and_then(|layout| {
                layout
                    .interactive_regions
                    .iter()
                    .find(|region| region.key == anchor_key)
            })
            .unwrap_or_else(|| panic!("anchor `{anchor_key}` exists"));
        let handle_center_y = bounds.rect.origin.y + bounds.rect.size.height * 0.5;
        let anchor_center_y = anchor.rect.origin.y + anchor.rect.size.height * 0.5;

        assert!(
            (handle_center_y - anchor_center_y).abs() <= 1.0,
            "handle `{port_key}` should track anchor `{anchor_key}`: handle_y={handle_center_y}, anchor_y={anchor_center_y}, bounds={:?}, anchor={:?}",
            bounds.rect,
            anchor.rect
        );
    }

    fn rect_inside_node(rect: CanvasRect, node_size: CanvasSize, epsilon: f32) -> bool {
        rect.is_positive_finite()
            && rect.origin.x >= -epsilon
            && rect.origin.y >= -epsilon
            && rect.origin.x + rect.size.width <= node_size.width + epsilon
            && rect.origin.y + rect.size.height <= node_size.height + epsilon
    }

    fn assert_shader_invalid_hover(app: &mut JellyflowEguiApp) {
        let graph = app.bridge.store().graph();
        let texture = graph
            .nodes()
            .iter()
            .find_map(|(node, record)| {
                (record.kind.0 == "demo.shader.texture_sample").then_some(*node)
            })
            .expect("texture sample exists");
        let mix = node_by_kind(app, "demo.shader.mix");
        let color = graph.nodes()[&texture]
            .ports
            .iter()
            .copied()
            .find(|port| {
                graph.ports()[port].key.0 == "color"
                    && graph.ports()[port].dir == PortDirection::Out
            })
            .expect("texture color port exists");
        let factor = graph.nodes()[&mix]
            .ports
            .iter()
            .copied()
            .find(|port| graph.ports()[port].key.0 == "factor")
            .expect("factor port exists");
        let color_handle = ConnectionHandleRef::new(texture, color, graph.ports()[&color].dir);
        let factor_handle = ConnectionHandleRef::new(mix, factor, graph.ports()[&factor].dir);
        let factor_bounds = app
            .bridge
            .store()
            .resolve_node_handle_measurement(factor_handle)
            .bounds
            .expect("factor bounds");
        let mix_pos = graph.nodes()[&mix].pos;
        let pointer = CanvasPoint {
            x: mix_pos.x + factor_bounds.rect.origin.x + factor_bounds.rect.size.width * 0.5,
            y: mix_pos.y + factor_bounds.rect.origin.y + factor_bounds.rect.size.height * 0.5,
        };

        let hover = app.bridge.resolve_connection_target(pointer, color_handle);
        assert_eq!(hover.feedback, ConnectionHandleValidity::Invalid);
        assert!(!hover.is_handle_valid);
    }

    fn shader_vec4() -> TypeDesc {
        TypeDesc::Opaque {
            key: "shader.vec4".to_owned(),
            params: Vec::new(),
        }
    }
}
