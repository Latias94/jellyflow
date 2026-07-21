use std::sync::Arc;

use jellyflow_runtime::{ConnectionProbe, ViewSessionError};
use open_gpui::{Context, Window};

use crate::presentation::{
    OpenGpuiNodeInteractionState, OpenGpuiNodeLod, OpenGpuiNodePresentationSnapshot,
    OpenGpuiPortCandidateState, OpenGpuiPortInteractionState, OpenGpuiSettlementState,
};
use crate::semantic_zoom::OpenGpuiNodePresentation;

use super::OpenGpuiSceneHost;

impl OpenGpuiSceneHost {
    pub(super) fn sync_binding_presentations(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ViewSessionError> {
        let graph = self.view.graph_snapshot()?;
        if self.hovered_node.is_some_and(|hovered| {
            !self
                .mounts
                .active_in_order()
                .any(|(instance, _)| instance.node() == hovered)
        }) {
            self.hovered_node = None;
            self.hovered_port = None;
        }
        if self.hovered_port.is_some_and(|hovered| {
            graph
                .graph()
                .port(&hovered)
                .is_none_or(|port| Some(port.node) != self.hovered_node)
                || self
                    .view
                    .geometry_snapshot()
                    .ok()
                    .is_none_or(|geometry| geometry.port(hovered).is_none())
        }) {
            self.hovered_port = None;
        }
        let interaction = self.interaction_owner.presentation();
        let connection = interaction.connection().cloned();
        let node_drag = interaction.node_drag().cloned();
        let updates = self
            .mounts
            .active_in_order()
            .filter_map(|(instance, mounted)| {
                let binding = mounted.binding.clone()?;
                let node_focused = mounted.focus.contains_focused(window, cx);
                let focused_ports = binding.read(cx);
                let node = graph.graph().node(&instance.node())?;
                let lod = match mounted.graph_presentation() {
                    OpenGpuiNodePresentation::Detail => OpenGpuiNodeLod::Detail,
                    OpenGpuiNodePresentation::Overview if mounted.has_interaction_island() => {
                        OpenGpuiNodeLod::InteractionIsland
                    }
                    OpenGpuiNodePresentation::Overview => OpenGpuiNodeLod::Overview,
                };
                let node_pending = node_drag.as_ref().is_some_and(|drag| {
                    drag.ticket().is_some() && drag.nodes().contains(&instance.node())
                });
                let node_state = OpenGpuiNodeInteractionState::idle()
                    .with_selected(mounted.selected)
                    .with_hovered(self.hovered_node == Some(instance.node()))
                    .with_focused(node_focused)
                    .with_dragging(
                        node_drag
                            .as_ref()
                            .is_some_and(|drag| drag.nodes().contains(&instance.node())),
                    )
                    .with_disabled(!node.selectable.unwrap_or(true))
                    .with_lod(lod)
                    .with_settlement(if node_pending {
                        OpenGpuiSettlementState::Pending
                    } else {
                        OpenGpuiSettlementState::Idle
                    });
                let ports = node
                    .ports
                    .iter()
                    .filter_map(|port| {
                        let facts = self.view.port_connection_summary(*port).ok().flatten()?;
                        let disabled =
                            !facts.can_start_connection() && !facts.can_accept_connection();
                        let mut candidate = OpenGpuiPortCandidateState::Idle;
                        let mut pending = false;
                        if let Some(connection) = &connection {
                            let source = connection
                                .intent()
                                .detached_port()
                                .unwrap_or_else(|| connection.fixed_port());
                            if source == *port {
                                candidate = OpenGpuiPortCandidateState::Source;
                                pending = connection.ticket().is_some();
                            } else if connection.probe().target().map(|target| target.port())
                                == Some(*port)
                            {
                                candidate = match connection.probe() {
                                    ConnectionProbe::Valid { .. } => {
                                        OpenGpuiPortCandidateState::ValidTarget
                                    }
                                    ConnectionProbe::Invalid { diagnostics, .. } => {
                                        OpenGpuiPortCandidateState::invalid(Arc::from(
                                            diagnostics.clone(),
                                        ))
                                    }
                                    ConnectionProbe::None => OpenGpuiPortCandidateState::Idle,
                                };
                                pending = connection.ticket().is_some();
                            }
                        }
                        Some((
                            *port,
                            OpenGpuiPortInteractionState::idle()
                                .with_hovered(
                                    interaction.phase()
                                        == jellyflow_runtime::CanvasInteractionPhase::Idle
                                        && self.hovered_port == Some(*port),
                                )
                                .with_connected(facts.is_connected())
                                .with_disabled(disabled)
                                .with_focused(
                                    focused_ports
                                        .port_focus_handle(*port)
                                        .is_some_and(|focus| focus.is_focused(window)),
                                )
                                .with_candidate(candidate)
                                .with_settlement(if pending {
                                    OpenGpuiSettlementState::Pending
                                } else {
                                    OpenGpuiSettlementState::Idle
                                }),
                        ))
                    })
                    .collect::<Vec<_>>();
                Some((
                    binding,
                    OpenGpuiNodePresentationSnapshot::new(node_state, ports),
                ))
            })
            .collect::<Vec<_>>();

        for (binding, presentation) in updates {
            let mut changed = false;
            binding.update(cx, |binding, cx| {
                changed = binding.set_presentation(presentation);
                if changed {
                    cx.notify();
                }
            });
            if changed {
                self.stats.binding_updates = self.stats.binding_updates.saturating_add(1);
                self.stats.binding_notifications =
                    self.stats.binding_notifications.saturating_add(1);
            }
        }
        Ok(())
    }
}
