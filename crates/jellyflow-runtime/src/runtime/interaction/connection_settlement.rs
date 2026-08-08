use std::collections::BTreeSet;

use jellyflow_core::core::NodeId;

use crate::rules::EdgeEndpoint;
use crate::runtime::adapter_protocol::{GeometrySnapshot, PublishedEdgeRoute};
use crate::runtime::graph_runtime::{
    EdgeInstanceKey, GraphRevision, GraphRuntimePolicy, GraphSnapshot, PortInstanceKey,
};
use crate::runtime::interaction::ConnectionGestureBinding;
use crate::runtime::reconciliation::{
    MutationBeginError, MutationCommandResult, MutationPhase, MutationReconciler, MutationRetry,
};
use crate::runtime::view_session::ViewProvenance;

use super::intent::ConnectionCommitTarget;

#[derive(Debug, Clone)]
pub(super) struct ConnectionSettlementIntent {
    provenance: ViewProvenance,
    binding: ConnectionGestureBinding,
    target: ConnectionCommitTarget,
    submission_revision: GraphRevision,
}

impl ConnectionSettlementIntent {
    pub(super) fn new(
        provenance: ViewProvenance,
        binding: ConnectionGestureBinding,
        target: ConnectionCommitTarget,
        submission_revision: GraphRevision,
    ) -> Self {
        Self {
            provenance,
            binding,
            target,
            submission_revision,
        }
    }

    pub(super) fn provenance(&self) -> ViewProvenance {
        self.provenance
    }

    pub(super) fn binding(&self) -> &ConnectionGestureBinding {
        &self.binding
    }

    pub(super) fn target(&self) -> &ConnectionCommitTarget {
        &self.target
    }

    pub(super) fn submission_revision(&self) -> GraphRevision {
        self.submission_revision
    }

    fn revalidated_at(&self, submission_revision: GraphRevision) -> Self {
        Self {
            provenance: self.provenance,
            binding: self.binding.clone(),
            target: self.target.clone(),
            submission_revision,
        }
    }

    fn expected_installation_revision(&self) -> Option<GraphRevision> {
        self.submission_revision.checked_add(1)
    }

    fn edge_instance(&self, snapshot: &GraphSnapshot) -> Option<EdgeInstanceKey> {
        let expected_revision = self.expected_installation_revision()?;
        let binding = &self.binding;
        let fixed = binding.fixed_port_instance();
        if !snapshot.port_instance_is_current(fixed) {
            return None;
        }
        if let Some(edge) = binding.reconnect_edge()
            && (!snapshot.edge_instance_is_current(edge)
                || binding
                    .detached_port_instance()
                    .is_none_or(|port| !snapshot.port_instance_is_current(port)))
        {
            return None;
        }
        let target = self.target.installed_target(snapshot, expected_revision)?;
        let edge = match binding.reconnect_edge() {
            Some(edge) => edge,
            None => {
                let edge = snapshot.edge_instance_key(binding.edge_id())?;
                snapshot
                    .edge_instance_was_introduced_at(edge, expected_revision)
                    .then_some(edge)?
            }
        };
        let model = snapshot.graph().edge(&edge.edge())?;
        let topology_matches = match binding.fixed_endpoint() {
            EdgeEndpoint::From => model.from == fixed.port() && model.to == target.port(),
            EdgeEndpoint::To => model.from == target.port() && model.to == fixed.port(),
        };
        topology_matches.then_some(edge)
    }

    pub(super) fn is_satisfied(&self, snapshot: &GraphSnapshot) -> bool {
        self.edge_instance(snapshot).is_some()
    }

    pub(super) fn is_admissible(
        &self,
        snapshot: &GraphSnapshot,
        policy: GraphRuntimePolicy,
    ) -> bool {
        self.binding.is_interactive(snapshot, policy) && self.target.is_current(snapshot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EdgeRoutePublicationIdentity {
    edge: EdgeInstanceKey,
    source: PortInstanceKey,
    target: PortInstanceKey,
}

impl EdgeRoutePublicationIdentity {
    fn from_intent(intent: &ConnectionSettlementIntent, snapshot: &GraphSnapshot) -> Option<Self> {
        let edge = intent.edge_instance(snapshot)?;
        let model = snapshot.graph().edge(&edge.edge())?;
        Some(Self {
            edge,
            source: snapshot.port_instance_key(model.from)?,
            target: snapshot.port_instance_key(model.to)?,
        })
    }

    fn is_current(self, snapshot: &GraphSnapshot) -> bool {
        snapshot.edge_instance_is_current(self.edge)
            && snapshot.port_instance_is_current(self.source)
            && snapshot.port_instance_is_current(self.target)
            && snapshot
                .graph()
                .edge(&self.edge.edge())
                .is_some_and(|edge| {
                    edge.from == self.source.port() && edge.to == self.target.port()
                })
    }

    fn matches_route(self, route: &PublishedEdgeRoute) -> bool {
        route.instance() == self.edge
            && route.source_instance() == self.source
            && route.target_instance() == self.target
    }

    pub(super) fn edge(self) -> EdgeInstanceKey {
        self.edge
    }

    pub(super) fn source(self) -> PortInstanceKey {
        self.source
    }

    pub(super) fn target(self) -> PortInstanceKey {
        self.target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConnectionPublicationProof {
    minimum_revision: GraphRevision,
    route: EdgeRoutePublicationIdentity,
}

impl ConnectionPublicationProof {
    fn from_intent(intent: &ConnectionSettlementIntent, snapshot: &GraphSnapshot) -> Option<Self> {
        Some(Self {
            minimum_revision: snapshot.revision(),
            route: EdgeRoutePublicationIdentity::from_intent(intent, snapshot)?,
        })
    }

    fn is_current(self, snapshot: &GraphSnapshot) -> bool {
        snapshot.revision() >= self.minimum_revision && self.route.is_current(snapshot)
    }

    fn is_published(self, snapshot: &GraphSnapshot, geometry: &GeometrySnapshot) -> bool {
        if !self.is_current(snapshot) {
            return false;
        }
        let published = geometry.route(self.route.edge().edge());
        if published.is_some_and(|route| self.route.matches_route(route)) {
            return true;
        }
        published.is_none() && self.route_is_suppressed(snapshot)
    }

    fn route_is_suppressed(self, snapshot: &GraphSnapshot) -> bool {
        let Some(edge) = snapshot.graph().edge(&self.route.edge().edge()) else {
            return false;
        };
        edge.hidden
            || snapshot
                .graph()
                .port(&edge.from)
                .and_then(|port| snapshot.graph().node(&port.node))
                .is_some_and(|node| node.hidden)
            || snapshot
                .graph()
                .port(&edge.to)
                .and_then(|port| snapshot.graph().node(&port.node))
                .is_some_and(|node| node.hidden)
    }

    pub(super) fn route(self) -> EdgeRoutePublicationIdentity {
        self.route
    }
}

#[derive(Debug)]
enum ConnectionSettlementPhase {
    AwaitingPublication(ConnectionPublicationProof),
    Reconciling(MutationReconciler<ConnectionSettlementIntent>),
}

#[derive(Debug)]
pub(super) struct ConnectionSettlementOwner {
    intent: ConnectionSettlementIntent,
    phase: ConnectionSettlementPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionSettlementReconciliation {
    Pending,
    StateChanged,
    AwaitingPublication,
    ProtocolFailure,
    OwnerRetired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionSettlementRetryError {
    ResolutionNotRequired,
    NotReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionSettlementResolutionKind {
    Uncertain,
    Retryable,
}

#[derive(Debug)]
pub(super) struct ConnectionSettlementRetry {
    owner: ConnectionSettlementOwner,
    retry: Option<MutationRetry<ConnectionSettlementIntent>>,
}

#[derive(Debug)]
pub(super) struct ConnectionSettlementRetryPreparation {
    retry: MutationRetry<ConnectionSettlementIntent>,
}

#[derive(Debug)]
pub(super) enum ConnectionSettlementRetryOutcome {
    Settled(ConnectionSettlementOwner),
    AuthoritativelyApplied(ConnectionSettlementOwner),
    ProtocolFailure,
    IdentityExhausted(ConnectionSettlementOwner),
}

impl ConnectionSettlementRetry {
    pub(super) fn new(
        owner: ConnectionSettlementOwner,
        preparation: ConnectionSettlementRetryPreparation,
    ) -> Self {
        Self {
            owner,
            retry: Some(preparation.retry),
        }
    }

    pub(super) fn revalidate_at(&mut self, submission_revision: GraphRevision) {
        let intent = self.owner.intent.revalidated_at(submission_revision);
        let retry = self
            .retry
            .take()
            .expect("a pending connection retry retains its exact reducer capability")
            .rebind_intent(intent.clone());
        self.owner.intent = intent;
        self.retry = Some(retry);
    }

    pub(super) fn complete(
        mut self,
        completion: MutationCommandResult,
        snapshot: Option<&GraphSnapshot>,
    ) -> ConnectionSettlementRetryOutcome {
        let ConnectionSettlementPhase::Reconciling(reconciler) = &mut self.owner.phase else {
            unreachable!("a prepared connection retry retains its reconciling owner");
        };
        let retry = self
            .retry
            .take()
            .expect("a prepared connection retry retains its reducer capability");
        let attempt = match reconciler.begin_retry(retry, self.owner.intent.submission_revision()) {
            Ok(attempt) => attempt,
            Err(MutationBeginError::IdentityExhausted) => {
                return ConnectionSettlementRetryOutcome::IdentityExhausted(self.owner);
            }
            Err(
                MutationBeginError::Busy
                | MutationBeginError::RetryExpired
                | MutationBeginError::StaleRetryBase { .. },
            ) => unreachable!("a prepared retry remains bound to the unchanged reducer owner"),
        };
        let completion_transition = reconciler
            .complete(attempt, completion)
            .expect("the freshly begun connection retry is the active reducer attempt");
        let observation_transition = snapshot.map(|snapshot| {
            reconciler.observe(snapshot.revision(), |intent| intent.is_satisfied(snapshot))
        });
        let satisfied = completion_transition.became_satisfied()
            || observation_transition.is_some_and(|transition| transition.became_satisfied());
        if satisfied {
            let snapshot = snapshot.expect(
                "a satisfied connection retry is proven by its authoritative completion snapshot",
            );
            let Some(phase) = publication_phase(&self.owner.intent, snapshot) else {
                return ConnectionSettlementRetryOutcome::ProtocolFailure;
            };
            self.owner.phase = phase;
            return ConnectionSettlementRetryOutcome::AuthoritativelyApplied(self.owner);
        }
        ConnectionSettlementRetryOutcome::Settled(self.owner)
    }
}

fn publication_phase(
    intent: &ConnectionSettlementIntent,
    snapshot: &GraphSnapshot,
) -> Option<ConnectionSettlementPhase> {
    ConnectionPublicationProof::from_intent(intent, snapshot)
        .map(ConnectionSettlementPhase::AwaitingPublication)
}

impl ConnectionSettlementOwner {
    pub(super) fn awaiting_publication(
        intent: ConnectionSettlementIntent,
        snapshot: &GraphSnapshot,
    ) -> Option<Self> {
        Some(Self {
            phase: publication_phase(&intent, snapshot)?,
            intent,
        })
    }

    pub(super) fn uncertain(
        intent: ConnectionSettlementIntent,
    ) -> Result<Self, MutationBeginError> {
        let submission_revision = intent.submission_revision();
        let mut reconciler = MutationReconciler::default();
        let attempt = reconciler.begin(intent.clone(), submission_revision)?;
        reconciler
            .complete(
                attempt,
                MutationCommandResult::Uncertain {
                    diagnostic: "connection command outcome is unknown".to_owned(),
                },
            )
            .expect("the freshly allocated connection reconciliation attempt is active");
        Ok(Self {
            intent,
            phase: ConnectionSettlementPhase::Reconciling(reconciler),
        })
    }

    pub(super) fn binding(&self) -> &ConnectionGestureBinding {
        self.intent.binding()
    }

    pub(super) fn target(&self) -> &ConnectionCommitTarget {
        self.intent.target()
    }

    pub(super) fn resolution_kind(&self) -> Option<ConnectionSettlementResolutionKind> {
        match &self.phase {
            ConnectionSettlementPhase::Reconciling(reconciler) => match reconciler.phase() {
                MutationPhase::Uncertain => Some(ConnectionSettlementResolutionKind::Uncertain),
                MutationPhase::Retryable => Some(ConnectionSettlementResolutionKind::Retryable),
                MutationPhase::Idle | MutationPhase::Pending | MutationPhase::AwaitingSnapshot => {
                    None
                }
            },
            ConnectionSettlementPhase::AwaitingPublication(_) => None,
        }
    }

    pub(super) fn can_retry(&self) -> bool {
        self.retry_capability().is_some()
    }

    pub(super) fn publication_proof(&self) -> Option<ConnectionPublicationProof> {
        match self.phase {
            ConnectionSettlementPhase::AwaitingPublication(proof) => Some(proof),
            ConnectionSettlementPhase::Reconciling(_) => None,
        }
    }

    pub(super) fn required_nodes(&self, snapshot: &GraphSnapshot) -> BTreeSet<NodeId> {
        let mut nodes = BTreeSet::new();
        let binding = &self.intent.binding;
        for port in [
            Some(binding.fixed_port_instance()),
            binding.detached_port_instance(),
        ]
        .into_iter()
        .flatten()
        {
            if snapshot.port_instance_is_current(port) {
                nodes.insert(port.node());
            }
        }
        if let ConnectionCommitTarget::Existing(target) = &self.intent.target
            && snapshot.port_instance_is_current(*target)
        {
            nodes.insert(target.node());
        }
        if let Some(proof) = self.publication_proof() {
            for port in [proof.route().source(), proof.route().target()] {
                if snapshot.port_instance_is_current(port) {
                    nodes.insert(port.node());
                }
            }
        }
        nodes
    }

    pub(super) fn reconcile(
        &mut self,
        snapshot: &GraphSnapshot,
        policy: GraphRuntimePolicy,
    ) -> ConnectionSettlementReconciliation {
        match &mut self.phase {
            ConnectionSettlementPhase::AwaitingPublication(proof) => {
                if snapshot.revision() < proof.minimum_revision {
                    return if self.intent.is_admissible(snapshot, policy) {
                        ConnectionSettlementReconciliation::Pending
                    } else {
                        ConnectionSettlementReconciliation::OwnerRetired
                    };
                }
                if proof.is_current(snapshot) {
                    ConnectionSettlementReconciliation::Pending
                } else {
                    ConnectionSettlementReconciliation::OwnerRetired
                }
            }
            ConnectionSettlementPhase::Reconciling(reconciler) => {
                let transition =
                    reconciler.observe(snapshot.revision(), |intent| intent.is_satisfied(snapshot));
                if transition.became_satisfied() {
                    let Some(proof) =
                        ConnectionPublicationProof::from_intent(&self.intent, snapshot)
                    else {
                        return ConnectionSettlementReconciliation::ProtocolFailure;
                    };
                    self.phase = ConnectionSettlementPhase::AwaitingPublication(proof);
                    return ConnectionSettlementReconciliation::AwaitingPublication;
                }
                if self.intent.is_admissible(snapshot, policy) {
                    if transition.state_changed() {
                        ConnectionSettlementReconciliation::StateChanged
                    } else {
                        ConnectionSettlementReconciliation::Pending
                    }
                } else {
                    ConnectionSettlementReconciliation::OwnerRetired
                }
            }
        }
    }

    pub(super) fn is_published(
        &self,
        snapshot: &GraphSnapshot,
        geometry: &GeometrySnapshot,
    ) -> bool {
        geometry.provenance() == self.intent.provenance()
            && match self.phase {
                ConnectionSettlementPhase::AwaitingPublication(proof) => {
                    proof.is_published(snapshot, geometry)
                }
                ConnectionSettlementPhase::Reconciling(_) => false,
            }
    }

    pub(super) fn prepare_retry(
        &mut self,
        base_revision: GraphRevision,
    ) -> Result<ConnectionSettlementRetryPreparation, ConnectionSettlementRetryError> {
        if self.resolution_kind().is_none() {
            return Err(ConnectionSettlementRetryError::ResolutionNotRequired);
        }
        let refreshed_intent = self.intent.revalidated_at(base_revision);
        let retry = self
            .retry_capability()
            .ok_or(ConnectionSettlementRetryError::NotReady)?
            .rebind_intent(refreshed_intent.clone());
        self.intent = refreshed_intent;
        Ok(ConnectionSettlementRetryPreparation { retry })
    }

    fn retry_capability(&self) -> Option<MutationRetry<ConnectionSettlementIntent>> {
        let ConnectionSettlementPhase::Reconciling(reconciler) = &self.phase else {
            return None;
        };
        reconciler.retry(matches!(reconciler.phase(), MutationPhase::Uncertain))
    }
}
