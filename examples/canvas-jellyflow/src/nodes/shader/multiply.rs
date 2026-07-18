use jellyflow::core::types::TypeDesc;
use jellyflow::core::{
    CanvasSize, Node, NodeId, NodeKindKey, PortCapacity, PortDirection, PortId, PortKind,
};
use jellyflow::runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow::runtime::{GraphCommand, GraphCommandOutcome, GraphRevision, NodeInstanceKey};
use jellyflow_open_gpui::{
    DraftAttempt, DraftConflictResolution, DraftIntentRejection, DraftPhase, DraftReceiptDecision,
    DraftReconcileDecision, DraftRecovery, DraftResolutionDecision, DraftSubmissionDecision,
    DraftSubmissionIntent, ExternalizedDraftStore, OpenGpuiActivityAcquireError,
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiCommandTicketError,
    OpenGpuiCommandTicketResult, OpenGpuiNodeActivityRegistry, OpenGpuiNodeBinding,
    OpenGpuiNodeBindingRead, OpenGpuiNodeRegistryBuilder, RevisionedDraft,
};
use open_gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Task, Window, div, px, rgb,
};
use open_gpui_ui_components::prelude::Sizable;
use open_gpui_ui_components::{Button, ButtonVariant, NumberInput};

use super::super::sync_activity_lease;
use super::{ShaderNodeError, ShaderPort, ShaderPortElement, port_element};

pub const SHADER_MULTIPLY_KIND: &str = "shader.multiply";
pub const SHADER_MULTIPLY_RENDERER: &str = "jellyflow.open-gpui.shader.multiply";
pub const SHADER_MULTIPLY_DEFAULT_SIZE: CanvasSize = CanvasSize {
    width: 260.0,
    height: 160.0,
};
pub const SHADER_NODE_SELECTOR: &str = "jellyflow-native-shader-multiply";
pub const SHADER_PREVIEW_CONTROL_SELECTOR: &str = "jellyflow-native-shader-preview-control";
pub const SHADER_PREVIEW_VALUE_SELECTOR: &str = "jellyflow-native-shader-preview-value";
pub const SHADER_FACTOR_COMMAND_PENDING_SELECTOR: &str =
    "jellyflow-native-shader-factor-command-pending";
pub const SHADER_FACTOR_COMMAND_DIAGNOSTIC_SELECTOR: &str =
    "jellyflow-native-shader-factor-command-diagnostic";
pub const SHADER_FACTOR_RETRY_SELECTOR: &str = "jellyflow-native-shader-factor-retry";
pub const SHADER_FACTOR_FIELD: &str = "factor";

pub type ShaderDraftStore = ExternalizedDraftStore<f32>;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ShaderFactorIntent {
    attempt: DraftAttempt,
    expected_revision: GraphRevision,
    value: f32,
}

impl From<&DraftSubmissionIntent<f32>> for ShaderFactorIntent {
    fn from(intent: &DraftSubmissionIntent<f32>) -> Self {
        Self {
            attempt: intent.attempt(),
            expected_revision: intent.expected_revision(),
            value: *intent.value(),
        }
    }
}

#[derive(Debug, Default)]
enum ShaderFactorCommandState {
    #[default]
    Idle,
    InFlight(ShaderFactorIntent),
    AwaitingSnapshot {
        intent: ShaderFactorIntent,
        committed_revision: GraphRevision,
    },
    NeedsRefresh {
        intent: ShaderFactorIntent,
        minimum_revision: GraphRevision,
    },
    ReadyToRetry(ShaderFactorIntent),
}

impl ShaderFactorCommandState {
    fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    fn is_waiting(&self) -> bool {
        matches!(
            self,
            Self::InFlight(_) | Self::AwaitingSnapshot { .. } | Self::NeedsRefresh { .. }
        )
    }

    fn retry_intent(&self) -> Option<ShaderFactorIntent> {
        match self {
            Self::ReadyToRetry(intent) => Some(*intent),
            _ => None,
        }
    }
}

/// One retained, framework-native shader node mounted by [`OpenGpuiSceneHost`].
///
/// [`OpenGpuiSceneHost`]: jellyflow_open_gpui::OpenGpuiSceneHost
pub struct ShaderMultiplyNode {
    binding: Entity<OpenGpuiNodeBinding>,
    activity: OpenGpuiNodeActivityRegistry,
    draft_store: ShaderDraftStore,
    factor: RevisionedDraft<f32>,
    draft_lease: Option<OpenGpuiActivityLease>,
    draft_capacity_overflowed: bool,
    preview_expanded: bool,
    factor_command: ShaderFactorCommandState,
    factor_command_task: Option<Task<()>>,
    diagnostic: Option<String>,
}

impl ShaderMultiplyNode {
    fn new(
        binding: Entity<OpenGpuiNodeBinding>,
        activity: OpenGpuiNodeActivityRegistry,
        draft_store: ShaderDraftStore,
        factor: RevisionedDraft<f32>,
        recovery: DraftRecovery,
    ) -> Self {
        let mut details = Vec::new();
        if let Some(attempt) = recovery.interrupted_attempt() {
            details.push(format!("factor#{}", attempt.get()));
        }
        if recovery.composition_interrupted() {
            details.push("factor:composition".to_owned());
        }
        let diagnostic = (!details.is_empty()).then(|| {
            format!(
                "Recovered interrupted shader draft state ({}); explicit resubmission or composition is required",
                details.join(", ")
            )
        });
        Self {
            binding,
            activity,
            draft_store,
            factor,
            draft_lease: None,
            draft_capacity_overflowed: false,
            preview_expanded: false,
            factor_command: ShaderFactorCommandState::Idle,
            factor_command_task: None,
            diagnostic,
        }
    }

    fn edit_factor(&mut self, value: f32, cx: &mut Context<Self>) {
        self.factor.edit_local(value);
        if self.factor_command.is_idle()
            && let Err(error) = self.submit_dirty_factor(cx)
        {
            self.diagnostic = Some(error.to_string());
        }
        cx.notify();
    }

    fn accept_remote_factor(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.factor_command,
            ShaderFactorCommandState::Idle | ShaderFactorCommandState::ReadyToRetry(_)
        ) {
            self.diagnostic = Some("shader factor command is still pending".to_owned());
            cx.notify();
            return;
        }
        let result = read_shader_binding(&self.binding, cx).and_then(|(facts, remote)| {
            let revision = facts.graph_revision();
            self.factor.reconcile_remote(revision, remote);
            match self
                .factor
                .resolve_at(DraftConflictResolution::AcceptRemote, revision)
            {
                DraftResolutionDecision::AcceptedRemote { .. } => Ok(()),
                _ => Err(ShaderNodeError::DraftConflict),
            }
        });
        if result.is_ok() {
            self.factor_command = ShaderFactorCommandState::Idle;
        }
        self.diagnostic = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn reapply_local_factor(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.factor_command,
            ShaderFactorCommandState::Idle | ShaderFactorCommandState::ReadyToRetry(_)
        ) {
            self.diagnostic = Some("shader factor command is still pending".to_owned());
            cx.notify();
            return;
        }
        let saved_intent = self.factor_command.retry_intent();
        let result = self.retry_factor(saved_intent, cx);
        if let Err(error) = result {
            if let Some(intent) = saved_intent {
                self.factor_command = ShaderFactorCommandState::ReadyToRetry(intent);
            }
            self.diagnostic = Some(error.to_string());
        }
        cx.notify();
    }

    fn submit_dirty_factor(&mut self, cx: &mut Context<Self>) -> Result<(), ShaderNodeError> {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let (facts, remote) = read_shader_binding(&self.binding, cx)?;
        self.factor.reconcile_remote(facts.graph_revision(), remote);
        let intent = match self.factor.plan_submission_at(facts.graph_revision()) {
            DraftSubmissionDecision::Submit(intent) => intent,
            DraftSubmissionDecision::Rejected(DraftIntentRejection::Clean) => return Ok(()),
            DraftSubmissionDecision::Rejected(_) => return Err(ShaderNodeError::DraftConflict),
        };
        self.enqueue_factor_intent(facts, node_id, intent, cx)
    }

    fn finish_factor_acknowledgement(
        &mut self,
        local_changed_after_submission: bool,
        cx: &mut Context<Self>,
    ) {
        self.factor_command = ShaderFactorCommandState::Idle;
        self.diagnostic = None;
        if !local_changed_after_submission {
            return;
        }

        let node = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = node.update(cx, |node, cx| {
                if node.factor_command.is_idle()
                    && let Err(error) = node.submit_dirty_factor(cx)
                {
                    node.diagnostic = Some(error.to_string());
                }
                cx.notify();
            });
        });
    }

    fn retry_factor(
        &mut self,
        saved_intent: Option<ShaderFactorIntent>,
        cx: &mut Context<Self>,
    ) -> Result<(), ShaderNodeError> {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let (facts, remote) = read_shader_binding(&self.binding, cx)?;
        let revision = facts.graph_revision();
        self.factor.reconcile_remote(revision, remote);
        if let Some(saved_intent) = saved_intent {
            self.factor.edit_local(saved_intent.value);
        }
        let intent = if self.factor.conflict().is_some() {
            match self
                .factor
                .resolve_at(DraftConflictResolution::ReapplyLocal, revision)
            {
                DraftResolutionDecision::Reapply(intent) => intent,
                _ => return Err(ShaderNodeError::DraftConflict),
            }
        } else {
            match self.factor.plan_submission_at(revision) {
                DraftSubmissionDecision::Submit(intent) => intent,
                DraftSubmissionDecision::Rejected(DraftIntentRejection::Clean) => {
                    self.factor_command = ShaderFactorCommandState::Idle;
                    self.diagnostic = None;
                    return Ok(());
                }
                DraftSubmissionDecision::Rejected(_) => {
                    return Err(ShaderNodeError::DraftConflict);
                }
            }
        };
        self.enqueue_factor_intent(facts, node_id, intent, cx)
    }

    fn enqueue_factor_intent(
        &mut self,
        facts: OpenGpuiNodeBindingRead,
        node_id: NodeId,
        intent: DraftSubmissionIntent<f32>,
        cx: &mut Context<Self>,
    ) -> Result<(), ShaderNodeError> {
        let retained_intent = ShaderFactorIntent::from(&intent);
        let next_data = match shader_data_with_factor(node_id, facts.node(), retained_intent.value)
        {
            Ok(next_data) => next_data,
            Err(error) => {
                self.factor.reject_submission(retained_intent.attempt);
                self.factor_command = ShaderFactorCommandState::ReadyToRetry(retained_intent);
                return Err(error);
            }
        };
        let ticket = match facts.enqueue_command(cx, move |_| {
            GraphCommand::set_node_data(retained_intent.expected_revision, node_id, next_data)
                .with_label("edit shader factor")
        }) {
            Ok(ticket) => ticket,
            Err(error) => {
                self.factor.reject_submission(retained_intent.attempt);
                self.factor_command = ShaderFactorCommandState::ReadyToRetry(retained_intent);
                return Err(ShaderNodeError::BindingUnavailable {
                    message: error.to_string(),
                });
            }
        };
        self.factor_command = ShaderFactorCommandState::InFlight(retained_intent);
        self.diagnostic = None;
        self.factor_command_task = Some(cx.spawn(async move |node, cx| {
            let result = ticket.await;
            let _ = node.update(cx, |node, cx| {
                node.finish_factor_ticket(retained_intent, result, cx)
            });
        }));
        Ok(())
    }

    fn finish_factor_ticket(
        &mut self,
        intent: ShaderFactorIntent,
        result: OpenGpuiCommandTicketResult,
        cx: &mut Context<Self>,
    ) {
        self.factor_command_task = None;
        if !matches!(
            self.factor_command,
            ShaderFactorCommandState::InFlight(active) if active.attempt == intent.attempt
        ) {
            return;
        }

        match result {
            Ok(GraphCommandOutcome::Applied(receipt)) => {
                match self
                    .factor
                    .record_self_receipt(intent.attempt, receipt.revision())
                {
                    DraftReceiptDecision::Acknowledged {
                        local_changed_after_submission,
                        ..
                    } => self.finish_factor_acknowledgement(local_changed_after_submission, cx),
                    DraftReceiptDecision::Recorded {
                        committed_revision, ..
                    }
                    | DraftReceiptDecision::AlreadyRecorded {
                        committed_revision, ..
                    } => {
                        self.factor_command = ShaderFactorCommandState::AwaitingSnapshot {
                            intent,
                            committed_revision,
                        };
                        self.diagnostic = Some(
                            "shader factor command applied; waiting for authoritative snapshot"
                                .to_owned(),
                        );
                    }
                    DraftReceiptDecision::Superseded {
                        current_revision, ..
                    } => {
                        self.factor_command = ShaderFactorCommandState::NeedsRefresh {
                            intent,
                            minimum_revision: current_revision,
                        };
                        self.diagnostic = Some(
                            "shader factor receipt was superseded; resynchronizing before retry"
                                .to_owned(),
                        );
                    }
                    DraftReceiptDecision::UnknownAttempt { .. }
                    | DraftReceiptDecision::InvalidCommittedRevision { .. } => {
                        self.factor_command = ShaderFactorCommandState::NeedsRefresh {
                            intent,
                            minimum_revision: receipt.revision(),
                        };
                        self.diagnostic = Some(
                            "shader factor receipt did not match the retained attempt; resynchronizing"
                                .to_owned(),
                        );
                    }
                }
            }
            Ok(GraphCommandOutcome::Noop(noop)) => {
                self.factor_command = ShaderFactorCommandState::NeedsRefresh {
                    intent,
                    minimum_revision: noop.revision(),
                };
                self.diagnostic =
                    Some("shader factor command made no graph change; resynchronizing".to_owned());
            }
            Ok(GraphCommandOutcome::Conflict(conflict)) => {
                self.factor_command = ShaderFactorCommandState::NeedsRefresh {
                    intent,
                    minimum_revision: conflict.current().revision(),
                };
                self.diagnostic = Some(format!(
                    "shader factor revision conflict (expected {}, current {}); refresh required before retry",
                    conflict.expected_revision(),
                    conflict.current().revision(),
                ));
            }
            Ok(GraphCommandOutcome::Rejected(rejection)) => {
                self.factor_command = ShaderFactorCommandState::NeedsRefresh {
                    intent,
                    minimum_revision: intent.expected_revision,
                };
                self.diagnostic = Some(format!(
                    "shader factor command was rejected: {}",
                    rejection.message()
                ));
            }
            Err(error) => {
                let (state, diagnostic) = shader_factor_ticket_failure(intent, error);
                self.factor_command = state;
                self.diagnostic = Some(diagnostic);
            }
        }
        cx.notify();
    }

    fn reconcile_factor_snapshot(
        &mut self,
        revision: GraphRevision,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        let decision = self.factor.reconcile_remote(revision, value);
        let state = std::mem::take(&mut self.factor_command);
        let mut acknowledged_local_change = None;
        self.factor_command = match state {
            ShaderFactorCommandState::AwaitingSnapshot {
                intent,
                committed_revision: _,
            } if matches!(
                decision,
                DraftReconcileDecision::SelfAcknowledged { attempt, .. }
                    if attempt == intent.attempt
            ) =>
            {
                let DraftReconcileDecision::SelfAcknowledged {
                    local_changed_after_submission,
                    ..
                } = decision
                else {
                    unreachable!("guard requires a matching acknowledgement");
                };
                acknowledged_local_change = Some(local_changed_after_submission);
                ShaderFactorCommandState::Idle
            }
            ShaderFactorCommandState::AwaitingSnapshot {
                intent,
                committed_revision,
            } if revision > committed_revision
                && self.factor.pending_attempt() != Some(intent.attempt) =>
            {
                self.diagnostic = Some(
                    "shader factor snapshot superseded the accepted receipt; explicit retry required"
                        .to_owned(),
                );
                ShaderFactorCommandState::ReadyToRetry(intent)
            }
            ShaderFactorCommandState::NeedsRefresh {
                intent,
                minimum_revision,
            } if revision >= minimum_revision => {
                if value == intent.value {
                    self.factor.reject_submission(intent.attempt);
                    self.factor.reconcile_remote(revision, value);
                    self.diagnostic = None;
                    ShaderFactorCommandState::Idle
                } else {
                    self.factor.reject_stale(intent.attempt, revision, value);
                    ShaderFactorCommandState::ReadyToRetry(intent)
                }
            }
            state => state,
        };
        if let Some(local_changed_after_submission) = acknowledged_local_change {
            self.finish_factor_acknowledgement(local_changed_after_submission, cx);
        }
    }

    fn sync_draft_activity(&mut self, cx: &mut Context<Self>) {
        let active =
            self.factor.phase() != DraftPhase::Clean || self.factor.pending_attempt().is_some();
        let result = sync_activity_lease(
            &self.activity,
            &mut self.draft_lease,
            OpenGpuiActivityKind::Draft,
            active,
        );
        if let Err(error) = result {
            if matches!(
                error,
                OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { .. }
            ) && active
            {
                self.externalize_overflow_draft(cx);
            }
            self.diagnostic = Some(format!("shader activity lease failed: {error}"));
        } else if self.draft_capacity_overflowed {
            let instance = self.binding.read(cx).snapshot().instance().clone();
            self.draft_store
                .discard_externalized(&instance, SHADER_FACTOR_FIELD);
            self.draft_capacity_overflowed = false;
        }
    }

    fn externalize_overflow_draft(&mut self, cx: &mut Context<Self>) {
        let instance = self.binding.read(cx).snapshot().instance().clone();
        self.draft_store
            .externalize_overflow(&instance, SHADER_FACTOR_FIELD, &self.factor);
        self.draft_capacity_overflowed = true;
    }
}

fn shader_factor_ticket_failure(
    intent: ShaderFactorIntent,
    error: OpenGpuiCommandTicketError,
) -> (ShaderFactorCommandState, String) {
    (
        ShaderFactorCommandState::NeedsRefresh {
            intent,
            minimum_revision: intent.expected_revision,
        },
        format!("shader factor command result is uncertain; resynchronizing before retry: {error}"),
    )
}

impl Render for ShaderMultiplyNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let drag_handle = self.binding.read(cx).drag_handle();
        let (
            node_id,
            instance_scope,
            graph_revision,
            title,
            factor,
            preview,
            input_a_connected,
            ports,
        ) = {
            let binding = self.binding.read(cx);
            let node_id = binding.snapshot().node_id();
            let instance_scope = format!("{:?}", binding.snapshot().instance());
            let facts = binding.snapshot().read().and_then(|facts| {
                let data = &facts.node().data;
                let title = data
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Multiply")
                    .to_owned();
                let factor = finite_f32(data.get(SHADER_FACTOR_FIELD), 1.0);
                let preview = finite_f32(data.get("preview"), factor);
                let input_a_connected = facts
                    .ports()
                    .find(|port| port.port().dir == PortDirection::In && port.port().key.0 == "a")
                    .and_then(|port| facts.port_connection_facts(port.id()).ok().flatten())
                    .is_some_and(|connection| !connection.incident().is_empty());
                let ports = facts
                    .ports()
                    .zip(binding.port_handles(&facts)?)
                    .map(|(port, handle)| {
                        let key = port.port().key.clone();
                        let label = match key.0.as_str() {
                            "a" => "A".into(),
                            "b" => "B".into(),
                            "result" => "Result".into(),
                            _ => key.0.clone().into(),
                        };
                        ShaderPort {
                            id: port.id(),
                            key,
                            direction: port.port().dir,
                            label,
                            handle,
                        }
                    })
                    .collect();
                Ok((
                    facts.graph_revision(),
                    title,
                    factor,
                    preview,
                    input_a_connected,
                    ports,
                ))
            });
            match facts {
                Ok((revision, title, factor, preview, input_a_connected, ports)) => (
                    node_id,
                    instance_scope,
                    Some(revision),
                    title,
                    factor,
                    preview,
                    input_a_connected,
                    ports,
                ),
                Err(_) => (
                    node_id,
                    instance_scope,
                    None,
                    "Retired shader node".to_owned(),
                    *self.factor.local(),
                    1.0,
                    false,
                    Vec::new(),
                ),
            }
        };
        if let Some(revision) = graph_revision {
            self.reconcile_factor_snapshot(revision, factor, cx);
        }
        self.sync_draft_activity(cx);
        let inline_factor = *self.factor.local();
        let factor_input_disabled =
            !self.factor_command.is_idle() || self.factor.conflict().is_some();
        let conflict_resolution_disabled = !matches!(
            self.factor_command,
            ShaderFactorCommandState::Idle | ShaderFactorCommandState::ReadyToRetry(_)
        );

        let mut input_a = None;
        let mut input_b = None;
        let mut output = None;
        for port in ports {
            let ShaderPortElement { key, marker, .. } = port_element(port, shader_port_selector);
            match key.0.as_str() {
                "a" => input_a = Some(marker),
                "b" => input_b = Some(marker),
                "result" => output = Some(marker),
                _ => {}
            }
        }

        let node = cx.weak_entity();
        let preview_button = Button::new(
            format!("jellyflow-shader-preview-{instance_scope}"),
            "Preview",
        )
        .variant(ButtonVariant::Ghost)
        .xsmall()
        .selected(self.preview_expanded)
        .on_click(move |_, _, cx| {
            let _ = node.update(cx, |node, cx| {
                node.preview_expanded = !node.preview_expanded;
                cx.notify();
            });
        });
        let preview_control = div()
            .debug_selector(|| SHADER_PREVIEW_CONTROL_SELECTOR.to_owned())
            .child(preview_button);

        let input_a_label = if input_a_connected {
            let selector = shader_connected_input_selector(node_id, "a");
            div()
                .debug_selector(move || selector)
                .text_sm()
                .text_color(rgb(0xdbe3ec))
                .child("A")
        } else {
            div().text_sm().text_color(rgb(0xdbe3ec)).child("A")
        };
        let factor_editor = (!input_a_connected).then(|| {
            let editor_selector = shader_inline_editor_selector(node_id, "a");
            let value_selector = shader_inline_value_selector(node_id, "a", inline_factor);
            let inline_node = cx.weak_entity();
            div()
                .debug_selector(move || editor_selector)
                .w(px(92.0))
                .child(
                    div().debug_selector(move || value_selector).child(
                        NumberInput::new(
                            format!("jellyflow-shader-factor-{instance_scope}"),
                            "Factor",
                        )
                        .value(inline_factor)
                        .min(-16.0)
                        .max(16.0)
                        .step(0.05)
                        .disabled(factor_input_disabled)
                        .xsmall()
                        .on_change(move |change, _, cx| {
                            let _ = inline_node.update(cx, |node, cx| {
                                node.edit_factor(change.value(), cx);
                            });
                        }),
                    ),
                )
        });

        let mut input_a_row = div()
            .relative()
            .h(px(40.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .children(
                input_a.map(|marker| div().absolute().left(px(-6.0)).top(px(14.0)).child(marker)),
            )
            .child(input_a_label)
            .child(div().flex_1())
            .children(factor_editor);
        if let Some(marker) = output {
            input_a_row = input_a_row
                .child(div().text_sm().text_color(rgb(0xdbe3ec)).child("Result"))
                .child(div().absolute().right(px(-6.0)).top(px(14.0)).child(marker));
        }
        let input_b_row = div()
            .relative()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .children(
                input_b.map(|marker| div().absolute().left(px(-6.0)).top(px(10.0)).child(marker)),
            )
            .child(div().text_sm().text_color(rgb(0xdbe3ec)).child("B"));

        let status_content = if self.factor.conflict().is_some() {
            let accept_node = cx.weak_entity();
            let reapply_node = cx.weak_entity();
            let accept_selector = shader_factor_accept_remote_selector(node_id);
            let reapply_selector = shader_factor_reapply_selector(node_id);
            Some(
                div()
                    .debug_selector(move || shader_factor_conflict_selector(node_id))
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(0x9d654d))
                    .bg(rgb(0x382a28))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(0xf0c3aa))
                            .child("Factor conflict"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                div().debug_selector(move || accept_selector.clone()).child(
                                    Button::new(
                                        format!("jellyflow-shader-factor-accept-{instance_scope}"),
                                        "Use remote",
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .xsmall()
                                    .disabled(conflict_resolution_disabled)
                                    .on_click(
                                        move |_, window, cx| {
                                            let _ = accept_node.update(cx, |node, cx| {
                                                node.accept_remote_factor(cx)
                                            });
                                            window.refresh();
                                        },
                                    ),
                                ),
                            )
                            .child(
                                div()
                                    .debug_selector(move || reapply_selector.clone())
                                    .child(
                                        Button::new(
                                            format!(
                                                "jellyflow-shader-factor-reapply-{instance_scope}"
                                            ),
                                            "Reapply",
                                        )
                                        .variant(ButtonVariant::Ghost)
                                        .xsmall()
                                        .disabled(conflict_resolution_disabled)
                                        .on_click(
                                            move |_, window, cx| {
                                                let _ = reapply_node.update(cx, |node, cx| {
                                                    node.reapply_local_factor(cx)
                                                });
                                                window.refresh();
                                            },
                                        ),
                                    ),
                            ),
                    )
                    .into_any_element(),
            )
        } else if self.factor_command.retry_intent().is_some() {
            let retry_node = cx.weak_entity();
            let message = self
                .diagnostic
                .clone()
                .unwrap_or_else(|| "Factor edit needs retry".to_owned());
            Some(
                div()
                    .debug_selector(|| SHADER_FACTOR_RETRY_SELECTOR.to_owned())
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(4.0))
                    .bg(rgb(0x3a2026))
                    .text_color(rgb(0xffa5ad))
                    .child(
                        div()
                            .debug_selector(|| SHADER_FACTOR_COMMAND_DIAGNOSTIC_SELECTOR.to_owned())
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .child(message),
                    )
                    .child(
                        Button::new(
                            format!("jellyflow-shader-factor-retry-{instance_scope}"),
                            "Retry",
                        )
                        .variant(ButtonVariant::Ghost)
                        .xsmall()
                        .on_click(move |_, window, cx| {
                            let _ = retry_node.update(cx, |node, cx| node.reapply_local_factor(cx));
                            window.refresh();
                        }),
                    )
                    .into_any_element(),
            )
        } else if let Some(message) = self.diagnostic.clone() {
            Some(
                div()
                    .debug_selector(|| SHADER_FACTOR_COMMAND_DIAGNOSTIC_SELECTOR.to_owned())
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(rgb(0x3a2026))
                    .text_color(rgb(0xffa5ad))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .child(message),
                    )
                    .into_any_element(),
            )
        } else if self.factor_command.is_waiting() {
            Some(
                div()
                    .debug_selector(|| SHADER_FACTOR_COMMAND_PENDING_SELECTOR.to_owned())
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(rgb(0x27364b))
                    .text_xs()
                    .text_color(rgb(0x9fc4ff))
                    .child("Saving factor...")
                    .into_any_element(),
            )
        } else if self.preview_expanded {
            Some(
                div()
                    .debug_selector(|| SHADER_PREVIEW_VALUE_SELECTOR.to_owned())
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(0x34404a))
                    .text_xs()
                    .text_color(rgb(0x9aa7b5))
                    .child("Output")
                    .child(
                        div()
                            .text_color(rgb(0x7fd5ac))
                            .child(format!("{preview:.2}")),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        let status = div().h(px(40.0)).px(px(12.0)).children(status_content);
        let body = div()
            .py(px(4.0))
            .flex()
            .flex_col()
            .child(input_a_row)
            .child(input_b_row)
            .child(status);

        div()
            .debug_selector(|| SHADER_NODE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(0x3b4450))
            .bg(rgb(0x20262d))
            .text_color(rgb(0xf4f7fb))
            .child(
                drag_handle.wrap(
                    div()
                        .h(px(40.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(rgb(0x3b4450))
                        .bg(rgb(0x252b33))
                        .child(div().text_sm().child(title))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .px(px(6.0))
                                        .py(px(3.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(0x303944))
                                        .text_xs()
                                        .text_color(rgb(0xaab4c0))
                                        .child("FLOAT"),
                                )
                                .child(preview_control),
                        ),
                ),
            )
            .child(body)
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(SHADER_MULTIPLY_KIND, "Multiply", SHADER_MULTIPLY_RENDERER)
        .category(["Shader", "Math"])
        .keywords(["multiply", "float", "vector"])
        .default_size(SHADER_MULTIPLY_DEFAULT_SIZE)
        .ports([
            NodePortSpec::new("a", PortDirection::In, PortKind::Data, PortCapacity::Single)
                .with_type(TypeDesc::Float)
                .with_label("A"),
            NodePortSpec::new("b", PortDirection::In, PortKind::Data, PortCapacity::Single)
                .with_type(TypeDesc::Float)
                .with_label("B"),
            NodePortSpec::new(
                "result",
                PortDirection::Out,
                PortKind::Data,
                PortCapacity::Multi,
            )
            .with_type(TypeDesc::Float)
            .with_label("Result"),
        ])
        .default_data(serde_json::json!({
            "title": "Multiply",
            "factor": 1.0,
            "preview": 1.0,
        }))
        .build()
}

pub(super) fn register(registry: &mut OpenGpuiNodeRegistryBuilder, draft_store: ShaderDraftStore) {
    registry.register(SHADER_MULTIPLY_RENDERER, move |binding, _window, cx| {
        mount_shader_multiply(binding, draft_store.clone(), cx)
    });
}

fn mount_shader_multiply(
    binding: Entity<OpenGpuiNodeBinding>,
    draft_store: ShaderDraftStore,
    cx: &mut App,
) -> Entity<ShaderMultiplyNode> {
    let activity = binding.read(cx).activity().clone();
    let instance = binding.read(cx).snapshot().instance().clone();
    let (factor, recovery) = match read_shader_binding(&binding, cx) {
        Ok((facts, authoritative)) => restore_shader_factor_draft(
            &draft_store,
            &instance,
            facts.graph_revision(),
            authoritative,
        ),
        Err(_) => (RevisionedDraft::new(0, 1.0), DraftRecovery::default()),
    };
    cx.new(|_| ShaderMultiplyNode::new(binding, activity, draft_store, factor, recovery))
}

fn restore_shader_factor_draft(
    draft_store: &ShaderDraftStore,
    instance: &NodeInstanceKey,
    revision: GraphRevision,
    authoritative: f32,
) -> (RevisionedDraft<f32>, DraftRecovery) {
    let restored = draft_store.restore(instance, SHADER_FACTOR_FIELD, revision, authoritative);
    let recovery = restored.recovery();
    (restored.into_draft(), recovery)
}

#[must_use]
pub fn shader_port_selector(port: PortId) -> String {
    format!("jellyflow-native-shader-port-{port:?}")
}

#[must_use]
pub fn shader_inline_editor_selector(node: NodeId, input_key: &str) -> String {
    format!("jellyflow-native-shader-inline-{node:?}-{input_key}")
}

#[must_use]
pub fn shader_inline_value_selector(node: NodeId, input_key: &str, value: f32) -> String {
    format!("jellyflow-native-shader-inline-value-{node:?}-{input_key}-{value:.2}")
}

#[must_use]
pub fn shader_connected_input_selector(node: NodeId, input_key: &str) -> String {
    format!("jellyflow-native-shader-connected-{node:?}-{input_key}")
}

#[must_use]
pub fn shader_factor_conflict_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-factor-conflict-{node:?}")
}

#[must_use]
pub fn shader_factor_accept_remote_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-factor-accept-{node:?}")
}

#[must_use]
pub fn shader_factor_reapply_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-factor-reapply-{node:?}")
}

fn read_shader_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &mut App,
) -> Result<(OpenGpuiNodeBindingRead, f32), ShaderNodeError> {
    let facts = binding.read(cx).snapshot().read().map_err(|error| {
        ShaderNodeError::BindingUnavailable {
            message: error.to_string(),
        }
    })?;
    let node_id = binding.read(cx).snapshot().node_id();
    ensure_shader_kind(node_id, facts.node())?;
    let factor = finite_f32(facts.node().data.get(SHADER_FACTOR_FIELD), 1.0);
    Ok((facts, factor))
}

fn shader_data_with_factor(
    node_id: NodeId,
    node: &Node,
    factor: f32,
) -> Result<serde_json::Value, ShaderNodeError> {
    ensure_shader_kind(node_id, node)?;
    let mut data = node.data.clone();
    let object = data.as_object_mut().ok_or(ShaderNodeError::InvalidData {
        node: node_id,
        field: "object",
    })?;
    object.insert(SHADER_FACTOR_FIELD.to_owned(), serde_json::json!(factor));
    Ok(data)
}

fn ensure_shader_kind(node_id: NodeId, node: &Node) -> Result<(), ShaderNodeError> {
    if node.kind != NodeKindKey::new(SHADER_MULTIPLY_KIND) {
        return Err(ShaderNodeError::WrongNodeKind {
            node: node_id,
            expected: SHADER_MULTIPLY_KIND,
            actual: node.kind.0.clone(),
        });
    }
    Ok(())
}

fn finite_f32(value: Option<&serde_json::Value>, fallback: f32) -> f32 {
    value
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use jellyflow::core::{CanvasPoint, CanvasRect, GraphBuilder, GraphId};
    use jellyflow::runtime::schema::NodeCatalog;
    use jellyflow::runtime::{GraphRuntime, ViewCloseReason, ViewSessionOptions};
    use jellyflow_open_gpui::{OpenGpuiNodeRegistry, OpenGpuiSceneHost};

    use super::*;

    #[test]
    fn ticket_transport_failures_never_acknowledge_the_factor_draft() {
        let mut draft = RevisionedDraft::new(23, 1.0);
        draft.edit_local(2.0);
        let DraftSubmissionDecision::Submit(intent) = draft.plan_submission_at(23) else {
            panic!("dirty factor must produce an intent");
        };
        let intent = ShaderFactorIntent::from(&intent);
        let failures = [
            OpenGpuiCommandTicketError::HostDropped,
            OpenGpuiCommandTicketError::HostClosed {
                reason: ViewCloseReason::HostClosed,
            },
            OpenGpuiCommandTicketError::MissingReceipt { operation_index: 7 },
            OpenGpuiCommandTicketError::WrongReceiptKind { operation_index: 8 },
        ];

        for failure in failures {
            let (state, diagnostic) = shader_factor_ticket_failure(intent, failure);
            assert!(matches!(
                state,
                ShaderFactorCommandState::NeedsRefresh {
                    minimum_revision: 23,
                    ..
                }
            ));
            assert_eq!(draft.pending_attempt(), Some(intent.attempt));
            assert!(diagnostic.contains("result is uncertain"));
        }
    }

    #[test]
    fn snapshot_before_receipt_queues_the_newest_factor_after_acknowledgement() {
        let mut draft = pending_factor_with_newer_local_value();
        let attempt = draft.pending_attempt().expect("pending factor attempt");

        draft.reconcile_remote(24, 2.0);
        let DraftReceiptDecision::Acknowledged {
            local_changed_after_submission,
            ..
        } = draft.record_self_receipt(attempt, 24)
        else {
            panic!("matching snapshot must acknowledge the factor receipt");
        };

        assert!(local_changed_after_submission);
        assert_latest_factor_can_be_submitted(draft);
    }

    #[test]
    fn receipt_before_snapshot_queues_the_newest_factor_after_acknowledgement() {
        let mut draft = pending_factor_with_newer_local_value();
        let attempt = draft.pending_attempt().expect("pending factor attempt");

        assert!(matches!(
            draft.record_self_receipt(attempt, 24),
            DraftReceiptDecision::Recorded { .. }
        ));
        let DraftReconcileDecision::SelfAcknowledged {
            local_changed_after_submission,
            ..
        } = draft.reconcile_remote(24, 2.0)
        else {
            panic!("matching factor snapshot must acknowledge the recorded receipt");
        };

        assert!(local_changed_after_submission);
        assert_latest_factor_can_be_submitted(draft);
    }

    fn pending_factor_with_newer_local_value() -> RevisionedDraft<f32> {
        let mut draft = RevisionedDraft::new(23, 1.0);
        draft.edit_local(2.0);
        assert!(matches!(
            draft.plan_submission_at(23),
            DraftSubmissionDecision::Submit(_)
        ));
        draft.edit_local(3.0);
        draft
    }

    fn assert_latest_factor_can_be_submitted(mut draft: RevisionedDraft<f32>) {
        assert_eq!(draft.local(), &3.0);
        assert_eq!(draft.phase(), DraftPhase::Dirty);
        let DraftSubmissionDecision::Submit(intent) = draft.plan_submission_at(24) else {
            panic!("acknowledged dirty factor must produce the next intent");
        };
        assert_eq!(intent.value(), &3.0);
    }

    #[open_gpui::test]
    fn real_host_commits_the_newest_factor_after_a_second_pending_edit(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let mut catalog = NodeCatalog::new();
        catalog
            .register(catalog_spec())
            .expect("register shader multiply kind");
        let graph = pending_factor_graph(&catalog);
        let runtime = GraphRuntime::builder(catalog)
            .build(graph)
            .expect("valid shader factor graph");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach shader factor view");
        let captured = Rc::new(RefCell::new(None::<Entity<ShaderMultiplyNode>>));
        let captured_for_factory = Rc::clone(&captured);
        let draft_store = ShaderDraftStore::new();
        let mut registry = OpenGpuiNodeRegistry::builder();
        registry.register(SHADER_MULTIPLY_RENDERER, move |binding, _window, cx| {
            let node = mount_shader_multiply(binding, draft_store.clone(), cx);
            captured_for_factory.borrow_mut().replace(node.clone());
            node
        });
        let (_host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid shader factor registry"),
                pending_factor_viewport(),
            )
        });
        draw_pending_factor_host(cx);
        let node = captured
            .borrow()
            .clone()
            .expect("mounted shader multiply node");

        cx.update_entity(&node, |node, cx| {
            node.edit_factor(2.0, cx);
            assert!(matches!(
                node.factor_command,
                ShaderFactorCommandState::InFlight(_)
            ));
            node.edit_factor(3.0, cx);
            assert_eq!(node.factor.local(), &3.0);
            assert_eq!(
                node.factor.pending_attempt().map(DraftAttempt::get),
                Some(1)
            );
        });
        for _ in 0..4 {
            draw_pending_factor_host(cx);
        }

        let snapshot = runtime.snapshot().expect("latest shader factor snapshot");
        assert_eq!(snapshot.revision(), 2);
        assert_eq!(pending_factor_value(snapshot.graph()), 3.0);
        assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
        cx.read_entity(&node, |node, _| {
            assert_eq!(node.factor.local(), &3.0);
            assert_eq!(node.factor.phase(), DraftPhase::Clean);
            assert!(node.factor.pending_attempt().is_none());
            assert!(node.factor_command.is_idle());
            assert!(node.diagnostic.is_none());
        });

        runtime
            .execute(GraphCommand::undo(2))
            .expect("undo latest factor command")
            .applied()
            .expect("latest factor undo applies");
        draw_pending_factor_host(cx);
        assert_eq!(
            pending_factor_value(runtime.snapshot().unwrap().graph()),
            2.0
        );
        cx.read_entity(&node, |node, _| {
            assert_eq!(node.factor.local(), &2.0);
            assert_eq!(node.factor.phase(), DraftPhase::Clean);
        });
    }

    #[open_gpui::test]
    fn conflict_status_replaces_preview_and_stays_inside_fixed_node_bounds(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let mut catalog = NodeCatalog::new();
        catalog
            .register(catalog_spec())
            .expect("register shader multiply kind");
        let graph = pending_factor_graph(&catalog);
        let runtime = GraphRuntime::builder(catalog)
            .build(graph)
            .expect("valid shader conflict graph");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach shader conflict view");
        let captured = Rc::new(RefCell::new(None::<Entity<ShaderMultiplyNode>>));
        let captured_for_factory = Rc::clone(&captured);
        let draft_store = ShaderDraftStore::new();
        let mut registry = OpenGpuiNodeRegistry::builder();
        registry.register(SHADER_MULTIPLY_RENDERER, move |binding, _window, cx| {
            let node = mount_shader_multiply(binding, draft_store.clone(), cx);
            captured_for_factory.borrow_mut().replace(node.clone());
            node
        });
        let (_host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid shader conflict registry"),
                pending_factor_viewport(),
            )
        });
        draw_pending_factor_host(cx);
        let node = captured
            .borrow()
            .clone()
            .expect("mounted shader conflict node");

        cx.update_entity(&node, |node, cx| {
            node.factor.edit_local(2.0);
            assert!(matches!(
                node.factor.reconcile_remote(1, 3.0),
                DraftReconcileDecision::Conflict { .. }
            ));
            node.preview_expanded = true;
            cx.notify();
        });
        draw_pending_factor_host(cx);

        let node_bounds = cx
            .debug_bounds(SHADER_NODE_SELECTOR)
            .expect("fixed shader node bounds");
        let conflict_bounds = cx
            .debug_bounds(&shader_factor_conflict_selector(pending_factor_node_id()))
            .expect("shader conflict status");
        let accept_bounds = cx
            .debug_bounds(&shader_factor_accept_remote_selector(
                pending_factor_node_id(),
            ))
            .expect("shader accept-remote action");
        let reapply_bounds = cx
            .debug_bounds(&shader_factor_reapply_selector(pending_factor_node_id()))
            .expect("shader reapply action");
        assert!(conflict_bounds.is_contained_within(&node_bounds));
        assert!(accept_bounds.is_contained_within(&node_bounds));
        assert!(reapply_bounds.is_contained_within(&node_bounds));
        assert!(cx.debug_bounds(SHADER_PREVIEW_VALUE_SELECTOR).is_none());
    }

    fn pending_factor_graph(catalog: &NodeCatalog) -> jellyflow::core::Graph {
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x5ade_0000));
        let creation = catalog
            .instantiate_node_with_ids(
                &NodeKindKey::new(SHADER_MULTIPLY_KIND),
                pending_factor_node_id(),
                CanvasPoint::default(),
                [
                    PortId::from_u128(0x5ade_1101),
                    PortId::from_u128(0x5ade_1102),
                    PortId::from_u128(0x5ade_1103),
                ],
            )
            .expect("instantiate shader multiply node");
        let (node_id, node, ports) = creation.into_parts();
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
        graph.build().expect("shader factor graph")
    }

    fn pending_factor_node_id() -> NodeId {
        NodeId::from_u128(0x5ade_1000)
    }

    fn pending_factor_value(graph: &jellyflow::core::Graph) -> f32 {
        graph.node(&pending_factor_node_id()).unwrap().data[SHADER_FACTOR_FIELD]
            .as_f64()
            .expect("numeric shader factor") as f32
    }

    fn pending_factor_viewport() -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint { x: -10.0, y: -10.0 },
            size: CanvasSize {
                width: 320.0,
                height: 240.0,
            },
        }
    }

    fn draw_pending_factor_host(cx: &mut open_gpui::VisualTestContext) {
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
        cx.run_until_parked();
    }
}
