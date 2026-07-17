//! Minimal fixture for implementing a native Jellyflow adapter.
//!
//! The template deliberately stops at semantic graph setup. Adapter code should request an owned
//! frame, render framework-native components, build complete layouts from the frame's targets, and
//! submit commands plus layouts through that frame's ordered feedback collector. Retained entities,
//! focus, input, caches, and application state belong to the target UI framework.

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphBuilder, GraphId, GraphValidationError,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow::runtime::schema::{
    NodeCatalog, NodeCatalogInstantiationError, NodeCatalogRegistrationError, NodeKindSpec,
    NodePortSpec, PortPresentationHint, PortSide,
};
use jellyflow::runtime::{
    FrameRequest, GraphCommand, GraphRevision, IncidentEdgePolicy, RendererStamp, TopologyEdit,
    TopologyPort,
};

pub const NATIVE_NODE_KIND: &str = "template.native.dynamic";
pub const NATIVE_RENDERER_KEY: &str = "template.native.dynamic";

const NODE_ID: u128 = 1;
const BASE_PORT_ID: u128 = 11;
const DYNAMIC_PORT_ID: u128 = 12;

#[derive(Debug, thiserror::Error)]
pub enum NativeGraphError {
    #[error(transparent)]
    Instantiation(#[from] NodeCatalogInstantiationError),
    #[error("native adapter fixture graph failed validation: {errors:?}")]
    Validation { errors: Vec<GraphValidationError> },
}

#[must_use]
pub fn fixture_node_id() -> NodeId {
    NodeId::from_u128(NODE_ID)
}

#[must_use]
pub fn base_port_id() -> PortId {
    PortId::from_u128(BASE_PORT_ID)
}

#[must_use]
pub fn dynamic_port_id() -> PortId {
    PortId::from_u128(DYNAMIC_PORT_ID)
}

/// Builds the application-owned semantic catalog used by the protocol fixture.
pub fn native_catalog() -> Result<NodeCatalog, NodeCatalogRegistrationError> {
    let mut catalog = NodeCatalog::new();
    catalog.register(
        NodeKindSpec::builder(NATIVE_NODE_KIND, "Dynamic native node", NATIVE_RENDERER_KEY)
            .default_size(CanvasSize {
                width: 240.0,
                height: 128.0,
            })
            .port(
                NodePortSpec::new(
                    "base",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                )
                .with_label("Base")
                .with_presentation(
                    PortPresentationHint::new()
                        .with_side(PortSide::Right)
                        .with_order(0),
                ),
            )
            .build(),
    )?;
    Ok(catalog)
}

/// Instantiates one semantic node with stable IDs and validates the graph before publication.
pub fn native_graph(catalog: &NodeCatalog) -> Result<Graph, NativeGraphError> {
    let creation = catalog.instantiate_node_with_ids(
        &NodeKindKey::new(NATIVE_NODE_KIND),
        fixture_node_id(),
        CanvasPoint { x: 24.0, y: 32.0 },
        [base_port_id()],
    )?;
    let (node_id, node, ports) = creation.into_parts();
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x4e41_5449_5645));
    builder.insert_node(node_id, node);
    for (port_id, port) in ports {
        builder.insert_port(port_id, port);
    }
    builder
        .build()
        .map_err(|errors| NativeGraphError::Validation { errors })
}

/// Maps the semantic kind to this adapter's native renderer for one owned frame request.
#[must_use]
pub fn fixture_frame_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 1024.0,
                height: 768.0,
            },
        },
        RendererStamp::new("template.native.fallback", 1),
    )
    .with_kind_renderer(
        NodeKindKey::new(NATIVE_NODE_KIND),
        RendererStamp::new(NATIVE_RENDERER_KEY, 1),
    )
    .with_kind_default(
        NodeKindKey::new(NATIVE_NODE_KIND),
        CanvasSize {
            width: 240.0,
            height: 128.0,
        },
    )
}

/// Adds one stable real port through a revision-checked command.
///
/// A native control should append this command to the originating frame's feedback collector in
/// event order. It must not synchronously mutate a runtime while framework rendering is borrowed.
#[must_use]
pub fn add_dynamic_port_command(expected_revision: GraphRevision, base: &Port) -> GraphCommand {
    let base = base.clone();
    GraphCommand::topology(
        expected_revision,
        TopologyEdit::new().replace_node_ports(
            fixture_node_id(),
            [
                TopologyPort::new(base_port_id(), base.clone()),
                TopologyPort::new(
                    dynamic_port_id(),
                    Port {
                        key: PortKey::new("dynamic"),
                        ..base
                    },
                ),
            ],
            IncidentEdgePolicy::Reject,
        ),
    )
    .with_label("add dynamic native port")
}
