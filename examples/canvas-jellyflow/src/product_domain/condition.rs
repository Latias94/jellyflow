//! Renderer-independent workflow Condition schema and topology planning.

use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{
    Node, NodeId, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind, TypeDesc,
};
use jellyflow_runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort};
use jellyflow_runtime::graph::{GraphSnapshot, GraphVersion};

pub const CONDITION_KIND: &str = "workflow.condition";
pub const CONDITION_INPUT_PORT_KEY: &str = "input";
pub const CONDITION_ELSE_PORT_KEY: &str = "else";

/// Comparison operation persisted as product data rather than display text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionOperator {
    Equals,
}

impl ConditionOperator {
    const fn key(self) -> &'static str {
        match self {
            Self::Equals => "equals",
        }
    }

    fn parse(node: NodeId, value: &serde_json::Value) -> Result<Self, ConditionError> {
        match value.as_str() {
            Some("equals") => Ok(Self::Equals),
            _ => Err(ConditionError::InvalidData {
                node,
                field: "cases[].predicate.operator",
            }),
        }
    }
}

/// One executable comparison independent of its rendered label and native controls.
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionPredicate {
    operand: String,
    operator: ConditionOperator,
    literal: serde_json::Value,
    value_type: TypeDesc,
}

impl ConditionPredicate {
    pub fn equals(
        operand: impl Into<String>,
        literal: serde_json::Value,
        value_type: TypeDesc,
    ) -> Result<Self, ConditionError> {
        let operand = operand.into();
        if operand.trim().is_empty() {
            return Err(ConditionError::EmptyPredicateOperand);
        }
        if !literal_matches_type(&literal, &value_type) {
            return Err(ConditionError::InvalidPredicateLiteral { operand });
        }
        Ok(Self {
            operand,
            operator: ConditionOperator::Equals,
            literal,
            value_type,
        })
    }

    #[must_use]
    pub fn operand(&self) -> &str {
        &self.operand
    }

    #[must_use]
    pub const fn operator(&self) -> ConditionOperator {
        self.operator
    }

    #[must_use]
    pub const fn literal(&self) -> &serde_json::Value {
        &self.literal
    }

    #[must_use]
    pub const fn value_type(&self) -> &TypeDesc {
        &self.value_type
    }
}

/// One durable Condition case, independent of native layout and controls.
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionCase {
    id: String,
    label: String,
    port_id: PortId,
    value_port_id: PortId,
    predicate: ConditionPredicate,
}

impl ConditionCase {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        port_id: PortId,
        value_port_id: PortId,
        predicate: ConditionPredicate,
    ) -> Result<Self, ConditionError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(ConditionError::EmptyCaseId);
        }
        if id == CONDITION_ELSE_PORT_KEY {
            return Err(ConditionError::ReservedCaseId { case_id: id });
        }
        let label = label.into();
        if label.trim().is_empty() {
            return Err(ConditionError::EmptyCaseLabel { case_id: id });
        }
        if port_id == value_port_id {
            return Err(ConditionError::DuplicateCasePort { port: port_id });
        }
        Ok(Self {
            id,
            label,
            port_id,
            value_port_id,
            predicate,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn port_id(&self) -> PortId {
        self.port_id
    }

    /// Returns the stable typed data input associated with this branch.
    #[must_use]
    pub const fn value_port_id(&self) -> PortId {
        self.value_port_id
    }

    /// Returns the value type selected when this branch was authored.
    #[must_use]
    pub const fn value_type(&self) -> &TypeDesc {
        self.predicate.value_type()
    }

    #[must_use]
    pub const fn predicate(&self) -> &ConditionPredicate {
        &self.predicate
    }
}

/// The fixed fallback output outside the editable case domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConditionElseBranch {
    label: String,
    port_id: PortId,
}

impl ConditionElseBranch {
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn port_id(&self) -> PortId {
        self.port_id
    }
}

/// One explicit product intent retained unchanged across a managed retry.
#[derive(Clone, Debug, PartialEq)]
pub enum ConditionIntent {
    Add(ConditionCase),
    Reorder {
        case_id: String,
        index: usize,
    },
    Remove {
        case_id: String,
        incident_edges: IncidentEdgePolicy,
    },
}

impl ConditionIntent {
    #[must_use]
    pub fn add(case: ConditionCase) -> Self {
        Self::Add(case)
    }

    #[must_use]
    pub fn reorder(case_id: impl Into<String>, index: usize) -> Self {
        Self::Reorder {
            case_id: case_id.into(),
            index,
        }
    }

    #[must_use]
    pub fn remove(case_id: impl Into<String>, incident_edges: IncidentEdgePolicy) -> Self {
        Self::Remove {
            case_id: case_id.into(),
            incident_edges,
        }
    }
}

/// A complete, validated Condition observation from one authoritative graph version.
///
/// Parsing and topology validation happen once at construction. Planning and satisfaction then
/// operate on the same typed facts, so a renderer cannot accidentally use weaker validation for
/// one path than the other.
#[derive(Clone, Debug)]
pub struct ConditionObservation {
    version: GraphVersion,
    node_id: NodeId,
    title: String,
    input_port: PortId,
    cases: Vec<ConditionCase>,
    else_branch: ConditionElseBranch,
    data: serde_json::Value,
    ports: BTreeMap<PortId, Port>,
    occupied_ports: BTreeSet<PortId>,
}

impl ConditionObservation {
    /// Reads a Condition from a complete runtime snapshot.
    pub fn from_snapshot(
        snapshot: &GraphSnapshot,
        node_id: NodeId,
    ) -> Result<Self, ConditionError> {
        let node = snapshot
            .graph()
            .node(&node_id)
            .ok_or(ConditionError::MissingNode { node: node_id })?;
        let ports = collect_node_ports(snapshot, node_id, node)?;
        let occupied_ports = snapshot.graph().ports().keys().copied().collect();
        Self::parse(snapshot.version(), node_id, node, ports, occupied_ports)
    }

    /// Reads a Condition from adapter-owned, coherent facts for one graph version.
    ///
    /// `ports` must contain the complete ordered node port set. The supplied IDs also form the
    /// collision set for new case ports; adapters with a complete graph snapshot should prefer
    /// [`Self::from_snapshot`] so collisions with every graph port are rejected as well.
    pub fn from_parts(
        version: GraphVersion,
        node_id: NodeId,
        node: &Node,
        ports: BTreeMap<PortId, Port>,
    ) -> Result<Self, ConditionError> {
        let occupied_ports = ports.keys().copied().collect();
        Self::parse(version, node_id, node, ports, occupied_ports)
    }

    fn parse(
        version: GraphVersion,
        node_id: NodeId,
        node: &Node,
        ports: BTreeMap<PortId, Port>,
        occupied_ports: BTreeSet<PortId>,
    ) -> Result<Self, ConditionError> {
        ensure_kind(node_id, node)?;
        let cases = parse_cases(node_id, &node.data)?;
        let else_branch = parse_else_branch(node_id, &node.data, &ports)?;
        let input_port = validate_topology(node_id, node, &cases, &else_branch, &ports)?;
        let title = node
            .data
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Condition")
            .to_owned();
        Ok(Self {
            version,
            node_id,
            title,
            input_port,
            cases,
            else_branch,
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
    pub const fn input_port(&self) -> PortId {
        self.input_port
    }

    #[must_use]
    pub fn cases(&self) -> &[ConditionCase] {
        &self.cases
    }

    #[must_use]
    pub const fn else_branch(&self) -> &ConditionElseBranch {
        &self.else_branch
    }

    /// Plans one atomic data-and-port replacement without mutating the graph.
    pub fn plan(&self, intent: &ConditionIntent) -> Result<GraphCommand, ConditionError> {
        self.plan_with_port_occupancy(intent, |port| self.occupied_ports.contains(&port))
    }

    /// Plans against adapter-owned graph-wide port occupancy without exposing adapter state.
    ///
    /// This is the planning authority for adapters that construct observations from local node
    /// facts. The callback must report every occupied graph port, including ports on other nodes.
    pub fn plan_with_port_occupancy(
        &self,
        intent: &ConditionIntent,
        port_is_occupied: impl Fn(PortId) -> bool,
    ) -> Result<GraphCommand, ConditionError> {
        let mut cases = self.cases.clone();
        let (label, incident_edges) = match intent {
            ConditionIntent::Add(case) => {
                if cases.iter().any(|current| current.id == case.id) {
                    return Err(ConditionError::DuplicateCaseId {
                        case_id: case.id.clone(),
                    });
                }
                if port_is_occupied(case.port_id) {
                    return Err(ConditionError::CasePortAlreadyExists { port: case.port_id });
                }
                if port_is_occupied(case.value_port_id) {
                    return Err(ConditionError::CasePortAlreadyExists {
                        port: case.value_port_id,
                    });
                }
                cases.push(case.clone());
                ("add condition case", IncidentEdgePolicy::Reject)
            }
            ConditionIntent::Reorder { case_id, index } => {
                let current = cases
                    .iter()
                    .position(|case| case.id == *case_id)
                    .ok_or_else(|| ConditionError::CaseNotFound {
                        case_id: case_id.clone(),
                    })?;
                if *index >= cases.len() {
                    return Err(ConditionError::CaseReorderIndexOutOfBounds {
                        index: *index,
                        case_count: cases.len(),
                    });
                }
                let case = cases.remove(current);
                cases.insert(*index, case);
                ("reorder condition case", IncidentEdgePolicy::Reject)
            }
            ConditionIntent::Remove {
                case_id,
                incident_edges,
            } => {
                let current = cases
                    .iter()
                    .position(|case| case.id == *case_id)
                    .ok_or_else(|| ConditionError::CaseNotFound {
                        case_id: case_id.clone(),
                    })?;
                cases.remove(current);
                ("remove condition case", *incident_edges)
            }
        };
        validate_unique_cases(&cases)?;

        let mut topology_ports = Vec::with_capacity(cases.len() * 2 + 2);
        topology_ports.push(TopologyPort::new(
            self.input_port,
            self.port(self.input_port)?.clone(),
        ));
        for case in &cases {
            let value_port = self
                .ports
                .get(&case.value_port_id)
                .cloned()
                .unwrap_or_else(|| condition_case_value_port(self.node_id, case));
            topology_ports.push(TopologyPort::new(case.value_port_id, value_port));
            let route_port = self
                .ports
                .get(&case.port_id)
                .cloned()
                .unwrap_or_else(|| condition_case_route_port(self.node_id, case));
            topology_ports.push(TopologyPort::new(case.port_id, route_port));
        }
        topology_ports.push(TopologyPort::new(
            self.else_branch.port_id,
            self.port(self.else_branch.port_id)?.clone(),
        ));
        let data = replace_cases(self.node_id, &self.data, &cases)?;
        Ok(GraphCommand::topology(
            self.version,
            TopologyEdit::new()
                .set_node_data(self.node_id, data)
                .replace_node_ports(self.node_id, topology_ports, incident_edges),
        )
        .with_label(label))
    }

    /// Returns whether valid authoritative topology already fulfills one product intent.
    #[must_use]
    pub fn satisfies(&self, intent: &ConditionIntent) -> bool {
        match intent {
            ConditionIntent::Add(case) => self.cases.iter().any(|current| current == case),
            ConditionIntent::Reorder { case_id, index } => self
                .cases
                .get(*index)
                .is_some_and(|case| case.id() == case_id),
            ConditionIntent::Remove { case_id, .. } => {
                self.cases.iter().all(|case| case.id() != case_id)
            }
        }
    }

    fn port(&self, port: PortId) -> Result<&Port, ConditionError> {
        self.ports.get(&port).ok_or(ConditionError::MissingPort {
            node: self.node_id,
            port,
        })
    }
}

/// Typed failures from parsing, validating, or planning a Condition mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConditionError {
    MissingNode { node: NodeId },
    WrongNodeKind { node: NodeId, actual: String },
    LegacyBranchesUnsupported { node: NodeId },
    InvalidData { node: NodeId, field: &'static str },
    EmptyPredicateOperand,
    InvalidPredicateLiteral { operand: String },
    EmptyCaseId,
    ReservedCaseId { case_id: String },
    EmptyCaseLabel { case_id: String },
    DuplicateCaseId { case_id: String },
    DuplicateCasePort { port: PortId },
    CasePortAlreadyExists { port: PortId },
    CaseNotFound { case_id: String },
    CaseReorderIndexOutOfBounds { index: usize, case_count: usize },
    MissingPort { node: NodeId, port: PortId },
    MissingInputPort { node: NodeId },
    InvalidInputPort { node: NodeId, port: PortId },
    MissingCasePort { node: NodeId, port: PortId },
    InvalidCasePort { node: NodeId, port: PortId },
    MissingElsePort { node: NodeId },
    InvalidElsePort { node: NodeId, port: PortId },
    InvalidPortOrder { node: NodeId },
}

impl std::fmt::Display for ConditionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode { node } => write!(formatter, "condition node is missing: {node:?}"),
            Self::WrongNodeKind { node, actual } => write!(
                formatter,
                "condition node {node:?} has kind {actual:?}, expected {CONDITION_KIND:?}"
            ),
            Self::LegacyBranchesUnsupported { node } => write!(
                formatter,
                "condition node {node:?} uses the unsupported legacy branches schema"
            ),
            Self::InvalidData { node, field } => {
                write!(
                    formatter,
                    "condition node {node:?} has invalid {field} data"
                )
            }
            Self::EmptyPredicateOperand => {
                formatter.write_str("condition predicate operand must not be empty")
            }
            Self::InvalidPredicateLiteral { operand } => write!(
                formatter,
                "condition predicate literal does not match operand {operand:?}"
            ),
            Self::EmptyCaseId => formatter.write_str("condition case id must not be empty"),
            Self::ReservedCaseId { case_id } => write!(
                formatter,
                "condition case id {case_id:?} is reserved for the immutable Else branch"
            ),
            Self::EmptyCaseLabel { case_id } => {
                write!(formatter, "condition case {case_id:?} must have a label")
            }
            Self::DuplicateCaseId { case_id } => {
                write!(formatter, "condition case id is duplicated: {case_id:?}")
            }
            Self::DuplicateCasePort { port } => {
                write!(formatter, "condition case port is duplicated: {port:?}")
            }
            Self::CasePortAlreadyExists { port } => {
                write!(formatter, "condition case port is already in use: {port:?}")
            }
            Self::CaseNotFound { case_id } => {
                write!(formatter, "condition case does not exist: {case_id:?}")
            }
            Self::CaseReorderIndexOutOfBounds { index, case_count } => write!(
                formatter,
                "condition case reorder index {index} is outside {case_count} cases"
            ),
            Self::MissingPort { node, port } => {
                write!(
                    formatter,
                    "condition node {node:?} is missing port {port:?}"
                )
            }
            Self::MissingInputPort { node } => {
                write!(
                    formatter,
                    "condition node {node:?} is missing its Input port"
                )
            }
            Self::InvalidInputPort { node, port } => write!(
                formatter,
                "condition node {node:?} has invalid Input port {port:?}"
            ),
            Self::MissingCasePort { node, port } => write!(
                formatter,
                "condition node {node:?} is missing case port {port:?}"
            ),
            Self::InvalidCasePort { node, port } => write!(
                formatter,
                "condition node {node:?} has invalid case port {port:?}"
            ),
            Self::MissingElsePort { node } => {
                write!(
                    formatter,
                    "condition node {node:?} is missing its Else port"
                )
            }
            Self::InvalidElsePort { node, port } => write!(
                formatter,
                "condition node {node:?} has invalid Else port {port:?}"
            ),
            Self::InvalidPortOrder { node } => write!(
                formatter,
                "condition node {node:?} must order Input, cases, then Else"
            ),
        }
    }
}

impl std::error::Error for ConditionError {}

fn collect_node_ports(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
    node: &Node,
) -> Result<BTreeMap<PortId, Port>, ConditionError> {
    node.ports
        .iter()
        .map(|port_id| {
            snapshot
                .graph()
                .port(port_id)
                .cloned()
                .map(|port| (*port_id, port))
                .ok_or(ConditionError::MissingPort {
                    node: node_id,
                    port: *port_id,
                })
        })
        .collect()
}

fn parse_cases(
    node_id: NodeId,
    data: &serde_json::Value,
) -> Result<Vec<ConditionCase>, ConditionError> {
    if data.get("branches").is_some() {
        return Err(ConditionError::LegacyBranchesUnsupported { node: node_id });
    }
    let cases = data
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or(ConditionError::InvalidData {
            node: node_id,
            field: "cases",
        })?;
    let cases = cases
        .iter()
        .map(|case| {
            let id = case.get("id").and_then(serde_json::Value::as_str).ok_or(
                ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].id",
                },
            )?;
            let label = case
                .get("label")
                .and_then(serde_json::Value::as_str)
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].label",
                })?;
            let port_id = case
                .get("port_id")
                .cloned()
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].port_id",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| ConditionError::InvalidData {
                        node: node_id,
                        field: "cases[].port_id",
                    })
                })?;
            let value_port_id = case
                .get("value_port_id")
                .cloned()
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].value_port_id",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| ConditionError::InvalidData {
                        node: node_id,
                        field: "cases[].value_port_id",
                    })
                })?;
            let predicate = case
                .get("predicate")
                .and_then(serde_json::Value::as_object)
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].predicate",
                })?;
            let operand = predicate
                .get("operand")
                .and_then(serde_json::Value::as_str)
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].predicate.operand",
                })?;
            let operator = predicate
                .get("operator")
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].predicate.operator",
                })
                .and_then(|value| ConditionOperator::parse(node_id, value))?;
            let literal = predicate
                .get("literal")
                .cloned()
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].predicate.literal",
                })?;
            let value_type = predicate
                .get("value_type")
                .cloned()
                .ok_or(ConditionError::InvalidData {
                    node: node_id,
                    field: "cases[].predicate.value_type",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| ConditionError::InvalidData {
                        node: node_id,
                        field: "cases[].predicate.value_type",
                    })
                })?;
            let parsed_predicate = match operator {
                ConditionOperator::Equals => {
                    ConditionPredicate::equals(operand, literal, value_type)?
                }
            };
            ConditionCase::new(id, label, port_id, value_port_id, parsed_predicate)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_cases(&cases)?;
    Ok(cases)
}

fn parse_else_branch(
    node_id: NodeId,
    data: &serde_json::Value,
    ports: &BTreeMap<PortId, Port>,
) -> Result<ConditionElseBranch, ConditionError> {
    let label = data
        .get("else_branch")
        .and_then(|value| value.get("label"))
        .and_then(serde_json::Value::as_str)
        .filter(|label| *label == "Else")
        .ok_or(ConditionError::InvalidData {
            node: node_id,
            field: "else_branch.label",
        })?;
    let mut matching = ports
        .iter()
        .filter(|(_, port)| port.node == node_id && port.key.0 == CONDITION_ELSE_PORT_KEY);
    let Some((port_id, _)) = matching.next() else {
        return Err(ConditionError::MissingElsePort { node: node_id });
    };
    if matching.next().is_some() {
        return Err(ConditionError::InvalidElsePort {
            node: node_id,
            port: *port_id,
        });
    }
    Ok(ConditionElseBranch {
        label: label.to_owned(),
        port_id: *port_id,
    })
}

fn validate_unique_cases(cases: &[ConditionCase]) -> Result<(), ConditionError> {
    let mut ids = BTreeSet::new();
    let mut ports = BTreeSet::new();
    for case in cases {
        if !ids.insert(case.id.as_str()) {
            return Err(ConditionError::DuplicateCaseId {
                case_id: case.id.clone(),
            });
        }
        if !ports.insert(case.port_id) {
            return Err(ConditionError::DuplicateCasePort { port: case.port_id });
        }
        if !ports.insert(case.value_port_id) {
            return Err(ConditionError::DuplicateCasePort {
                port: case.value_port_id,
            });
        }
    }
    Ok(())
}

fn validate_topology(
    node_id: NodeId,
    node: &Node,
    cases: &[ConditionCase],
    else_branch: &ConditionElseBranch,
    ports: &BTreeMap<PortId, Port>,
) -> Result<PortId, ConditionError> {
    let mut inputs = ports
        .iter()
        .filter(|(_, port)| port.node == node_id && port.key.0 == CONDITION_INPUT_PORT_KEY);
    let Some((input_port_id, input_port)) = inputs.next() else {
        return Err(ConditionError::MissingInputPort { node: node_id });
    };
    if inputs.next().is_some() || !input_port_matches(node_id, input_port) {
        return Err(ConditionError::InvalidInputPort {
            node: node_id,
            port: *input_port_id,
        });
    }

    for case in cases {
        let value_port = ports
            .get(&case.value_port_id)
            .ok_or(ConditionError::MissingCasePort {
                node: node_id,
                port: case.value_port_id,
            })?;
        if !case_value_port_matches(node_id, value_port, case) {
            return Err(ConditionError::InvalidCasePort {
                node: node_id,
                port: case.value_port_id,
            });
        }
        let route_port = ports
            .get(&case.port_id)
            .ok_or(ConditionError::MissingCasePort {
                node: node_id,
                port: case.port_id,
            })?;
        if !case_route_port_matches(node_id, route_port, case) {
            return Err(ConditionError::InvalidCasePort {
                node: node_id,
                port: case.port_id,
            });
        }
    }

    let else_port = ports
        .get(&else_branch.port_id)
        .ok_or(ConditionError::MissingElsePort { node: node_id })?;
    if !else_port_matches(node_id, else_port)
        || cases.iter().any(|case| case.port_id == else_branch.port_id)
    {
        return Err(ConditionError::InvalidElsePort {
            node: node_id,
            port: else_branch.port_id,
        });
    }

    let expected_ports = std::iter::once(*input_port_id)
        .chain(
            cases
                .iter()
                .flat_map(|case| [case.value_port_id(), case.port_id()]),
        )
        .chain(std::iter::once(else_branch.port_id))
        .collect::<Vec<_>>();
    if node.ports != expected_ports
        || ports.len() != expected_ports.len()
        || expected_ports
            .iter()
            .any(|port_id| !ports.contains_key(port_id))
    {
        return Err(ConditionError::InvalidPortOrder { node: node_id });
    }
    Ok(*input_port_id)
}

fn input_port_matches(node_id: NodeId, port: &Port) -> bool {
    port.node == node_id
        && port.key.0 == CONDITION_INPUT_PORT_KEY
        && port.dir == PortDirection::In
        && port.kind == PortKind::Exec
        && port.capacity == PortCapacity::Single
        && port.ty.is_none()
}

fn case_route_port_matches(node_id: NodeId, port: &Port, case: &ConditionCase) -> bool {
    port.node == node_id
        && port.key == condition_case_key(case.id())
        && port.dir == PortDirection::Out
        && port.kind == PortKind::Exec
        && port.capacity == PortCapacity::Multi
        && port.ty.is_none()
        && port.data.get("case_id").and_then(serde_json::Value::as_str) == Some(case.id())
        && port.data.get("label").and_then(serde_json::Value::as_str) == Some(case.label())
}

fn case_value_port_matches(node_id: NodeId, port: &Port, case: &ConditionCase) -> bool {
    port.node == node_id
        && port.key == condition_case_value_key(case.id())
        && port.dir == PortDirection::In
        && port.kind == PortKind::Data
        && port.capacity == PortCapacity::Single
        && port.ty.as_ref() == Some(case.value_type())
        && port.data.get("case_id").and_then(serde_json::Value::as_str) == Some(case.id())
        && port.data.get("label").and_then(serde_json::Value::as_str) == Some(case.label())
}

fn else_port_matches(node_id: NodeId, port: &Port) -> bool {
    port.node == node_id
        && port.key.0 == CONDITION_ELSE_PORT_KEY
        && port.dir == PortDirection::Out
        && port.kind == PortKind::Exec
        && port.capacity == PortCapacity::Multi
        && port.ty.is_none()
}

fn ensure_kind(node_id: NodeId, node: &Node) -> Result<(), ConditionError> {
    if node.kind.0 == CONDITION_KIND {
        Ok(())
    } else {
        Err(ConditionError::WrongNodeKind {
            node: node_id,
            actual: node.kind.0.clone(),
        })
    }
}

/// Returns the canonical graph data for a newly created empty Condition.
#[must_use]
pub fn condition_default_data() -> serde_json::Value {
    condition_data("Condition", &[])
}

pub(crate) fn condition_data(title: &str, cases: &[ConditionCase]) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "cases": cases
            .iter()
            .map(|case| serde_json::json!({
                "id": case.id,
                "label": case.label,
                "port_id": case.port_id,
                "value_port_id": case.value_port_id,
                "predicate": {
                    "operand": case.predicate.operand,
                    "operator": case.predicate.operator.key(),
                    "literal": case.predicate.literal,
                    "value_type": case.predicate.value_type,
                },
            }))
            .collect::<Vec<_>>(),
        "else_branch": { "label": "Else" },
    })
}

fn replace_cases(
    node_id: NodeId,
    data: &serde_json::Value,
    cases: &[ConditionCase],
) -> Result<serde_json::Value, ConditionError> {
    let mut value = data.clone();
    let object = value.as_object_mut().ok_or(ConditionError::InvalidData {
        node: node_id,
        field: "object",
    })?;
    object.insert(
        "cases".to_owned(),
        serde_json::Value::Array(
            cases
                .iter()
                .map(|case| {
                    serde_json::json!({
                        "id": case.id,
                        "label": case.label,
                        "port_id": case.port_id,
                        "value_port_id": case.value_port_id,
                        "predicate": {
                            "operand": case.predicate.operand,
                            "operator": case.predicate.operator.key(),
                            "literal": case.predicate.literal,
                            "value_type": case.predicate.value_type,
                        },
                    })
                })
                .collect(),
        ),
    );
    Ok(value)
}

pub(crate) fn condition_case_route_port(node: NodeId, case: &ConditionCase) -> Port {
    Port {
        node,
        key: condition_case_key(case.id()),
        dir: PortDirection::Out,
        kind: PortKind::Exec,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::json!({
            "case_id": case.id,
            "label": case.label,
        }),
    }
}

pub(crate) fn condition_case_value_port(node: NodeId, case: &ConditionCase) -> Port {
    Port {
        node,
        key: condition_case_value_key(case.id()),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(case.value_type().clone()),
        data: serde_json::json!({
            "case_id": case.id,
            "label": case.label,
        }),
    }
}

fn literal_matches_type(value: &serde_json::Value, ty: &TypeDesc) -> bool {
    match ty {
        TypeDesc::Any | TypeDesc::Unknown | TypeDesc::Var { .. } | TypeDesc::Opaque { .. } => true,
        TypeDesc::Never => false,
        TypeDesc::Null => value.is_null(),
        TypeDesc::Bool => value.is_boolean(),
        TypeDesc::Int => value.as_i64().is_some() || value.as_u64().is_some(),
        TypeDesc::Float => value.is_number(),
        TypeDesc::String | TypeDesc::Bytes => value.is_string(),
        TypeDesc::List { of } => value
            .as_array()
            .is_some_and(|values| values.iter().all(|value| literal_matches_type(value, of))),
        TypeDesc::Map { key, value: item } => {
            matches!(
                key.as_ref(),
                TypeDesc::Any
                    | TypeDesc::Unknown
                    | TypeDesc::String
                    | TypeDesc::Var { .. }
                    | TypeDesc::Opaque { .. }
            ) && value.as_object().is_some_and(|values| {
                values
                    .values()
                    .all(|value| literal_matches_type(value, item))
            })
        }
        TypeDesc::Object { fields, open } => value.as_object().is_some_and(|object| {
            fields.iter().all(|(field, ty)| {
                object
                    .get(field)
                    .is_some_and(|value| literal_matches_type(value, ty))
            }) && (*open || object.keys().all(|field| fields.contains_key(field)))
        }),
        TypeDesc::Union { types } => types.iter().any(|ty| literal_matches_type(value, ty)),
        TypeDesc::Option { of } => value.is_null() || literal_matches_type(value, of),
    }
}

fn condition_case_key(case_id: &str) -> PortKey {
    PortKey::new(format!("case.route.{case_id}"))
}

fn condition_case_value_key(case_id: &str) -> PortKey {
    PortKey::new(format!("case.value.{case_id}"))
}
