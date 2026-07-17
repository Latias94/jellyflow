use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, GroupId, NodeId};
use jellyflow::runtime::runtime::viewport::{
    ViewportPanRequest, ViewportTransform, ViewportZoomRequest, pan_viewport, zoom_viewport,
};
use jellyflow::runtime::{GraphCommand, GraphRevision, NodeGeometryEdit, ViewSession};
use open_gpui::{Context, PinchEvent, Pixels, ScrollWheelEvent, Window, px};

use crate::command_journal::OpenGpuiCommandOrigin;

use crate::interaction::{
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGestureEvent, OpenGpuiSceneGestureKind,
    OpenGpuiSceneGestureListener, OpenGpuiSceneGesturePhase,
};

use super::OpenGpuiSceneHost;

const SCENE_DRAG_THRESHOLD: f32 = 1.0;
const SCENE_MIN_ZOOM: f32 = 0.05;
const SCENE_MAX_ZOOM: f32 = 8.0;
const SCENE_WHEEL_LINE_HEIGHT: f32 = 16.0;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CanvasSelectionWork {
    initial_order_items_copied: usize,
    initial_membership_items_copied: usize,
    query_items_examined: usize,
    owned_handoffs: usize,
    owned_node_items_handed_off: usize,
}

#[derive(Debug, Default)]
pub(super) struct SceneGestureController {
    state: Option<SceneGestureState>,
    pending_node_drag: Option<PendingNodeDrag>,
}

impl SceneGestureController {
    #[cfg(test)]
    pub(super) fn is_active(&self) -> bool {
        self.state.is_some()
    }

    pub(super) fn start(
        &mut self,
        event: &OpenGpuiSceneGestureEvent,
        start_screen: CanvasPoint,
        view: &ViewSession,
    ) -> bool {
        if self.state.is_some() || self.pending_node_drag.is_some() {
            return false;
        }
        let Ok(transform) = view.viewport_transform() else {
            return false;
        };

        let state = match event.kind() {
            OpenGpuiSceneGestureKind::Node(instance) => {
                let Ok(snapshot) = view.graph_snapshot() else {
                    return false;
                };
                let Ok(selection) = view.selection() else {
                    return false;
                };
                let clicked = instance.node();
                let Some(clicked_node) = snapshot.graph().node(&clicked) else {
                    return false;
                };
                let additive = event.modifiers().shift || event.modifiers().secondary();
                let mut nodes = selection.nodes().to_vec();
                nodes.retain(|node| {
                    snapshot
                        .graph()
                        .node(node)
                        .is_some_and(|node| node.selectable.unwrap_or(true))
                });
                let clicked_selectable = clicked_node.selectable.unwrap_or(true);
                if clicked_selectable && !nodes.contains(&clicked) {
                    if additive {
                        nodes.push(clicked);
                    } else {
                        nodes = vec![clicked];
                    }
                }
                let edges = if additive || !clicked_selectable {
                    selection.edges().to_vec()
                } else {
                    Vec::new()
                };
                let groups = if additive || !clicked_selectable {
                    selection.groups().to_vec()
                } else {
                    Vec::new()
                };
                if view.set_selection(nodes.clone(), edges, groups).is_err() {
                    return false;
                }
                if !clicked_node.draggable.unwrap_or(true) {
                    return false;
                }
                let drag_nodes = if nodes.contains(&clicked) {
                    nodes
                } else {
                    vec![clicked]
                };
                let positions = drag_nodes
                    .into_iter()
                    .filter_map(|node| {
                        snapshot
                            .graph()
                            .node(&node)
                            .filter(|record| record.draggable.unwrap_or(true))
                            .map(|record| (node, record.pos))
                    })
                    .collect::<BTreeMap<_, _>>();
                if positions.is_empty() {
                    return false;
                }
                SceneGestureState::NodeDrag(NodeDragGesture {
                    start_screen,
                    start_zoom: transform.zoom,
                    expected_revision: snapshot.revision(),
                    positions,
                    delta: CanvasPoint::default(),
                    moved: false,
                })
            }
            OpenGpuiSceneGestureKind::Pan => SceneGestureState::Pan(CanvasPanGesture {
                start_screen,
                transform,
            }),
            OpenGpuiSceneGestureKind::Selection => {
                let Ok(selection) = view.selection() else {
                    return false;
                };
                let additive = event.modifiers().shift || event.modifiers().secondary();
                let state = CanvasSelectionGesture::new(
                    start_screen,
                    transform,
                    selection.nodes().to_vec(),
                    selection.edges().to_vec(),
                    selection.groups().to_vec(),
                    additive,
                );
                if !additive
                    && view
                        .set_selection(Vec::new(), Vec::new(), Vec::new())
                        .is_err()
                {
                    return false;
                }
                SceneGestureState::Selection(state)
            }
        };
        self.state = Some(state);
        true
    }

    pub(super) fn update(
        &mut self,
        event: &OpenGpuiSceneGestureEvent,
        screen: CanvasPoint,
        view: &ViewSession,
    ) {
        if !self
            .state
            .as_ref()
            .is_some_and(|state| state.matches(event.kind()))
        {
            return;
        }

        let mut viewport = None;
        let mut selection = None;
        match self.state.as_mut() {
            Some(SceneGestureState::NodeDrag(drag)) => {
                let screen_delta = CanvasPoint {
                    x: screen.x - drag.start_screen.x,
                    y: screen.y - drag.start_screen.y,
                };
                drag.moved = screen_delta.x.abs() >= SCENE_DRAG_THRESHOLD
                    || screen_delta.y.abs() >= SCENE_DRAG_THRESHOLD;
                drag.delta = if drag.moved {
                    CanvasPoint {
                        x: screen_delta.x / drag.start_zoom,
                        y: screen_delta.y / drag.start_zoom,
                    }
                } else {
                    CanvasPoint::default()
                };
            }
            Some(SceneGestureState::Pan(pan)) => {
                viewport = pan_viewport(
                    pan.transform,
                    ViewportPanRequest::new(CanvasPoint {
                        x: screen.x - pan.start_screen.x,
                        y: screen.y - pan.start_screen.y,
                    }),
                );
            }
            Some(SceneGestureState::Selection(box_selection)) => {
                box_selection.current_screen = screen;
                box_selection.moved = (screen.x - box_selection.start_screen.x).abs()
                    >= SCENE_DRAG_THRESHOLD
                    || (screen.y - box_selection.start_screen.y).abs() >= SCENE_DRAG_THRESHOLD;
                if box_selection.moved {
                    let bounds = canvas_rect_between(
                        box_selection
                            .transform
                            .canvas_point_at_screen(box_selection.start_screen),
                        box_selection
                            .transform
                            .canvas_point_at_screen(box_selection.current_screen),
                    );
                    let mut intersecting = view.nodes_intersecting(bounds).unwrap_or_default();
                    if let Ok(snapshot) = view.graph_snapshot() {
                        intersecting.retain(|node| {
                            snapshot
                                .graph()
                                .node(node)
                                .is_some_and(|node| node.selectable.unwrap_or(true))
                        });
                    } else {
                        intersecting.clear();
                    }

                    if let Some(nodes) =
                        box_selection.selection_for_intersecting_nodes(intersecting)
                    {
                        selection = Some((
                            nodes,
                            if box_selection.additive {
                                box_selection.initial_edges.clone()
                            } else {
                                Vec::new()
                            },
                            if box_selection.additive {
                                box_selection.initial_groups.clone()
                            } else {
                                Vec::new()
                            },
                        ));
                    }
                }
            }
            None => {}
        }

        if let Some(transform) = viewport {
            let _ = view.set_viewport(transform.pan, transform.zoom);
        }
        if let Some((nodes, edges, groups)) = selection {
            let _ = view.set_selection(nodes, edges, groups);
        }
    }

    pub(super) fn finish_command(&mut self) -> Option<GraphCommand> {
        let Some(SceneGestureState::NodeDrag(drag)) = self.state.take() else {
            return None;
        };
        if !drag.moved || !drag.delta.is_finite() {
            return None;
        }
        let edits = drag
            .positions
            .into_iter()
            .filter_map(|(node, position)| {
                let next = CanvasPoint {
                    x: position.x + drag.delta.x,
                    y: position.y + drag.delta.y,
                };
                next.is_finite()
                    .then_some(NodeGeometryEdit::new(node).with_position(next))
            })
            .collect::<Vec<_>>();
        if edits.is_empty() {
            return None;
        }
        self.pending_node_drag = Some(PendingNodeDrag {
            nodes: edits.iter().map(NodeGeometryEdit::node).collect(),
            delta: drag.delta,
        });
        Some(
            GraphCommand::set_node_geometry(drag.expected_revision, edits).with_label("Move nodes"),
        )
    }

    pub(super) fn cancel(&mut self, view: &ViewSession) {
        self.settle_node_drag();
        let Some(state) = self.state.take() else {
            return;
        };
        match state {
            SceneGestureState::NodeDrag(_) => {}
            SceneGestureState::Pan(pan) => {
                let _ = view.set_viewport(pan.transform.pan, pan.transform.zoom);
            }
            SceneGestureState::Selection(selection) => {
                let _ = view.set_selection(
                    selection.initial_nodes,
                    selection.initial_edges,
                    selection.initial_groups,
                );
            }
        }
    }

    pub(super) fn node_drag_delta(&self, node: NodeId) -> Option<CanvasPoint> {
        self.state
            .as_ref()
            .and_then(|state| match state {
                SceneGestureState::NodeDrag(drag) if drag.positions.contains_key(&node) => {
                    Some(drag.delta)
                }
                _ => None,
            })
            .or_else(|| {
                self.pending_node_drag
                    .as_ref()
                    .and_then(|drag| drag.nodes.contains(&node).then_some(drag.delta))
            })
    }

    pub(super) fn settle_node_drag(&mut self) {
        self.pending_node_drag = None;
    }

    #[cfg(test)]
    pub(super) fn retain_node_drag_for_test(&mut self, node: NodeId, delta: CanvasPoint) {
        self.pending_node_drag = Some(PendingNodeDrag {
            nodes: BTreeSet::from([node]),
            delta,
        });
    }

    pub(super) fn has_node_drag_visual(&self) -> bool {
        matches!(self.state, Some(SceneGestureState::NodeDrag(_)))
            || self.pending_node_drag.is_some()
    }

    pub(super) fn selection_rectangle(&self) -> Option<CanvasRect> {
        let Some(SceneGestureState::Selection(selection)) = self.state.as_ref() else {
            return None;
        };
        selection
            .moved
            .then(|| canvas_rect_between(selection.start_screen, selection.current_screen))
    }
}

#[derive(Debug)]
enum SceneGestureState {
    NodeDrag(NodeDragGesture),
    Pan(CanvasPanGesture),
    Selection(CanvasSelectionGesture),
}

impl SceneGestureState {
    fn matches(&self, kind: &OpenGpuiSceneGestureKind) -> bool {
        matches!(
            (self, kind),
            (Self::NodeDrag(_), OpenGpuiSceneGestureKind::Node(_))
                | (Self::Pan(_), OpenGpuiSceneGestureKind::Pan)
                | (Self::Selection(_), OpenGpuiSceneGestureKind::Selection)
        )
    }
}

#[derive(Debug)]
struct NodeDragGesture {
    start_screen: CanvasPoint,
    start_zoom: f32,
    expected_revision: GraphRevision,
    positions: BTreeMap<NodeId, CanvasPoint>,
    delta: CanvasPoint,
    moved: bool,
}

#[derive(Debug)]
struct PendingNodeDrag {
    nodes: BTreeSet<NodeId>,
    delta: CanvasPoint,
}

#[derive(Debug)]
struct CanvasPanGesture {
    start_screen: CanvasPoint,
    transform: ViewportTransform,
}

#[derive(Debug)]
struct CanvasSelectionGesture {
    start_screen: CanvasPoint,
    transform: ViewportTransform,
    initial_nodes: Vec<NodeId>,
    initial_edges: Vec<EdgeId>,
    initial_groups: Vec<GroupId>,
    additive: bool,
    base_node_count: usize,
    base_node_membership: BTreeSet<NodeId>,
    selected_nodes: Vec<NodeId>,
    selected_node_membership: BTreeSet<NodeId>,
    current_screen: CanvasPoint,
    moved: bool,
    #[cfg(test)]
    work: CanvasSelectionWork,
}

impl CanvasSelectionGesture {
    fn new(
        start_screen: CanvasPoint,
        transform: ViewportTransform,
        initial_nodes: Vec<NodeId>,
        initial_edges: Vec<EdgeId>,
        initial_groups: Vec<GroupId>,
        additive: bool,
    ) -> Self {
        let selected_nodes = if additive {
            initial_nodes.clone()
        } else {
            Vec::new()
        };
        let base_node_count = selected_nodes.len();
        let base_node_membership = selected_nodes.iter().copied().collect::<BTreeSet<_>>();
        Self {
            start_screen,
            transform,
            initial_nodes,
            initial_edges,
            initial_groups,
            additive,
            base_node_count,
            selected_node_membership: base_node_membership.clone(),
            base_node_membership,
            selected_nodes,
            current_screen: start_screen,
            moved: false,
            #[cfg(test)]
            work: CanvasSelectionWork::default(),
        }
    }

    fn selection_for_intersecting_nodes(
        &mut self,
        mut intersecting: Vec<NodeId>,
    ) -> Option<Vec<NodeId>> {
        #[cfg(test)]
        {
            self.work.query_items_examined = self
                .work
                .query_items_examined
                .saturating_add(intersecting.len());
        }

        let mut next_membership = BTreeSet::new();
        intersecting.retain(|node| {
            !self.base_node_membership.contains(node) && next_membership.insert(*node)
        });
        if self.selected_nodes[self.base_node_count..] == intersecting {
            return None;
        }

        for node in self.selected_nodes.drain(self.base_node_count..) {
            let removed = self.selected_node_membership.remove(&node);
            debug_assert!(removed, "ordered selection and membership must agree");
        }
        self.selected_node_membership
            .extend(intersecting.iter().copied());
        self.selected_nodes.extend(intersecting);
        debug_assert_eq!(
            self.selected_nodes.len(),
            self.selected_node_membership.len(),
            "ordered selection and membership must stay aligned"
        );

        // ViewSession owns its input. Retain one ordered copy for the next move comparison and
        // create exactly one owned handoff only when the effective selection changed.
        let nodes = self.selected_nodes.clone();
        #[cfg(test)]
        {
            self.work.owned_handoffs = self.work.owned_handoffs.saturating_add(1);
            self.work.owned_node_items_handed_off = self
                .work
                .owned_node_items_handed_off
                .saturating_add(nodes.len());
        }
        Some(nodes)
    }
}

fn canvas_rect_between(first: CanvasPoint, second: CanvasPoint) -> CanvasRect {
    let min_x = first.x.min(second.x);
    let min_y = first.y.min(second.y);
    CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: (first.x - second.x).abs(),
            height: (first.y - second.y).abs(),
        },
    }
}

impl OpenGpuiSceneHost {
    #[cfg(test)]
    pub(super) fn active_scene_gesture(&self) -> bool {
        self.scene_gesture.is_active()
    }

    pub(super) fn host_scene_listener(cx: &Context<Self>) -> OpenGpuiSceneGestureListener {
        let host = cx.weak_entity();
        Rc::new(move |event, window, cx| {
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                host.handle_scene_gesture(event, window, cx);
            });
        })
    }

    fn handle_scene_gesture(
        &mut self,
        event: OpenGpuiSceneGestureEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        self.scene_evidence.record_scene_event(&event);
        match event.phase() {
            OpenGpuiSceneGesturePhase::Start => {
                if !self.start_scene_gesture(&event) {
                    self.scene_interactions
                        .cancel_deferred(OpenGpuiSceneGestureCancelReason::InteractionUnavailable);
                }
            }
            OpenGpuiSceneGesturePhase::Move => self.update_scene_gesture(&event),
            OpenGpuiSceneGesturePhase::Release => {
                self.update_scene_gesture(&event);
                self.finish_scene_gesture(cx);
            }
            OpenGpuiSceneGesturePhase::Cancel(_) => self.cancel_scene_gesture(),
        }
        cx.notify();
    }

    fn start_scene_gesture(&mut self, event: &OpenGpuiSceneGestureEvent) -> bool {
        let Some(start_screen) = self.scene_screen_position(event.position()) else {
            return false;
        };
        self.scene_gesture.start(event, start_screen, &self.view)
    }

    fn update_scene_gesture(&mut self, event: &OpenGpuiSceneGestureEvent) {
        let Some(screen) = self.scene_screen_position(event.position()) else {
            return;
        };
        self.scene_gesture.update(event, screen, &self.view);
    }

    fn finish_scene_gesture(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self.scene_gesture.finish_command() else {
            return;
        };
        if let Err(error) = self
            .commands
            .enqueue_detached(command, OpenGpuiCommandOrigin::NodeDrag)
        {
            self.scene_gesture.settle_node_drag();
            self.record_command_diagnostic(error.to_string());
        } else {
            cx.notify();
        }
    }

    fn cancel_scene_gesture(&mut self) {
        self.scene_gesture.cancel(&self.view);
    }

    pub(super) fn handle_scene_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(transform) = self.view.viewport_transform() else {
            return false;
        };
        let Some(anchor) = self.scene_screen_position(event.position) else {
            return false;
        };
        let delta = event.delta.pixel_delta(px(SCENE_WHEEL_LINE_HEIGHT));
        let delta = CanvasPoint {
            x: delta.x.as_f32(),
            y: delta.y.as_f32(),
        };
        let next = if event.modifiers.shift || delta.x.abs() > delta.y.abs() {
            pan_viewport(transform, ViewportPanRequest::new(delta))
        } else {
            zoom_viewport(
                transform,
                ViewportZoomRequest::new(
                    anchor,
                    transform.zoom * (delta.y * 0.005).exp(),
                    SCENE_MIN_ZOOM,
                    SCENE_MAX_ZOOM,
                ),
            )
        };
        let Some(next) = next else {
            return false;
        };
        if self.view.set_viewport(next.pan, next.zoom).is_err() {
            return false;
        }
        cx.notify();
        true
    }

    pub(super) fn handle_scene_pinch(
        &mut self,
        event: &PinchEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(transform) = self.view.viewport_transform() else {
            return false;
        };
        let Some(anchor) = self.scene_screen_position(event.position) else {
            return false;
        };
        let Some(next) = zoom_viewport(
            transform,
            ViewportZoomRequest::new(
                anchor,
                transform.zoom * (1.0 + event.delta).max(0.1),
                SCENE_MIN_ZOOM,
                SCENE_MAX_ZOOM,
            ),
        ) else {
            return false;
        };
        if self.view.set_viewport(next.pan, next.zoom).is_err() {
            return false;
        }
        cx.notify();
        true
    }

    pub(super) fn scene_screen_position(
        &self,
        position: open_gpui::Point<Pixels>,
    ) -> Option<CanvasPoint> {
        let origin = self.scene_origin?;
        let screen = CanvasPoint {
            x: (position.x - origin.x).as_f32(),
            y: (position.y - origin.y).as_f32(),
        };
        screen.is_finite().then_some(screen)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jellyflow::core::{CanvasPoint, NodeId};
    use jellyflow::runtime::runtime::viewport::ViewportTransform;

    use super::{
        CanvasSelectionGesture, NodeDragGesture, SceneGestureController, SceneGestureState,
    };

    #[test]
    fn finished_node_drag_retains_visual_delta_until_command_settlement() {
        let node = NodeId::from_u128(1);
        let delta = CanvasPoint { x: 42.0, y: 18.0 };
        let mut controller = SceneGestureController {
            state: Some(SceneGestureState::NodeDrag(NodeDragGesture {
                start_screen: CanvasPoint::default(),
                start_zoom: 1.0,
                expected_revision: 7,
                positions: BTreeMap::from([(node, CanvasPoint { x: 10.0, y: 20.0 })]),
                delta,
                moved: true,
            })),
            pending_node_drag: None,
        };

        controller
            .finish_command()
            .expect("moved drag creates a geometry command");
        assert_eq!(controller.node_drag_delta(node), Some(delta));

        controller.settle_node_drag();
        assert_eq!(controller.node_drag_delta(node), None);
    }

    #[test]
    fn additive_selection_has_bounded_copy_and_handoff_work_across_pointer_moves() {
        const INITIAL_SELECTION_LEN: usize = 10_000;
        const EFFECTIVE_CHANGES: usize = 64;
        const POINTER_MOVES: usize = 128;

        let initial_nodes = (1..=INITIAL_SELECTION_LEN)
            .map(|id| NodeId::from_u128(id as u128))
            .collect::<Vec<_>>();
        let added = [
            NodeId::from_u128((INITIAL_SELECTION_LEN + 1) as u128),
            NodeId::from_u128((INITIAL_SELECTION_LEN + 2) as u128),
        ];
        let mut selection = CanvasSelectionGesture::new(
            CanvasPoint::default(),
            ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid transform"),
            initial_nodes,
            Vec::new(),
            Vec::new(),
            true,
        );

        for change in 0..EFFECTIVE_CHANGES {
            let intersecting = vec![added[change % added.len()]];
            assert_eq!(
                selection
                    .selection_for_intersecting_nodes(intersecting.clone())
                    .expect("alternating hit changes selection")
                    .len(),
                INITIAL_SELECTION_LEN + 1
            );
            assert_eq!(
                selection.selection_for_intersecting_nodes(intersecting),
                None
            );
        }

        assert_eq!(selection.work.query_items_examined, POINTER_MOVES);
        assert_eq!(
            selection.work.initial_order_items_copied, 0,
            "pointer moves must reuse the initial ordered selection"
        );
        assert_eq!(
            selection.work.initial_membership_items_copied, 0,
            "pointer moves must reuse the initial selection membership"
        );
        assert_eq!(selection.work.owned_handoffs, EFFECTIVE_CHANGES);
        assert_eq!(
            selection.work.owned_node_items_handed_off,
            (INITIAL_SELECTION_LEN + 1) * EFFECTIVE_CHANGES
        );
    }

    #[test]
    fn additive_selection_preserves_base_and_intersection_order() {
        let base = vec![NodeId::from_u128(3), NodeId::from_u128(1)];
        let mut selection = CanvasSelectionGesture::new(
            CanvasPoint::default(),
            ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid transform"),
            base.clone(),
            Vec::new(),
            Vec::new(),
            true,
        );

        assert_eq!(
            selection.selection_for_intersecting_nodes(vec![
                NodeId::from_u128(1),
                NodeId::from_u128(5),
                NodeId::from_u128(4),
                NodeId::from_u128(5),
            ]),
            Some(vec![
                NodeId::from_u128(3),
                NodeId::from_u128(1),
                NodeId::from_u128(5),
                NodeId::from_u128(4),
            ])
        );
        assert_eq!(
            selection.selected_node_membership,
            [1_u128, 3, 4, 5]
                .into_iter()
                .map(NodeId::from_u128)
                .collect()
        );
        assert_eq!(
            selection
                .selection_for_intersecting_nodes(
                    vec![NodeId::from_u128(4), NodeId::from_u128(5),]
                ),
            Some(vec![
                NodeId::from_u128(3),
                NodeId::from_u128(1),
                NodeId::from_u128(4),
                NodeId::from_u128(5),
            ])
        );
        assert_eq!(
            selection
                .selection_for_intersecting_nodes(
                    vec![NodeId::from_u128(4), NodeId::from_u128(5),]
                ),
            None
        );
        assert_eq!(selection.initial_nodes, base);
    }

    #[test]
    fn non_additive_selection_replaces_active_nodes_and_keeps_cancel_state() {
        let initial = vec![NodeId::from_u128(3), NodeId::from_u128(1)];
        let mut selection = CanvasSelectionGesture::new(
            CanvasPoint::default(),
            ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid transform"),
            initial.clone(),
            Vec::new(),
            Vec::new(),
            false,
        );

        assert!(selection.selected_nodes.is_empty());
        assert_eq!(
            selection.selection_for_intersecting_nodes(vec![
                NodeId::from_u128(1),
                NodeId::from_u128(5),
                NodeId::from_u128(1),
            ]),
            Some(vec![NodeId::from_u128(1), NodeId::from_u128(5)])
        );
        assert_eq!(selection.initial_nodes, initial);
        assert_eq!(
            selection.selection_for_intersecting_nodes(Vec::new()),
            Some(Vec::new())
        );
        assert_eq!(selection.selection_for_intersecting_nodes(Vec::new()), None);
    }
}
