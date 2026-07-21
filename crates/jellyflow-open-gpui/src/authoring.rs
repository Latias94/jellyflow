use std::collections::BTreeMap;

use jellyflow_core::{Node, NodeId, Port, PortId};
use jellyflow_runtime::runtime::viewport::ViewportTransform;
use jellyflow_runtime::{
    GraphCommand, GraphRevision, GraphSnapshot, NodeInstanceKey, PortConnectionSummary,
    ViewCloseReason, ViewSessionError, WeakViewSession,
};
use open_gpui::{App, FocusHandle};

pub use crate::binding::OpenGpuiNodeBinding;
use crate::binding::{OpenGpuiMountCapability, OpenGpuiMountState};
use crate::command_journal::{
    OpenGpuiCommandEnqueueError, OpenGpuiCommandSink, OpenGpuiCommandTicket,
};
use crate::port_handle::OpenGpuiMeasurementSink;
pub use crate::port_handle::{OpenGpuiPortAnchor, OpenGpuiPortHandle, OpenGpuiPortSide};
use crate::presentation::{OpenGpuiNodePresentationSnapshot, OpenGpuiPortInteractionState};

/// One owned, immutable authoring view for an exact native node mount and graph revision.
///
/// The view deliberately owns its graph root and matching port capabilities. It is cheap to keep
/// for one input callback or render pass, but must not be retained as component state. Enqueueing a
/// command consumes it so a command cannot accidentally be rebound to newer facts.
pub struct OpenGpuiNodeAuthoring {
    instance: NodeInstanceKey,
    snapshot: GraphSnapshot,
    viewport_transform: ViewportTransform,
    presentation: OpenGpuiNodePresentationSnapshot,
    ports: Vec<OpenGpuiAuthoringPortState>,
    view: WeakViewSession,
    mount: OpenGpuiMountCapability,
    commands: OpenGpuiCommandSink,
}

impl std::fmt::Debug for OpenGpuiNodeAuthoring {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiNodeAuthoring")
            .field("instance", &self.instance)
            .field("revision", &self.snapshot.revision())
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
        instance: NodeInstanceKey,
        view: WeakViewSession,
        mount: OpenGpuiMountCapability,
        commands: OpenGpuiCommandSink,
        measurement: OpenGpuiMeasurementSink,
        port_focus: &BTreeMap<PortId, FocusHandle>,
        presentation: OpenGpuiNodePresentationSnapshot,
    ) -> Result<Self, OpenGpuiNodeAuthoringError> {
        ensure_current_instance(&instance, &view, &mount)?;

        let snapshot = view.graph_snapshot().map_err(map_session_error)?;
        let node_id = instance.node();
        let node = snapshot
            .graph()
            .node(&node_id)
            .ok_or(OpenGpuiNodeAuthoringError::MissingNode { node: node_id })?;
        let viewport_transform = view.viewport_transform().map_err(map_session_error)?;
        let mut ports = Vec::with_capacity(node.ports.len());
        for port_id in &node.ports {
            let port =
                snapshot
                    .graph()
                    .port(port_id)
                    .ok_or(OpenGpuiNodeAuthoringError::MissingPort {
                        node: node_id,
                        port: *port_id,
                    })?;
            let connection = view
                .port_connection_summary(*port_id)
                .map_err(map_session_error)?
                .ok_or(OpenGpuiNodeAuthoringError::MissingPort {
                    node: node_id,
                    port: *port_id,
                })?;
            if connection.revision() != snapshot.revision() {
                return Err(OpenGpuiNodeAuthoringError::IncoherentPortFacts {
                    port: *port_id,
                    authoring_revision: snapshot.revision(),
                    facts_revision: connection.revision(),
                });
            }
            ports.push(OpenGpuiAuthoringPortState {
                id: *port_id,
                connection,
                handle: OpenGpuiPortHandle::new(
                    instance.clone(),
                    *port_id,
                    port.dir,
                    port.key.0.clone(),
                    measurement.clone(),
                )
                .with_focus_handle(port_focus.get(port_id).cloned())
                .with_presentation_state(
                    presentation
                        .port(*port_id)
                        .cloned()
                        .unwrap_or_else(OpenGpuiPortInteractionState::idle),
                ),
            });
        }

        ensure_current_instance(&instance, &view, &mount)?;
        let current = view.graph_snapshot().map_err(map_session_error)?;
        if !snapshot.ptr_eq(&current) {
            return Err(OpenGpuiNodeAuthoringError::SnapshotChanged {
                authoring_revision: snapshot.revision(),
                current_revision: current.revision(),
            });
        }

        Ok(Self {
            instance,
            snapshot,
            viewport_transform,
            presentation,
            ports,
            view,
            mount,
            commands,
        })
    }

    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.instance.node()
    }

    #[must_use]
    pub fn revision(&self) -> GraphRevision {
        self.snapshot.revision()
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

    pub fn ports(&self) -> impl ExactSizeIterator<Item = OpenGpuiAuthoringPort<'_>> + '_ {
        self.node()
            .ports
            .iter()
            .zip(&self.ports)
            .map(move |(port_id, state)| {
                debug_assert_eq!(*port_id, state.id);
                OpenGpuiAuthoringPort {
                    id: state.id,
                    port: self
                        .snapshot
                        .graph()
                        .port(port_id)
                        .expect("an authoring view keeps every validated immutable port"),
                    connection: state.connection,
                    handle: &state.handle,
                }
            })
    }

    #[must_use]
    pub fn port(&self, id: PortId) -> Option<OpenGpuiAuthoringPort<'_>> {
        self.node()
            .ports
            .iter()
            .position(|port| *port == id)
            .and_then(|index| self.ports().nth(index))
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
        ensure_current_instance(&self.instance, &self.view, &self.mount)?;
        let command_revision = command.expected_revision();
        let authoring_revision = self.revision();
        if command_revision != authoring_revision {
            return Err(OpenGpuiNodeAuthoringError::CommandRevisionMismatch {
                authoring_revision,
                command_revision,
            });
        }

        let current = self.view.graph_snapshot().map_err(map_session_error)?;
        if !self.snapshot.ptr_eq(&current) {
            return Err(OpenGpuiNodeAuthoringError::StaleRevision {
                authoring_revision,
                current_revision: current.revision(),
            });
        }
        ensure_current_instance(&self.instance, &self.view, &self.mount)?;
        self.commands.enqueue(command, cx).map_err(Into::into)
    }
}

fn ensure_current_instance(
    instance: &NodeInstanceKey,
    view: &WeakViewSession,
    mount: &OpenGpuiMountCapability,
) -> Result<(), OpenGpuiNodeAuthoringError> {
    match mount.state() {
        OpenGpuiMountState::Live => {}
        OpenGpuiMountState::Retired => {
            return Err(OpenGpuiNodeAuthoringError::RetiredMount {
                node: instance.node(),
            });
        }
        OpenGpuiMountState::Closed(reason) => {
            return Err(OpenGpuiNodeAuthoringError::Closed { reason });
        }
    }

    let current = view
        .node_incarnation(instance.node())
        .map_err(map_session_error)?;
    if current != Some(instance.incarnation()) {
        return Err(OpenGpuiNodeAuthoringError::RetiredIncarnation {
            node: instance.node(),
        });
    }
    Ok(())
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
    #[error(
        "port {port:?} facts revision {facts_revision} does not match authoring revision {authoring_revision}"
    )]
    IncoherentPortFacts {
        port: PortId,
        authoring_revision: GraphRevision,
        facts_revision: GraphRevision,
    },
    #[error(
        "graph changed while opening authoring view: revision {authoring_revision} became {current_revision}"
    )]
    SnapshotChanged {
        authoring_revision: GraphRevision,
        current_revision: GraphRevision,
    },
    #[error(
        "command revision {command_revision} does not match authoring revision {authoring_revision}"
    )]
    CommandRevisionMismatch {
        authoring_revision: GraphRevision,
        command_revision: GraphRevision,
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
