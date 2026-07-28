use std::collections::BTreeMap;

use jellyflow_core::{Node, NodeId, Port, PortId};
use jellyflow_runtime::viewport::ViewportTransform;
use jellyflow_runtime::{
    commands::GraphCommand, graph::GraphRevision, graph::GraphSnapshot, graph::GraphVersion,
    graph::PortConnectionSummary, graph::PortInstanceKey, view::NodeInstanceKey,
    view::ViewCloseReason, view::ViewSessionError,
};
use open_gpui::App;

use crate::activity::OpenGpuiNodeActivityRegistry;
pub use crate::binding::OpenGpuiNodeBinding;
use crate::binding::{
    OpenGpuiMountContext, OpenGpuiMountValidationError, OpenGpuiNodePresentationInputSignature,
};
use crate::command_journal::OpenGpuiCommandSink;
pub use crate::command_journal::{
    OPEN_GPUI_COMMAND_JOURNAL_CAPACITY, OpenGpuiCommandEnqueueError, OpenGpuiCommandTicket,
    OpenGpuiCommandTicketError, OpenGpuiCommandTicketResult,
};
pub use crate::node_drag_handle::OpenGpuiNodeDragHandle;
pub use crate::port_handle::{OpenGpuiPortAnchor, OpenGpuiPortHandle, OpenGpuiPortSide};
use crate::presentation::{OpenGpuiNodePresentationSnapshot, OpenGpuiPortInteractionState};

/// One owned, immutable authoring view for an exact native node mount and graph revision.
///
/// The view deliberately owns its graph root and matching port capabilities. It is cheap to keep
/// for one input callback or render pass, but must not be retained as component state. Enqueueing a
/// command consumes it so a command cannot accidentally be rebound to newer facts.
pub struct OpenGpuiNodeAuthoring {
    context: OpenGpuiMountContext,
    snapshot: GraphSnapshot,
    viewport_transform: ViewportTransform,
    presentation: OpenGpuiNodePresentationSnapshot,
    ports: Vec<OpenGpuiAuthoringPortState>,
    port_index: BTreeMap<PortId, usize>,
    commands: OpenGpuiCommandSink,
    activity: OpenGpuiNodeActivityRegistry,
}

/// One value read from the immutable facts owned by an exact node authoring view.
///
/// The provenance is intentionally opaque. Retained fields consume it to reject values crossed
/// between nodes, native mounts, or graph snapshots.
pub struct OpenGpuiNodeObservation<T> {
    stamp: OpenGpuiAuthoringStamp,
    value: T,
}

impl<T> std::fmt::Debug for OpenGpuiNodeObservation<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiNodeObservation")
            .field("instance", self.stamp.context.instance())
            .field("version", &self.stamp.snapshot.version())
            .field("value", &self.value)
            .finish()
    }
}

impl<T> OpenGpuiNodeObservation<T> {
    #[cfg(test)]
    pub(crate) fn from_parts(stamp: OpenGpuiAuthoringStamp, value: T) -> Self {
        Self { stamp, value }
    }

    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        self.stamp.context.instance()
    }

    #[must_use]
    pub fn version(&self) -> GraphVersion {
        self.stamp.snapshot.version()
    }

    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    pub(crate) fn into_parts(self) -> (OpenGpuiAuthoringStamp, T) {
        (self.stamp, self.value)
    }

    #[cfg(test)]
    pub(crate) fn stamp(&self) -> &OpenGpuiAuthoringStamp {
        &self.stamp
    }
}

#[derive(Clone)]
pub(crate) struct OpenGpuiAuthoringStamp {
    pub(crate) context: OpenGpuiMountContext,
    pub(crate) snapshot: GraphSnapshot,
    pub(crate) activity: OpenGpuiNodeActivityRegistry,
}

impl OpenGpuiAuthoringStamp {
    pub(crate) fn ensure_current(&self) -> Result<(), OpenGpuiNodeAuthoringError> {
        let current = self.context.ensure_current().map_err(map_mount_error)?;
        if !self.snapshot.ptr_eq(&current) {
            return Err(OpenGpuiNodeAuthoringError::StaleRevision {
                authoring_revision: self.snapshot.revision(),
                current_revision: current.revision(),
            });
        }
        Ok(())
    }

    pub(crate) fn same_snapshot(&self, other: &Self) -> bool {
        self.snapshot.ptr_eq(&other.snapshot)
    }
}

impl std::fmt::Debug for OpenGpuiNodeAuthoring {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiNodeAuthoring")
            .field("instance", self.context.instance())
            .field("version", &self.snapshot.version())
            .field("port_count", &self.ports.len())
            .finish_non_exhaustive()
    }
}

struct OpenGpuiAuthoringPortState {
    id: PortId,
    connection: PortConnectionSummary,
    handle: OpenGpuiPortHandle,
}

/// Coherent graph facts and native capability for one real port in node order.
#[derive(Debug, Clone, Copy)]
pub struct OpenGpuiAuthoringPort<'a> {
    id: PortId,
    port: &'a Port,
    connection: PortConnectionSummary,
    handle: &'a OpenGpuiPortHandle,
}

impl<'a> OpenGpuiAuthoringPort<'a> {
    #[must_use]
    pub fn id(self) -> PortId {
        self.id
    }

    #[must_use]
    pub fn instance(self) -> PortInstanceKey {
        self.connection.instance()
    }

    #[must_use]
    pub fn facts(self) -> &'a Port {
        self.port
    }

    #[must_use]
    pub fn connection(self) -> PortConnectionSummary {
        self.connection
    }

    #[must_use]
    pub fn handle(self) -> &'a OpenGpuiPortHandle {
        self.handle
    }
}

impl OpenGpuiNodeAuthoring {
    pub(crate) fn open(
        context: OpenGpuiMountContext,
        commands: OpenGpuiCommandSink,
        activity: OpenGpuiNodeActivityRegistry,
        port_handle: impl Fn(PortId) -> Option<OpenGpuiPortHandle>,
        presentation_input: Option<&OpenGpuiNodePresentationInputSignature>,
        presentation: OpenGpuiNodePresentationSnapshot,
    ) -> Result<Self, OpenGpuiNodeAuthoringError> {
        let snapshot = context.ensure_current().map_err(map_mount_error)?;
        let node_id = context.instance().node();
        let node = snapshot
            .graph()
            .node(&node_id)
            .ok_or(OpenGpuiNodeAuthoringError::MissingNode { node: node_id })?;
        let viewport_transform = context
            .view()
            .viewport_transform()
            .map_err(map_session_error)?;
        let cached_input = presentation_input.filter(|input| {
            input.graph_revision() == snapshot.revision()
                && input
                    .ordered_connection_summaries()
                    .all(|(_, summary)| summary.version() == snapshot.version())
                && input
                    .ordered_connection_summaries()
                    .map(|(port, _)| port)
                    .eq(node.ports.iter().copied())
        });
        let summaries = if let Some(input) = cached_input {
            input.ordered_connection_summaries().collect::<Vec<_>>()
        } else {
            let summaries = context
                .view()
                .port_connection_summaries(&node.ports)
                .map_err(map_session_error)?
                .into_outcome();
            if summaries.version() != snapshot.version() {
                return Err(OpenGpuiNodeAuthoringError::IncoherentPortBatch {
                    authoring_version: snapshot.version(),
                    facts_version: summaries.version(),
                });
            }
            summaries
                .into_entries()
                .into_iter()
                .map(|(port, summary)| {
                    summary.map(|summary| (port, summary)).ok_or(
                        OpenGpuiNodeAuthoringError::MissingPort {
                            node: node_id,
                            port,
                        },
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut ports = Vec::with_capacity(node.ports.len());
        let mut port_index = BTreeMap::new();
        for (index, (port_id, connection)) in summaries.into_iter().enumerate() {
            debug_assert_eq!(node.ports.get(index), Some(&port_id));
            if connection.version() != snapshot.version() {
                return Err(OpenGpuiNodeAuthoringError::IncoherentPortBatch {
                    authoring_version: snapshot.version(),
                    facts_version: connection.version(),
                });
            }
            snapshot
                .graph()
                .port(&port_id)
                .ok_or(OpenGpuiNodeAuthoringError::MissingPort {
                    node: node_id,
                    port: port_id,
                })?;
            let handle =
                port_handle(port_id).ok_or(OpenGpuiNodeAuthoringError::MissingPortCapability {
                    node: node_id,
                    port: port_id,
                })?;
            let port_instance = connection.instance();
            if port_instance.port() != port_id
                || port_instance.node() != node_id
                || port_instance.owner() != context.instance().lifetime()
                || handle.port_instance() != port_instance
            {
                return Err(OpenGpuiNodeAuthoringError::IncoherentPortIdentity {
                    node: node_id,
                    port: port_id,
                });
            }
            port_index.insert(port_id, index);
            ports.push(OpenGpuiAuthoringPortState {
                id: port_id,
                connection,
                handle: handle.with_presentation_state(
                    presentation
                        .port(port_id)
                        .cloned()
                        .unwrap_or_else(OpenGpuiPortInteractionState::idle),
                ),
            });
        }

        let current = context.ensure_current().map_err(map_mount_error)?;
        if !snapshot.ptr_eq(&current) {
            return Err(OpenGpuiNodeAuthoringError::SnapshotChanged {
                authoring_revision: snapshot.revision(),
                current_revision: current.revision(),
            });
        }

        Ok(Self {
            context,
            snapshot,
            viewport_transform,
            presentation,
            ports,
            port_index,
            commands,
            activity,
        })
    }

    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        self.context.instance()
    }

    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.context.instance().node()
    }

    #[must_use]
    pub fn version(&self) -> GraphVersion {
        self.snapshot.version()
    }

    #[must_use]
    pub fn node(&self) -> &Node {
        self.snapshot
            .graph()
            .node(&self.node_id())
            .expect("an authoring view keeps its validated immutable node")
    }

    #[must_use]
    pub fn viewport_transform(&self) -> ViewportTransform {
        self.viewport_transform
    }

    #[must_use]
    pub fn presentation(&self) -> &OpenGpuiNodePresentationSnapshot {
        &self.presentation
    }

    /// Reads one field value from this view's immutable node facts and preserves its provenance.
    #[must_use]
    pub fn observe_field<T>(&self, read: impl FnOnce(&Node) -> T) -> OpenGpuiNodeObservation<T> {
        OpenGpuiNodeObservation {
            stamp: self.stamp(),
            value: read(self.node()),
        }
    }

    /// Fallible form of [`Self::observe_field`].
    pub fn try_observe_field<T, E>(
        &self,
        read: impl FnOnce(&Node) -> Result<T, E>,
    ) -> Result<OpenGpuiNodeObservation<T>, E> {
        Ok(OpenGpuiNodeObservation {
            stamp: self.stamp(),
            value: read(self.node())?,
        })
    }

    pub(crate) fn stamp(&self) -> OpenGpuiAuthoringStamp {
        OpenGpuiAuthoringStamp {
            context: self.context.clone(),
            snapshot: self.snapshot.clone(),
            activity: self.activity.clone(),
        }
    }

    pub fn ports(&self) -> impl ExactSizeIterator<Item = OpenGpuiAuthoringPort<'_>> + '_ {
        self.ports
            .iter()
            .map(move |state| self.port_from_state(state))
    }

    #[must_use]
    pub fn port(&self, id: PortId) -> Option<OpenGpuiAuthoringPort<'_>> {
        let index = *self.port_index.get(&id)?;
        self.ports
            .get(index)
            .map(|state| self.port_from_state(state))
    }

    fn port_from_state<'a>(
        &'a self,
        state: &'a OpenGpuiAuthoringPortState,
    ) -> OpenGpuiAuthoringPort<'a> {
        OpenGpuiAuthoringPort {
            id: state.id,
            port: self
                .snapshot
                .graph()
                .port(&state.id)
                .expect("an authoring view keeps every validated immutable port"),
            connection: state.connection,
            handle: &state.handle,
        }
    }

    /// Enqueues one application-built command against exactly these authoring facts.
    ///
    /// Validation is completed before journal admission. This method never refreshes the command,
    /// rewrites its expected revision, retries it, or reserves journal capacity on failure.
    pub fn enqueue(
        self,
        command: GraphCommand,
        cx: &mut App,
    ) -> Result<OpenGpuiCommandTicket, OpenGpuiNodeAuthoringError> {
        self.context.ensure_exact_view().map_err(map_mount_error)?;
        let command_version = command.expected_version();
        let authoring_version = self.snapshot.version();
        if command_version != authoring_version {
            return Err(OpenGpuiNodeAuthoringError::CommandVersionMismatch {
                authoring_version,
                command_version,
            });
        }

        let current = self.context.ensure_current().map_err(map_mount_error)?;
        if !self.snapshot.ptr_eq(&current) {
            return Err(OpenGpuiNodeAuthoringError::StaleRevision {
                authoring_revision: authoring_version.revision(),
                current_revision: current.revision(),
            });
        }
        self.context.ensure_exact_view().map_err(map_mount_error)?;
        self.commands.enqueue(command, cx).map_err(Into::into)
    }
}

fn map_mount_error(error: OpenGpuiMountValidationError) -> OpenGpuiNodeAuthoringError {
    match error {
        OpenGpuiMountValidationError::RetiredMount { node } => {
            OpenGpuiNodeAuthoringError::RetiredMount { node }
        }
        OpenGpuiMountValidationError::RetiredIncarnation { node } => {
            OpenGpuiNodeAuthoringError::RetiredIncarnation { node }
        }
        OpenGpuiMountValidationError::ForeignView => {
            OpenGpuiNodeAuthoringError::Session(ViewSessionError::ForeignInteractionFacts)
        }
        OpenGpuiMountValidationError::Closed { reason } => {
            OpenGpuiNodeAuthoringError::Closed { reason }
        }
        OpenGpuiMountValidationError::Session(error) => map_session_error(error),
    }
}

fn map_session_error(error: ViewSessionError) -> OpenGpuiNodeAuthoringError {
    match error {
        ViewSessionError::Closed { reason } => OpenGpuiNodeAuthoringError::Closed { reason },
        error => OpenGpuiNodeAuthoringError::Session(error),
    }
}

/// Failure to open or consume one mount-scoped authoring view.
#[derive(Debug, thiserror::Error)]
pub enum OpenGpuiNodeAuthoringError {
    #[error("native node mount is retired: {node:?}")]
    RetiredMount { node: NodeId },
    #[error("native node incarnation is retired: {node:?}")]
    RetiredIncarnation { node: NodeId },
    #[error("native node host is closed: {reason:?}")]
    Closed { reason: ViewCloseReason },
    #[error("node is missing from the authoring graph snapshot: {node:?}")]
    MissingNode { node: NodeId },
    #[error("port {port:?} is missing from node {node:?}'s authoring graph snapshot")]
    MissingPort { node: NodeId, port: PortId },
    #[error("port {port:?} has no live native capability for node {node:?}")]
    MissingPortCapability { node: NodeId, port: PortId },
    #[error("port {port:?} facts and native capability disagree for node {node:?}")]
    IncoherentPortIdentity { node: NodeId, port: PortId },
    #[error(
        "port facts version {facts_version:?} does not match authoring version {authoring_version:?}"
    )]
    IncoherentPortBatch {
        authoring_version: GraphVersion,
        facts_version: GraphVersion,
    },
    #[error(
        "graph changed while opening authoring view: revision {authoring_revision} became {current_revision}"
    )]
    SnapshotChanged {
        authoring_revision: GraphRevision,
        current_revision: GraphRevision,
    },
    #[error(
        "command version {command_version:?} does not match authoring version {authoring_version:?}"
    )]
    CommandVersionMismatch {
        authoring_version: GraphVersion,
        command_version: GraphVersion,
    },
    #[error(
        "authoring revision {authoring_revision} is stale; current graph revision is {current_revision}"
    )]
    StaleRevision {
        authoring_revision: GraphRevision,
        current_revision: GraphRevision,
    },
    #[error(transparent)]
    Session(ViewSessionError),
    #[error(transparent)]
    CommandEnqueue(#[from] OpenGpuiCommandEnqueueError),
}
