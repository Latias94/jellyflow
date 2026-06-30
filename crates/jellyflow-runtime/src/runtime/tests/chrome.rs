use crate::runtime::chrome::{
    NodeChromeFactsRequest, NodeChromeLayoutPolicy, NodeChromeState, resolve_node_chrome_facts,
};
use crate::runtime::resize::NodeResizeConstraints;
use crate::schema::{NodeChromeDescriptor, NodeChromeKind, NodeChromePlacement};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

#[test]
fn selected_node_resolves_chrome_affordances_with_resize_constraints() {
    let node = NodeId::from_u128(10);
    let rect = CanvasRect {
        origin: CanvasPoint { x: 100.0, y: 50.0 },
        size: CanvasSize {
            width: 220.0,
            height: 140.0,
        },
    };
    let constraints = NodeResizeConstraints::new(
        Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        }),
        Some(CanvasSize {
            width: 360.0,
            height: 240.0,
        }),
    );
    let descriptors = vec![
        NodeChromeDescriptor::resizer("resize.corner").with_order(20),
        NodeChromeDescriptor::toolbar("toolbar.primary", NodeChromePlacement::TopRight)
            .with_renderer_key("node-toolbar")
            .with_order(10),
        NodeChromeDescriptor::status_strip("status.run", NodeChromePlacement::InsideFooter)
            .with_order(30),
        NodeChromeDescriptor::run_action_strip("actions.run", NodeChromePlacement::Bottom)
            .with_order(40),
    ];

    let unselected =
        resolve_node_chrome_facts(NodeChromeFactsRequest::new(node, rect, &descriptors))
            .expect("finite node rect");

    assert_eq!(unselected.chrome.len(), 1);
    assert_eq!(unselected.chrome[0].key, "status.run");
    assert_eq!(unselected.chrome[0].kind, NodeChromeKind::StatusStrip);

    let selected = resolve_node_chrome_facts(
        NodeChromeFactsRequest::new(node, rect, &descriptors)
            .with_state(NodeChromeState::selected())
            .with_resize_constraints(constraints),
    )
    .expect("selected chrome facts");

    let keys: Vec<_> = selected
        .chrome
        .iter()
        .map(|chrome| chrome.key.as_str())
        .collect();
    assert_eq!(
        keys,
        vec![
            "toolbar.primary",
            "resize.corner",
            "status.run",
            "actions.run"
        ],
        "chrome facts should follow semantic order before adapter rendering"
    );
    assert_eq!(
        selected
            .get("resize.corner")
            .and_then(|chrome| chrome.resize_constraints),
        Some(constraints)
    );
    assert_eq!(
        selected
            .get("toolbar.primary")
            .and_then(|chrome| chrome.renderer_key.as_deref()),
        Some("node-toolbar")
    );
    assert!(selected.get("actions.run").expect("run strip").interactive);
}

#[test]
fn toolbar_placement_follows_bounds_after_drag_resize_and_zoom_policy() {
    let node = NodeId::from_u128(11);
    let descriptors = [NodeChromeDescriptor::toolbar(
        "toolbar.primary",
        NodeChromePlacement::TopRight,
    )];
    let initial_rect = CanvasRect {
        origin: CanvasPoint { x: 40.0, y: 80.0 },
        size: CanvasSize {
            width: 160.0,
            height: 96.0,
        },
    };
    let moved_resized_rect = CanvasRect {
        origin: CanvasPoint { x: 120.0, y: 150.0 },
        size: CanvasSize {
            width: 240.0,
            height: 160.0,
        },
    };
    let policy = NodeChromeLayoutPolicy::default()
        .with_bar_height(24.0)
        .with_margin(6.0)
        .with_zoom(2.0);

    let initial = resolve_node_chrome_facts(
        NodeChromeFactsRequest::new(node, initial_rect, &descriptors)
            .with_state(NodeChromeState::selected())
            .with_policy(policy),
    )
    .expect("initial facts");
    let moved = resolve_node_chrome_facts(
        NodeChromeFactsRequest::new(node, moved_resized_rect, &descriptors)
            .with_state(NodeChromeState::selected())
            .with_policy(policy),
    )
    .expect("moved facts");

    let initial_toolbar = initial.get("toolbar.primary").expect("toolbar");
    let moved_toolbar = moved.get("toolbar.primary").expect("toolbar");

    assert_eq!(initial_toolbar.rect.origin.x, 120.0);
    assert_eq!(initial_toolbar.rect.origin.y, 65.0);
    assert_eq!(initial_toolbar.rect.size.height, 12.0);
    assert_eq!(moved_toolbar.rect.origin.x, 280.0);
    assert_eq!(moved_toolbar.rect.origin.y, 135.0);
    assert_eq!(moved_toolbar.rect.size.height, 12.0);
}

#[test]
fn invalid_node_bounds_do_not_emit_chrome_facts() {
    let node = NodeId::from_u128(12);
    let descriptors = [NodeChromeDescriptor::status_strip(
        "status.run",
        NodeChromePlacement::InsideFooter,
    )];
    let invalid_rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 0.0,
            height: 120.0,
        },
    };

    assert_eq!(
        resolve_node_chrome_facts(NodeChromeFactsRequest::new(
            node,
            invalid_rect,
            &descriptors
        )),
        None
    );
}
