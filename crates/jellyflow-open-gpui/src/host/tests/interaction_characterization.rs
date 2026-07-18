use super::*;

#[open_gpui::test]
fn blank_connection_wire_while_the_current_controller_is_committing(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(input.center(), Some(MouseButton::Left), Default::default());

    cx.update_entity(&host, |host, _| {
        let revision = host
            .view
            .graph_snapshot()
            .expect("current graph")
            .revision();
        let generation = host.connections.generation().expect("active connection");
        host.connections
            .commit_to_port(
                generation,
                PortId::from_u128(21),
                || Some(revision),
                || true,
            )
            .expect("characterization enters committing");
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.connection_phase(),
            super::super::OpenGpuiConnectionPhase::Committing,
        );
        assert_eq!(host.scene_evidence.last_planned_wire_count.get(), 0);
        assert_eq!(host.scene_evidence.last_connection_preview.get(), None);
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            0,
            "the legacy controller drops the optimistic wire before a receipt",
        );
    });
}

#[open_gpui::test]
fn old_edge_flash_while_the_current_reconnect_is_committing(cx: &mut open_gpui::TestAppContext) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, occupied_input_graph());
    draw(cx);

    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("occupied input bounds");
    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(px(700.0), px(500.0)),
        Some(MouseButton::Left),
        Default::default(),
    );

    cx.update_entity(&host, |host, _| {
        let revision = host
            .view
            .graph_snapshot()
            .expect("current graph")
            .revision();
        let generation = host.connections.generation().expect("active reconnect");
        host.connections
            .commit_to_port(
                generation,
                PortId::from_u128(21),
                || Some(revision),
                || true,
            )
            .expect("characterization enters committing");
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.connection_phase(),
            super::super::OpenGpuiConnectionPhase::Committing,
        );
        assert_eq!(host.scene_evidence.last_connection_preview.get(), None);
        assert_eq!(
            host.scene_evidence.last_planned_wire_count.get(),
            1,
            "the legacy controller restores the old edge before reconnect settlement",
        );
    });
    assert!(
        runtime
            .snapshot()
            .expect("unchanged graph")
            .graph()
            .edge(&edge)
            .is_some(),
    );
}
