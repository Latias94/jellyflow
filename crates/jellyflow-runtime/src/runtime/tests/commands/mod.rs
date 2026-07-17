use std::cell::Cell;
use std::rc::Rc;

use crate::graph_kernel::GraphOp;
use jellyflow_core::core::{
    Binding, BindingId, CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder,
    GraphId, GraphLocalBindingTarget, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection,
    PortId, PortKey, PortKind, SYMBOL_REF_NODE_KIND, Symbol, SymbolId, symbol_ref_node_data,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{TypeCompatibility, TypeCompatibilityResult, TypeDesc};

use crate::io::GraphFile;
use crate::profile::{
    ApplyPipelineError, GraphProfile, PortTypeImpactContext, PortTypeRevalidation,
};
use crate::rules::{Diagnostic, DiagnosticTarget, EdgeEndpoint};
use crate::runtime::commands::{
    ConnectionEdit, GraphCommand, GraphCommandOutcome, IncidentEdgePolicy, NodeGeometryEdit,
    SymbolEdit, TopologyEdit, TopologyPort, plan_graph_command,
};
use crate::runtime::graph_runtime::{
    GraphIndex, GraphRuntime, GraphRuntimeError, GraphRuntimePolicy,
};
use crate::schema::{NodeCatalog, NodeCatalogMigrationError, NodeKindSpec};

use super::fixtures::GraphFixtureMutExt;
use super::view_session::fixtures::{RoutedGraphIds, graph_with_nodes, graph_with_routed_edge};

mod conflict_history;
mod connection;
mod geometry;
mod layout;
mod profile;
mod scale;
mod symbols;
mod topology;

fn profiled_runtime(
    graph: Graph,
    profile: Box<dyn GraphProfile>,
) -> Result<GraphRuntime, GraphRuntimeError> {
    GraphRuntime::builder(Default::default())
        .with_profile(profile)
        .build(graph)
}

fn canonical_add_catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("math.add", "Add", "math.add.native")
                .latest_kind_version(2)
                .alias("legacy.add")
                .build(),
        )
        .unwrap();
    catalog
}

fn node_with_kind(mut node: Node, kind: &str, version: u32) -> Node {
    node.kind = NodeKindKey::new(kind);
    node.kind_version = version;
    node
}

fn port(node: jellyflow_core::core::NodeId, key: &str, dir: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn assert_rejected_without_side_effects(
    runtime: &GraphRuntime,
    command: GraphCommand,
    expected_key: &str,
) -> GraphCommandOutcome {
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let stats_before = runtime.stats().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let outcome = runtime.execute(command).unwrap();

    let rejection = outcome.rejection().expect("command is rejected");
    assert_eq!(rejection.diagnostics()[0].key, expected_key);
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(runtime.stats().unwrap(), stats_before);
    assert_eq!(event_count.get(), 0);
    outcome
}

fn assert_profile_error_key(error: GraphRuntimeError, expected_key: &str) {
    let GraphRuntimeError::Apply(ApplyPipelineError::Rejected { diagnostics, .. }) = error else {
        panic!("expected a profile rejection, got {error:?}");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.key == expected_key),
        "missing profile diagnostic `{expected_key}`: {diagnostics:?}",
    );
}

fn frozen_graph_value(graph: &Graph) -> serde_json::Value {
    serde_json::to_value(GraphFile::from_graph(graph.clone())).expect("graph file serializes")
}

fn string_typed_routed_graph() -> (Graph, RoutedGraphIds) {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.source_port)
                .map(|port| port.ty = Some(TypeDesc::String))
        })
        .expect("source port");
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.ty = Some(TypeDesc::String))
        })
        .expect("target port");
    (graph, ids)
}

fn topology_ports_with_type(
    graph: &Graph,
    node: NodeId,
    typed_port: PortId,
    ty: TypeDesc,
) -> Vec<TopologyPort> {
    graph
        .node(&node)
        .expect("topology node")
        .ports
        .iter()
        .map(|id| {
            let mut port = graph.port(id).expect("topology port").clone();
            if *id == typed_port {
                port.ty = Some(ty.clone());
            }
            TopologyPort::new(*id, port)
        })
        .collect()
}

fn undo(runtime: &GraphRuntime) {
    let revision = runtime.snapshot().unwrap().revision();
    runtime
        .execute(GraphCommand::undo(revision))
        .unwrap()
        .applied()
        .expect("undo applies");
}

fn redo(runtime: &GraphRuntime) {
    let revision = runtime.snapshot().unwrap().revision();
    runtime
        .execute(GraphCommand::redo(revision))
        .unwrap()
        .applied()
        .expect("redo applies");
}

struct PassThroughProfile;

struct FixedPortTypesProfile {
    string_port: PortId,
    int_port: PortId,
}

struct PortDataDependentTypesProfile {
    trigger: PortId,
    dependent: PortId,
    string_target: PortId,
}

impl GraphProfile for PortDataDependentTypesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        if port == self.dependent {
            let enabled = graph
                .port(&self.trigger)
                .is_some_and(|port| port.data == serde_json::Value::Bool(true));
            return Some(if enabled {
                TypeDesc::Int
            } else {
                TypeDesc::String
            });
        }
        if port == self.string_target || port == self.trigger {
            return Some(TypeDesc::String);
        }
        None
    }

    fn dependent_port_types(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
        _changed_port: PortId,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(Vec::new())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

struct ConnectionDependentTypesProfile {
    dependent: PortId,
}

struct ChainedPortTypesProfile {
    trigger: PortId,
    intermediate: PortId,
    dependent: PortId,
    string_target: PortId,
}

struct MissingPortDependencyProfile {
    missing: PortId,
}

struct RemovedPortDependentTypesProfile {
    trigger: PortId,
    dependent: PortId,
    string_target: PortId,
}

impl GraphProfile for RemovedPortDependentTypesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        if port == self.dependent {
            return Some(if graph.port(&self.trigger).is_some() {
                TypeDesc::String
            } else {
                TypeDesc::Int
            });
        }
        if port == self.string_target || port == self.trigger {
            return Some(TypeDesc::String);
        }
        None
    }

    fn port_types_affected_by_mutation(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(Vec::new())
    }

    fn dependent_port_types(
        &self,
        context: &mut PortTypeImpactContext<'_>,
        changed_port: PortId,
    ) -> PortTypeRevalidation {
        if changed_port == self.trigger && context.footprint().ports.contains(&self.trigger) {
            PortTypeRevalidation::Exact(vec![self.dependent])
        } else {
            PortTypeRevalidation::Exact(Vec::new())
        }
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

impl GraphProfile for ConnectionDependentTypesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        if port == self.dependent {
            return Some(if graph.edges().len() > 1 {
                TypeDesc::Int
            } else {
                TypeDesc::String
            });
        }
        Some(TypeDesc::String)
    }

    fn port_types_affected_by_mutation(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(Vec::new())
    }

    fn dependent_port_types(
        &self,
        context: &mut PortTypeImpactContext<'_>,
        changed_port: PortId,
    ) -> PortTypeRevalidation {
        let topology_changed = !context.footprint().edges.is_empty()
            && context.incident_edge_ids_before(changed_port)
                != context.incident_edge_ids_after(changed_port);
        if topology_changed {
            PortTypeRevalidation::Exact(vec![self.dependent])
        } else {
            PortTypeRevalidation::Exact(Vec::new())
        }
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

impl GraphProfile for ChainedPortTypesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        if port == self.dependent {
            let enabled = graph
                .port(&self.trigger)
                .is_some_and(|port| port.data == serde_json::Value::Bool(true));
            return Some(if enabled {
                TypeDesc::Int
            } else {
                TypeDesc::String
            });
        }
        if port == self.trigger || port == self.intermediate || port == self.string_target {
            return Some(TypeDesc::String);
        }
        None
    }

    fn dependent_port_types(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
        changed_port: PortId,
    ) -> PortTypeRevalidation {
        if changed_port == self.trigger {
            PortTypeRevalidation::Exact(vec![self.intermediate])
        } else if changed_port == self.intermediate {
            PortTypeRevalidation::Exact(vec![self.dependent])
        } else {
            PortTypeRevalidation::Exact(Vec::new())
        }
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

impl GraphProfile for MissingPortDependencyProfile {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn dependent_port_types(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
        _changed_port: PortId,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(vec![self.missing])
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

struct CountingGraphTypesProfile {
    query_count: Rc<Cell<usize>>,
}

struct StoredPortTypesProfile;

struct AcceptAllTypeCompatibility;

impl TypeCompatibility for AcceptAllTypeCompatibility {
    fn compatible(&self, _from: &TypeDesc, _to: &TypeDesc) -> TypeCompatibilityResult {
        TypeCompatibilityResult::Compatible
    }
}

struct AcceptAllStoredTypesProfile {
    compatibility: AcceptAllTypeCompatibility,
}

impl GraphProfile for AcceptAllStoredTypesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn type_compatibility(&self) -> &dyn TypeCompatibility {
        &self.compatibility
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

impl GraphProfile for StoredPortTypesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

impl GraphProfile for CountingGraphTypesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        self.query_count
            .set(self.query_count.get().saturating_add(1));
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

impl GraphProfile for FixedPortTypesProfile {
    fn type_of_port(&self, _graph: &Graph, port: PortId) -> Option<TypeDesc> {
        if port == self.string_port {
            Some(TypeDesc::String)
        } else if port == self.int_port {
            Some(TypeDesc::Int)
        } else {
            None
        }
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

impl GraphProfile for PassThroughProfile {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn port_types_affected_by_mutation(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(Vec::new())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

struct NoDataCyclesProfile;

impl GraphProfile for NoDataCyclesProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn allow_cycles(&self, edge_kind: EdgeKind) -> bool {
        edge_kind != EdgeKind::Data
    }
}

struct DerivePortOnce {
    node: jellyflow_core::core::NodeId,
    port: PortId,
    concretize_calls: Rc<Cell<usize>>,
}

struct DerivePayloadThenPort {
    node: jellyflow_core::core::NodeId,
    port: PortId,
    concretize_calls: Rc<Cell<usize>>,
}

impl GraphProfile for DerivePayloadThenPort {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn port_types_affected_by_mutation(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(Vec::new())
    }

    fn dependent_port_types(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
        _changed_port: PortId,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(Vec::new())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn concretize(&self, graph: &Graph) -> TopologyEdit {
        self.concretize_calls
            .set(self.concretize_calls.get().saturating_add(1));
        let node = graph.node(&self.node).expect("profile node exists");
        if node.data.get("derived") != Some(&serde_json::Value::Bool(true)) {
            return TopologyEdit::new().set_node_data(
                self.node,
                serde_json::json!({ "value": node.data["value"], "derived": true }),
            );
        }
        if graph.port(&self.port).is_none() {
            return TopologyEdit::new().replace_node_ports(
                self.node,
                [TopologyPort::new(
                    self.port,
                    port(self.node, "derived", PortDirection::Out),
                )],
                IncidentEdgePolicy::Reject,
            );
        }
        TopologyEdit::new()
    }
}

struct RejectDerivedPort {
    node: jellyflow_core::core::NodeId,
    port: PortId,
}

struct RejectNullNodeDataWhenEnabled {
    node: jellyflow_core::core::NodeId,
    reject_null: Rc<Cell<bool>>,
}

impl GraphProfile for RejectNullNodeDataWhenEnabled {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic> {
        let data = &graph.node(&self.node).expect("profile node exists").data;
        if self.reject_null.get() && data.is_null() {
            return vec![Diagnostic::error(
                "profile.history_revert_forbidden",
                crate::rules::DiagnosticTarget::Node { id: self.node },
                "reverting node data is forbidden",
            )];
        }
        Vec::new()
    }
}

impl GraphProfile for RejectDerivedPort {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic> {
        if graph.port(&self.port).is_some() {
            vec![Diagnostic::error(
                "profile.derived_port_forbidden",
                crate::rules::DiagnosticTarget::Port { id: self.port },
                "derived port is forbidden",
            )]
        } else {
            Vec::new()
        }
    }

    fn concretize(&self, graph: &Graph) -> TopologyEdit {
        if graph.port(&self.port).is_some() {
            TopologyEdit::new()
        } else {
            TopologyEdit::new().replace_node_ports(
                self.node,
                [TopologyPort::new(
                    self.port,
                    port(self.node, "derived", PortDirection::Out),
                )],
                IncidentEdgePolicy::Reject,
            )
        }
    }
}

struct OscillatingProfile {
    node: jellyflow_core::core::NodeId,
    concretize_calls: Rc<Cell<usize>>,
}

impl GraphProfile for OscillatingProfile {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn concretize_bound(&self) -> usize {
        3
    }

    fn concretize(&self, graph: &Graph) -> TopologyEdit {
        self.concretize_calls
            .set(self.concretize_calls.get().saturating_add(1));
        let next = graph.node(&self.node).and_then(|node| node.data.as_bool()) != Some(true);
        TopologyEdit::new().set_node_data(self.node, serde_json::Value::Bool(next))
    }
}

impl GraphProfile for DerivePortOnce {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn concretize(&self, graph: &Graph) -> TopologyEdit {
        self.concretize_calls
            .set(self.concretize_calls.get().saturating_add(1));
        if graph.port(&self.port).is_some() {
            return TopologyEdit::new();
        }

        TopologyEdit::new().replace_node_ports(
            self.node,
            [TopologyPort::new(
                self.port,
                port(self.node, "derived", PortDirection::Out),
            )],
            IncidentEdgePolicy::Reject,
        )
    }
}
