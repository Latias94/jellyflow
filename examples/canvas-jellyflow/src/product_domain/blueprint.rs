//! Renderer-independent Blueprint pin schema and topology planning.

use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{
    Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
    TypeDesc,
};
use jellyflow_runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort};
use jellyflow_runtime::graph::{GraphSnapshot, GraphVersion};

pub const BLUEPRINT_CALL_KIND: &str = "blueprint.call";

const BLUEPRINT_PINS_FIELD: &str = "pins";
const BLUEPRINT_DYNAMIC_PORT_PREFIX: &str = "arg.";

/// A durable Blueprint input identity independent of its label and display order.
#[derive(Clone, Debug, PartialEq)]
pub struct BlueprintPin {
    id: String,
    port_id: PortId,
    name: String,
    ty: TypeDesc,
}

impl BlueprintPin {
    pub fn new(
        id: impl Into<String>,
        port_id: PortId,
        name: impl Into<String>,
        ty: TypeDesc,
    ) -> Result<Self, BlueprintError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(BlueprintError::EmptyPinId);
        }
        let name = name.into();
        if name.trim().is_empty() {
            return Err(BlueprintError::EmptyPinName);
        }
        Ok(Self {
            id: id.trim().to_owned(),
            port_id,
            name: name.trim().to_owned(),
            ty,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn port_id(&self) -> PortId {
        self.port_id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn ty(&self) -> &TypeDesc {
        &self.ty
    }
}

/// One explicit Blueprint edit retained unchanged across managed retries.
#[derive(Clone, Debug, PartialEq)]
pub enum BlueprintIntent {
    Add(BlueprintPin),
    Reorder {
        pin_id: String,
        index: usize,
    },
    Remove {
        pin_id: String,
        incident_edges: IncidentEdgePolicy,
    },
}

impl BlueprintIntent {
    #[must_use]
    pub fn add(pin: BlueprintPin) -> Self {
        Self::Add(pin)
    }

    #[must_use]
    pub fn reorder(pin_id: impl Into<String>, index: usize) -> Self {
        Self::Reorder {
            pin_id: pin_id.into(),
            index,
        }
    }

    #[must_use]
    pub fn remove(pin_id: impl Into<String>, incident_edges: IncidentEdgePolicy) -> Self {
        Self::Remove {
            pin_id: pin_id.into(),
            incident_edges,
        }
    }
}

/// A complete, validated Blueprint observation from one authoritative graph version.
#[derive(Clone, Debug)]
pub struct BlueprintObservation {
    version: GraphVersion,
    node_id: NodeId,
    title: String,
    static_ports: Vec<PortId>,
    pins: Vec<BlueprintPin>,
    data: serde_json::Value,
    ports: BTreeMap<PortId, Port>,
    occupied_ports: BTreeSet<PortId>,
}

impl BlueprintObservation {
    /// Reads a Blueprint from a complete runtime snapshot.
    pub fn from_snapshot(
        snapshot: &GraphSnapshot,
        node_id: NodeId,
    ) -> Result<Self, BlueprintError> {
        let node = snapshot
            .graph()
            .node(&node_id)
            .ok_or(BlueprintError::MissingNode { node: node_id })?;
        let ports = collect_node_ports(snapshot, node_id, node)?;
        let occupied_ports = snapshot.graph().ports().keys().copied().collect();
        Self::parse(snapshot.version(), node_id, node, ports, occupied_ports)
    }

    /// Reads a Blueprint from adapter-owned coherent facts for one graph version.
    pub fn from_parts(
        version: GraphVersion,
        node_id: NodeId,
        node: &Node,
        ports: BTreeMap<PortId, Port>,
    ) -> Result<Self, BlueprintError> {
        let occupied_ports = ports.keys().copied().collect();
        Self::parse(version, node_id, node, ports, occupied_ports)
    }

    fn parse(
        version: GraphVersion,
        node_id: NodeId,
        node: &Node,
        ports: BTreeMap<PortId, Port>,
        occupied_ports: BTreeSet<PortId>,
    ) -> Result<Self, BlueprintError> {
        ensure_kind(node_id, node)?;
        let pins = parse_pins(node_id, &node.data)?;
        let static_ports = validate_topology(node_id, node, &ports, &pins)?;
        let title = node
            .data
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Function Call")
            .to_owned();
        Ok(Self {
            version,
            node_id,
            title,
            static_ports,
            pins,
            data: node.data.clone(),
            ports,
            occupied_ports,
        })
    }

    #[must_use]
    pub const fn version(&self) -> GraphVersion {
        self.version
    }

    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn static_ports(&self) -> &[PortId] {
        &self.static_ports
    }

    #[must_use]
    pub fn pins(&self) -> &[BlueprintPin] {
        &self.pins
    }

    #[must_use]
    pub fn port(&self, port: PortId) -> Option<&Port> {
        self.ports.get(&port)
    }

    /// Plans one atomic data-and-port replacement without mutating the graph.
    pub fn plan(&self, intent: &BlueprintIntent) -> Result<GraphCommand, BlueprintError> {
        self.plan_with_port_occupancy(intent, |port| self.occupied_ports.contains(&port))
    }

    /// Plans against adapter-owned graph-wide port occupancy.
    pub fn plan_with_port_occupancy(
        &self,
        intent: &BlueprintIntent,
        port_is_occupied: impl Fn(PortId) -> bool,
    ) -> Result<GraphCommand, BlueprintError> {
        let mut pins = self.pins.clone();
        let (label, incident_edges) = match intent {
            BlueprintIntent::Add(pin) => {
                if pins.iter().any(|current| current.id == pin.id) {
                    return Err(BlueprintError::DuplicatePinId {
                        pin_id: pin.id.clone(),
                    });
                }
                if port_is_occupied(pin.port_id) {
                    return Err(BlueprintError::PinPortAlreadyExists { port: pin.port_id });
                }
                if pins.iter().any(|current| current.name == pin.name) {
                    return Err(BlueprintError::DuplicatePinName {
                        name: pin.name.clone(),
                    });
                }
                pins.push(pin.clone());
                ("add blueprint pin", IncidentEdgePolicy::Reject)
            }
            BlueprintIntent::Reorder { pin_id, index } => {
                let current = pins
                    .iter()
                    .position(|pin| pin.id == *pin_id)
                    .ok_or_else(|| BlueprintError::PinNotFound {
                        pin_id: pin_id.clone(),
                    })?;
                if *index >= pins.len() {
                    return Err(BlueprintError::PinIndexOutOfBounds {
                        pin_id: pin_id.clone(),
                        index: *index,
                        pin_count: pins.len(),
                    });
                }
                let pin = pins.remove(current);
                pins.insert(*index, pin);
                ("reorder blueprint pin", IncidentEdgePolicy::Reject)
            }
            BlueprintIntent::Remove {
                pin_id,
                incident_edges,
            } => {
                let current = pins
                    .iter()
                    .position(|pin| pin.id == *pin_id)
                    .ok_or_else(|| BlueprintError::PinNotFound {
                        pin_id: pin_id.clone(),
                    })?;
                pins.remove(current);
                ("remove blueprint pin", *incident_edges)
            }
        };
        validate_unique_pins(&pins)?;

        let data = replace_pins(self.node_id, &self.data, &pins)?;
        let ordered = self
            .static_ports
            .iter()
            .map(|port| {
                self.ports
                    .get(port)
                    .cloned()
                    .map(|facts| TopologyPort::new(*port, facts))
                    .ok_or(BlueprintError::MissingPort {
                        node: self.node_id,
                        port: *port,
                    })
            })
            .chain(pins.iter().map(|pin| {
                let facts = self
                    .ports
                    .get(&pin.port_id())
                    .cloned()
                    .unwrap_or_else(|| dynamic_pin_port(self.node_id, pin));
                Ok(TopologyPort::new(pin.port_id(), facts))
            }))
            .collect::<Result<Vec<_>, BlueprintError>>()?;
        Ok(GraphCommand::topology(
            self.version,
            TopologyEdit::new()
                .set_node_data(self.node_id, data)
                .replace_node_ports(self.node_id, ordered, incident_edges),
        )
        .with_label(label))
    }

    /// Returns whether complete authoritative topology already fulfills an intent.
    #[must_use]
    pub fn satisfies(&self, intent: &BlueprintIntent) -> bool {
        match intent {
            BlueprintIntent::Add(pin) => self.pins.iter().any(|current| current == pin),
            BlueprintIntent::Reorder { pin_id, index } => {
                self.pins.get(*index).is_some_and(|pin| pin.id() == pin_id)
            }
            BlueprintIntent::Remove { pin_id, .. } => {
                self.pins.iter().all(|pin| pin.id() != pin_id)
            }
        }
    }
}

/// Typed failures from parsing, validating, or planning a Blueprint mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlueprintError {
    MissingNode {
        node: NodeId,
    },
    WrongNodeKind {
        node: NodeId,
        actual: String,
    },
    InvalidData {
        node: NodeId,
        field: &'static str,
    },
    EmptyPinId,
    EmptyPinName,
    DuplicatePinId {
        pin_id: String,
    },
    DuplicatePinPort {
        port: PortId,
    },
    DuplicatePinName {
        name: String,
    },
    PinPortAlreadyExists {
        port: PortId,
    },
    PinNotFound {
        pin_id: String,
    },
    PinIndexOutOfBounds {
        pin_id: String,
        index: usize,
        pin_count: usize,
    },
    MissingPort {
        node: NodeId,
        port: PortId,
    },
    InvalidStaticPorts {
        node: NodeId,
    },
    InvalidPinPort {
        node: NodeId,
        port: PortId,
    },
    InvalidPortOrder {
        node: NodeId,
    },
}

impl std::fmt::Display for BlueprintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode { node } => write!(formatter, "blueprint node is missing: {node:?}"),
            Self::WrongNodeKind { node, actual } => write!(
                formatter,
                "blueprint node {node:?} has kind {actual:?}, expected {BLUEPRINT_CALL_KIND:?}"
            ),
            Self::InvalidData { node, field } => {
                write!(
                    formatter,
                    "blueprint node {node:?} has invalid {field} data"
                )
            }
            Self::EmptyPinId => formatter.write_str("blueprint pin id must not be empty"),
            Self::EmptyPinName => formatter.write_str("blueprint pin name must not be empty"),
            Self::DuplicatePinId { pin_id } => {
                write!(formatter, "blueprint pin id is duplicated: {pin_id:?}")
            }
            Self::DuplicatePinPort { port } => {
                write!(formatter, "blueprint pin port is duplicated: {port:?}")
            }
            Self::DuplicatePinName { name } => {
                write!(formatter, "blueprint pin name is duplicated: {name:?}")
            }
            Self::PinPortAlreadyExists { port } => {
                write!(formatter, "blueprint pin port is already in use: {port:?}")
            }
            Self::PinNotFound { pin_id } => {
                write!(formatter, "blueprint pin does not exist: {pin_id:?}")
            }
            Self::PinIndexOutOfBounds {
                pin_id,
                index,
                pin_count,
            } => write!(
                formatter,
                "blueprint pin {pin_id:?} cannot move to index {index}; pin count is {pin_count}"
            ),
            Self::MissingPort { node, port } => {
                write!(
                    formatter,
                    "blueprint node {node:?} is missing port {port:?}"
                )
            }
            Self::InvalidStaticPorts { node } => {
                write!(
                    formatter,
                    "blueprint node {node:?} has invalid static ports"
                )
            }
            Self::InvalidPinPort { node, port } => {
                write!(
                    formatter,
                    "blueprint node {node:?} has invalid pin port {port:?}"
                )
            }
            Self::InvalidPortOrder { node } => write!(
                formatter,
                "blueprint node {node:?} must order static ports before dynamic pins"
            ),
        }
    }
}

impl std::error::Error for BlueprintError {}

fn collect_node_ports(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
    node: &Node,
) -> Result<BTreeMap<PortId, Port>, BlueprintError> {
    node.ports
        .iter()
        .map(|port_id| {
            snapshot
                .graph()
                .port(port_id)
                .cloned()
                .map(|port| (*port_id, port))
                .ok_or(BlueprintError::MissingPort {
                    node: node_id,
                    port: *port_id,
                })
        })
        .collect()
}

fn parse_pins(
    node_id: NodeId,
    data: &serde_json::Value,
) -> Result<Vec<BlueprintPin>, BlueprintError> {
    let pins = data
        .get(BLUEPRINT_PINS_FIELD)
        .and_then(serde_json::Value::as_array)
        .ok_or(BlueprintError::InvalidData {
            node: node_id,
            field: BLUEPRINT_PINS_FIELD,
        })?;
    let pins = pins
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let id = value.get("id").and_then(serde_json::Value::as_str).ok_or(
                BlueprintError::InvalidData {
                    node: node_id,
                    field: "pins[].id",
                },
            )?;
            let port_id = value
                .get("port_id")
                .cloned()
                .ok_or(BlueprintError::InvalidData {
                    node: node_id,
                    field: "pins[].port_id",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| BlueprintError::InvalidData {
                        node: node_id,
                        field: "pins[].port_id",
                    })
                })?;
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or(BlueprintError::InvalidData {
                    node: node_id,
                    field: "pins[].name",
                })?;
            let ty = value
                .get("type")
                .cloned()
                .ok_or(BlueprintError::InvalidData {
                    node: node_id,
                    field: "pins[].type",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| BlueprintError::InvalidData {
                        node: node_id,
                        field: "pins[].type",
                    })
                })?;
            let order = value
                .get("order")
                .and_then(serde_json::Value::as_u64)
                .and_then(|order| usize::try_from(order).ok())
                .ok_or(BlueprintError::InvalidData {
                    node: node_id,
                    field: "pins[].order",
                })?;
            if order != index {
                return Err(BlueprintError::InvalidData {
                    node: node_id,
                    field: "pins[].order",
                });
            }
            BlueprintPin::new(id, port_id, name, ty)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_pins(&pins)?;
    Ok(pins)
}

fn replace_pins(
    node_id: NodeId,
    data: &serde_json::Value,
    pins: &[BlueprintPin],
) -> Result<serde_json::Value, BlueprintError> {
    let mut data = data.clone();
    let object = data.as_object_mut().ok_or(BlueprintError::InvalidData {
        node: node_id,
        field: "object",
    })?;
    object.insert(
        BLUEPRINT_PINS_FIELD.to_owned(),
        serde_json::Value::Array(
            pins.iter()
                .enumerate()
                .map(|(order, pin)| {
                    serde_json::json!({
                        "id": pin.id,
                        "port_id": pin.port_id,
                        "name": pin.name,
                        "type": pin.ty,
                        "order": order,
                    })
                })
                .collect(),
        ),
    );
    Ok(data)
}

fn validate_unique_pins(pins: &[BlueprintPin]) -> Result<(), BlueprintError> {
    let mut ids = BTreeSet::new();
    let mut ports = BTreeSet::new();
    let mut names = BTreeSet::new();
    for pin in pins {
        if !ids.insert(pin.id.as_str()) {
            return Err(BlueprintError::DuplicatePinId {
                pin_id: pin.id.clone(),
            });
        }
        if !ports.insert(pin.port_id) {
            return Err(BlueprintError::DuplicatePinPort { port: pin.port_id });
        }
        if !names.insert(pin.name.as_str()) {
            return Err(BlueprintError::DuplicatePinName {
                name: pin.name.clone(),
            });
        }
    }
    Ok(())
}

fn validate_topology(
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    pins: &[BlueprintPin],
) -> Result<Vec<PortId>, BlueprintError> {
    let static_ports = static_ports(node_id, node, ports)?;
    for pin in pins {
        let port = ports
            .get(&pin.port_id())
            .ok_or(BlueprintError::MissingPort {
                node: node_id,
                port: pin.port_id(),
            })?;
        if !pin_port_matches(node_id, port, pin) {
            return Err(BlueprintError::InvalidPinPort {
                node: node_id,
                port: pin.port_id(),
            });
        }
    }
    let expected = static_ports
        .iter()
        .copied()
        .chain(pins.iter().map(BlueprintPin::port_id))
        .collect::<Vec<_>>();
    if node.ports != expected || ports.len() != expected.len() {
        return Err(BlueprintError::InvalidPortOrder { node: node_id });
    }
    Ok(static_ports)
}

fn static_ports(
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
) -> Result<Vec<PortId>, BlueprintError> {
    let mut result = Vec::with_capacity(3);
    for (key, direction, kind, capacity, ty) in [
        (
            "exec_in",
            PortDirection::In,
            PortKind::Exec,
            PortCapacity::Single,
            None,
        ),
        (
            "exec_out",
            PortDirection::Out,
            PortKind::Exec,
            PortCapacity::Multi,
            None,
        ),
        (
            "value",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
            Some(TypeDesc::Float),
        ),
    ] {
        let mut matching = node.ports.iter().filter_map(|id| {
            ports
                .get(id)
                .filter(|port| port.node == node_id && port.key.0 == key)
                .map(|_| *id)
        });
        let Some(id) = matching.next() else {
            return Err(BlueprintError::InvalidStaticPorts { node: node_id });
        };
        if matching.next().is_some() {
            return Err(BlueprintError::InvalidStaticPorts { node: node_id });
        }
        let port = ports
            .get(&id)
            .expect("a matched static Blueprint port must still exist");
        if port.dir != direction || port.kind != kind || port.capacity != capacity || port.ty != ty
        {
            return Err(BlueprintError::InvalidStaticPorts { node: node_id });
        }
        result.push(id);
    }
    Ok(result)
}

fn pin_port_matches(node_id: NodeId, port: &Port, pin: &BlueprintPin) -> bool {
    port.node == node_id
        && port.key == dynamic_pin_key(pin.id())
        && port.dir == PortDirection::In
        && port.kind == PortKind::Data
        && port.capacity == PortCapacity::Single
        && port.ty.as_ref() == Some(pin.ty())
        && port.data.get("pin_id").and_then(serde_json::Value::as_str) == Some(pin.id())
        && port.data.get("label").and_then(serde_json::Value::as_str) == Some(pin.name())
}

fn ensure_kind(node_id: NodeId, node: &Node) -> Result<(), BlueprintError> {
    if node.kind == NodeKindKey::new(BLUEPRINT_CALL_KIND) {
        Ok(())
    } else {
        Err(BlueprintError::WrongNodeKind {
            node: node_id,
            actual: node.kind.0.clone(),
        })
    }
}

pub(crate) fn dynamic_pin_port(node: NodeId, pin: &BlueprintPin) -> Port {
    Port {
        node,
        key: dynamic_pin_key(pin.id()),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(pin.ty.clone()),
        data: serde_json::json!({
            "pin_id": pin.id,
            "label": pin.name,
        }),
    }
}

pub(crate) fn dynamic_pin_key(pin_id: &str) -> PortKey {
    PortKey::new(format!("{BLUEPRINT_DYNAMIC_PORT_PREFIX}{pin_id}"))
}

#[must_use]
pub fn blueprint_default_data() -> serde_json::Value {
    serde_json::json!({
        "title": "Function Call",
        "pins": [],
    })
}
