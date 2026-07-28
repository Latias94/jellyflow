use std::collections::BTreeMap;

use jellyflow_core::{CanvasPoint, NodeId, PortId};
use jellyflow_runtime::{
    graph::GraphRevision, graph::PortConnectionSummary, interaction::CanvasInteractionPhase,
    interaction::CanvasInteractionPresentation, interaction::ConnectionProbe,
    interaction::KeyboardConnectionFeedback, interaction::KeyboardConnectionPort,
    view::ViewSession, view::ViewSessionError,
};
use open_gpui::{Context, Window};

use crate::binding::{
    OpenGpuiConnectionCandidateInput, OpenGpuiNodePresentationInputSignature,
    OpenGpuiPortCandidateInput, OpenGpuiPortPresentationInput, OpenGpuiProjectedInteractionInput,
};
use crate::presentation::{OpenGpuiNodeInteractionState, OpenGpuiNodeLod, OpenGpuiSettlementState};
use crate::semantic_zoom::OpenGpuiNodePresentation;

use super::OpenGpuiSceneHost;

#[derive(Default)]
pub(super) struct OpenGpuiPresentationPortCache {
    revision: Option<GraphRevision>,
    requested: Vec<PortId>,
    entries: Vec<(PortId, Option<PortConnectionSummary>)>,
}

impl OpenGpuiPresentationPortCache {
    fn refresh(
        &mut self,
        view: &ViewSession,
        revision: GraphRevision,
        requested: &[PortId],
    ) -> Result<bool, OpenGpuiPresentationSyncError> {
        if self.revision == Some(revision) && self.requested == requested {
            return Ok(false);
        }
        let batch = view.port_connection_summaries(requested)?.into_outcome();
        if batch.revision() != revision {
            return Err(OpenGpuiPresentationSyncError::IncoherentRevision {
                graph_revision: revision,
                summary_revision: batch.revision(),
            });
        }
        let entries = batch.into_entries();
        if let Some((index, (actual, _))) = entries
            .iter()
            .enumerate()
            .find(|(index, (actual, _))| requested.get(*index) != Some(actual))
        {
            return Err(OpenGpuiPresentationSyncError::IncoherentOrder {
                index,
                expected: requested[index],
                actual: *actual,
            });
        }
        self.revision = Some(revision);
        self.requested.clear();
        self.requested.extend_from_slice(requested);
        self.entries = entries;
        Ok(true)
    }

    fn entries(&self) -> &[(PortId, Option<PortConnectionSummary>)] {
        &self.entries
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum OpenGpuiPresentationSyncError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error(
        "port-summary revision {summary_revision} does not match graph revision {graph_revision}"
    )]
    IncoherentRevision {
        graph_revision: GraphRevision,
        summary_revision: GraphRevision,
    },
    #[error("port-summary order diverged at {index}: expected {expected:?}, received {actual:?}")]
    IncoherentOrder {
        index: usize,
        expected: PortId,
        actual: PortId,
    },
    #[error("port-summary batch is missing graph port {port:?}")]
    MissingPort { port: PortId },
    #[error("presentation binding references missing graph node {node:?}")]
    MissingNode { node: NodeId },
}

struct OpenGpuiBindingPresentationUpdate {
    binding: open_gpui::Entity<crate::binding::OpenGpuiNodeBinding>,
    input: OpenGpuiNodePresentationInputSignature,
    presentation: Option<crate::presentation::OpenGpuiNodePresentationSnapshot>,
}

impl OpenGpuiSceneHost {
    pub(super) fn sync_binding_presentations(
        &mut self,
        presentation_offsets: &BTreeMap<NodeId, CanvasPoint>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), OpenGpuiPresentationSyncError> {
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

        let requested_ports = self
            .mounts
            .active_in_order()
            .filter_map(|(_, mounted)| mounted.binding.as_ref())
            .flat_map(|binding| binding.read(cx).port_ids().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        if self.presentation_ports.refresh(
            &self.view,
            graph.revision(),
            requested_ports.as_slice(),
        )? {
            self.work_ledger.binding_presentation_queries = self
                .work_ledger
                .binding_presentation_queries
                .saturating_add(1);
        }

        let interaction = self.interaction_owner.presentation();
        let connection = interaction.connection();
        let keyboard_source = self.interaction_owner.keyboard_connection_source();
        let keyboard_feedback = self
            .interaction_owner
            .keyboard_connection_feedback()
            .cloned();
        let node_drag = interaction.node_drag();
        let projected_interaction = projected_interaction_input(&interaction);
        let summaries = self.presentation_ports.entries();
        let mut summary_cursor = 0_usize;
        let mut materializations = 0_u64;
        let mut updates = Vec::new();

        for (instance, mounted) in self.mounts.active_in_order() {
            let Some(binding) = mounted.binding.clone() else {
                continue;
            };
            let node_focused = mounted.focus.contains_focused(window, cx);
            let binding_state = binding.read(cx);
            let node = graph.graph().node(&instance.node()).ok_or(
                OpenGpuiPresentationSyncError::MissingNode {
                    node: instance.node(),
                },
            )?;
            let lod = match mounted.graph_presentation() {
                OpenGpuiNodePresentation::Detail => OpenGpuiNodeLod::Detail,
                OpenGpuiNodePresentation::Overview if mounted.has_interaction_island() => {
                    OpenGpuiNodeLod::InteractionIsland
                }
                OpenGpuiNodePresentation::Overview => OpenGpuiNodeLod::Overview,
            };
            let dragging = presentation_offsets.contains_key(&instance.node());
            let node_pending = dragging && node_drag.is_some_and(|drag| drag.ticket().is_some());
            let node_state = OpenGpuiNodeInteractionState::idle()
                .with_selected(mounted.selected)
                .with_hovered(self.hovered_node == Some(instance.node()))
                .with_focused(node_focused)
                .with_dragging(dragging)
                .with_disabled(!node.selectable.unwrap_or(true))
                .with_lod(lod)
                .with_settlement(if node_pending {
                    OpenGpuiSettlementState::Pending
                } else {
                    OpenGpuiSettlementState::Idle
                });

            let port_ids = binding_state.port_ids().collect::<Vec<_>>();
            let mut port_inputs = Vec::with_capacity(port_ids.len());
            let mut interaction_relevant = dragging;
            for port in port_ids {
                let (requested, summary) = summaries
                    .get(summary_cursor)
                    .copied()
                    .ok_or(OpenGpuiPresentationSyncError::MissingPort { port })?;
                summary_cursor = summary_cursor.saturating_add(1);
                if requested != port {
                    return Err(OpenGpuiPresentationSyncError::IncoherentOrder {
                        index: summary_cursor - 1,
                        expected: port,
                        actual: requested,
                    });
                }
                let summary = summary.ok_or(OpenGpuiPresentationSyncError::MissingPort { port })?;
                let (candidate, pending) = port_candidate_input(
                    port,
                    connection,
                    keyboard_source,
                    keyboard_feedback.as_ref(),
                    &projected_interaction,
                );
                interaction_relevant |= !candidate.is_idle()
                    || interaction
                        .connection_intent()
                        .is_some_and(|binding| binding_owns_port(&binding, port));
                port_inputs.push(OpenGpuiPortPresentationInput::new(
                    port,
                    summary,
                    interaction.phase() == CanvasInteractionPhase::Idle
                        && self.hovered_port == Some(port),
                    binding_state
                        .port_focus_handle(port)
                        .is_some_and(|focus| focus.is_focused(window)),
                    candidate,
                    pending,
                ));
            }

            let binding_interaction = if interaction_relevant {
                projected_interaction.clone()
            } else {
                OpenGpuiProjectedInteractionInput::default()
            };
            let input = OpenGpuiNodePresentationInputSignature::new(
                graph.revision(),
                binding_state.facts_revision(),
                node_state,
                binding_interaction,
                port_inputs,
            );
            let identical = binding_state.presentation_input() == Some(&input);
            let semantically_equal = binding_state
                .presentation_input()
                .is_some_and(|current| current.semantically_eq(&input));
            if identical {
                continue;
            }
            let presentation = (!semantically_equal).then(|| {
                materializations = materializations.saturating_add(1);
                input.materialize()
            });
            updates.push(OpenGpuiBindingPresentationUpdate {
                binding,
                input,
                presentation,
            });
        }
        debug_assert_eq!(summary_cursor, summaries.len());
        self.work_ledger.binding_presentation_materializations = self
            .work_ledger
            .binding_presentation_materializations
            .saturating_add(materializations);

        for update in updates {
            let mut changed = false;
            update.binding.update(cx, |binding, cx| {
                binding.set_presentation_input(update.input);
                if let Some(presentation) = update.presentation {
                    changed = binding.set_presentation(presentation);
                    if changed {
                        cx.notify();
                    }
                }
            });
            if changed {
                self.work_ledger.binding_updates =
                    self.work_ledger.binding_updates.saturating_add(1);
                self.work_ledger.binding_notifications =
                    self.work_ledger.binding_notifications.saturating_add(1);
            }
        }
        Ok(())
    }
}

fn binding_owns_port(
    binding: &jellyflow_runtime::interaction::ConnectionGestureBinding,
    port: PortId,
) -> bool {
    binding.fixed_port_instance().port() == port
        || binding
            .detached_port_instance()
            .is_some_and(|detached| detached.port() == port)
}

fn port_candidate_input(
    port: PortId,
    connection: Option<&jellyflow_runtime::interaction::ConnectionPresentation>,
    keyboard_source: Option<KeyboardConnectionPort>,
    keyboard_feedback: Option<&KeyboardConnectionFeedback>,
    projected: &OpenGpuiProjectedInteractionInput,
) -> (OpenGpuiPortCandidateInput, bool) {
    let Some(connection) = connection else {
        if keyboard_source.is_some_and(|source| source.port() == port) {
            return (
                OpenGpuiPortCandidateInput::Source,
                projected.phase() == CanvasInteractionPhase::Committing,
            );
        }
        if let Some(feedback) = keyboard_feedback.filter(|feedback| feedback.target() == port) {
            return (
                OpenGpuiPortCandidateInput::invalid(feedback.diagnostics()),
                false,
            );
        }
        return (OpenGpuiPortCandidateInput::Idle, false);
    };
    let binding = connection.binding();
    let source = binding
        .detached_port_instance()
        .map(|detached| detached.port())
        .unwrap_or_else(|| binding.fixed_port_instance().port());
    if source == port {
        return (
            OpenGpuiPortCandidateInput::Source,
            connection.ticket().is_some(),
        );
    }
    if connection.probe().target().map(|target| target.port()) != Some(port) {
        return (OpenGpuiPortCandidateInput::Idle, false);
    }
    let candidate = projected
        .candidate_for_port(port)
        .unwrap_or(OpenGpuiPortCandidateInput::Idle);
    (candidate, connection.ticket().is_some())
}

fn projected_interaction_input(
    interaction: &CanvasInteractionPresentation,
) -> OpenGpuiProjectedInteractionInput {
    let connection = interaction.connection();
    let candidate = connection.and_then(|connection| {
        let target = connection.probe().target()?;
        let candidate = match connection.probe() {
            ConnectionProbe::None => OpenGpuiPortCandidateInput::Idle,
            ConnectionProbe::Deferred { .. } | ConnectionProbe::Valid { .. } => {
                OpenGpuiPortCandidateInput::ValidTarget
            }
            ConnectionProbe::Invalid { diagnostics, .. } => {
                OpenGpuiPortCandidateInput::invalid(diagnostics)
            }
        };
        Some(OpenGpuiConnectionCandidateInput::new(
            target.port(),
            connection.probe().revision(),
            candidate,
        ))
    });
    OpenGpuiProjectedInteractionInput::new(
        interaction.phase(),
        interaction.generation(),
        interaction.connection_intent(),
        connection
            .and_then(|connection| connection.ticket())
            .or_else(|| interaction.node_drag().and_then(|drag| drag.ticket())),
        candidate,
    )
}
