use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    GraphValidationError, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind, SYMBOL_REF_NODE_KIND,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{
    DefaultTypeCompatibility, TypeCompatibility, TypeCompatibilityResult, TypeDesc,
};
use serde_json::json;

use super::{DummyMigrator, demo_add_node};
use crate::io::PersistenceLimits;
use crate::profile::{ApplyPipelineError, GraphProfile};
use crate::rules::{Diagnostic, DiagnosticTarget};
use crate::runtime::commands::{ConnectionEdit, GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::{CreateFromWireQueryError, GraphRuntime, GraphRuntimeError};
use crate::schema::{
    NodeCatalog, NodeCatalogInstantiationError, NodeCatalogMigrationError,
    NodeCatalogRegistrationError, NodeCreation, NodeKindMigrateError, NodeKindMigrationIssue,
    NodeKindMigrator, NodeKindSpec, NodePortSpec,
};

const TEST_NODE_SIZE: CanvasSize = CanvasSize {
    width: 180.0,
    height: 96.0,
};

fn test_bounds(origin: CanvasPoint) -> CanvasRect {
    CanvasRect {
        origin,
        size: TEST_NODE_SIZE,
    }
}

struct FailingMigrator;

impl NodeKindMigrator for FailingMigrator {
    fn migrate(
        &self,
        _from_version: u32,
        _to_version: u32,
        _data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        Err(NodeKindMigrateError::message("fixture migration failed"))
    }
}

struct CountingMigrator {
    calls: Arc<AtomicUsize>,
}

impl NodeKindMigrator for CountingMigrator {
    fn migrate(
        &self,
        _from_version: u32,
        _to_version: u32,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut data = data.clone();
        data["migrated"] = json!(true);
        Ok(data)
    }
}

struct RejectCanonicalGraphProfile;

struct RejectingStoredTypesProfile {
    compatibility: RejectResolvedTypes,
}

impl GraphProfile for RejectingStoredTypesProfile {
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

impl GraphProfile for RejectCanonicalGraphProfile {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        vec![Diagnostic::error(
            "test.canonical.rejected",
            DiagnosticTarget::Graph,
            "canonical graph rejected by profile",
        )]
    }

    fn allow_cycles(&self, _edge_kind: EdgeKind) -> bool {
        true
    }
}

fn add_spec(kind: &str, latest_kind_version: u32) -> NodeKindSpec {
    NodeKindSpec::builder(kind)
        .latest_kind_version(latest_kind_version)
        .port(
            NodePortSpec::new(
                "lhs",
                PortDirection::In,
                PortKind::Data,
                PortCapacity::Single,
            )
            .with_type(TypeDesc::Float)
            .with_default_data(json!({ "slot": "lhs" })),
        )
        .default_data(json!({ "lhs": 0.0 }))
        .build()
}

fn source_port(direction: PortDirection, kind: PortKind, ty: Option<TypeDesc>) -> Port {
    Port {
        node: NodeId::from_u128(90),
        key: PortKey::new("source"),
        dir: direction,
        kind,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty,
        data: serde_json::Value::Null,
    }
}

fn graph_with_source_port(port_id: PortId, mut port: Port, connectable: bool) -> Graph {
    let node_id = port.node;
    let mut node = demo_add_node("source.node", 1, serde_json::Value::Null);
    node.ports = vec![port_id];
    node.connectable = Some(connectable);
    port.node = node_id;
    GraphBuilder::new(GraphId::from_u128(13))
        .with_node(node_id, node)
        .with_port(port_id, port)
        .build()
        .unwrap()
}

struct RejectResolvedTypes;

impl TypeCompatibility for RejectResolvedTypes {
    fn compatible(&self, _from: &TypeDesc, _to: &TypeDesc) -> TypeCompatibilityResult {
        TypeCompatibilityResult::Incompatible {
            reason: "test policy rejected resolved types".to_owned(),
        }
    }
}

#[test]
fn semantic_catalog_resolves_aliases_and_instantiates_real_ports() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(add_spec("math.add", 2).with_alias("legacy.add"))
        .unwrap();

    let alias = NodeKindKey::new("legacy.add");
    let spec = catalog
        .get(&alias)
        .expect("alias resolves to registered kind");
    assert_eq!(spec.kind(), &NodeKindKey::new("math.add"));
    assert_eq!(spec.latest_kind_version(), 2);
    assert_eq!(spec.ports().len(), 1);

    let node_id = NodeId::from_u128(1);
    let port_id = PortId::from_u128(2);
    let creation = catalog
        .instantiate_node_with_ids(
            &alias,
            node_id,
            test_bounds(CanvasPoint { x: 24.0, y: 48.0 }),
            [port_id],
        )
        .unwrap();

    assert_eq!(creation.node_id(), node_id);
    assert_eq!(creation.node().kind, NodeKindKey::new("math.add"));
    assert_eq!(creation.node().kind_version, 2);
    assert_eq!(creation.node().size, Some(TEST_NODE_SIZE));
    assert_eq!(creation.node().data, json!({ "lhs": 0.0 }));
    assert_eq!(creation.node().ports, [port_id]);
    assert_eq!(creation.ports().len(), 1);
    let port = &creation.ports()[0].1;
    assert_eq!(port.node, node_id);
    assert_eq!(port.dir, PortDirection::In);
    assert_eq!(port.kind, PortKind::Data);
    assert_eq!(port.capacity, PortCapacity::Single);
    assert_eq!(port.ty, Some(TypeDesc::Float));
    assert_eq!(port.data, json!({ "slot": "lhs" }));
}

#[test]
fn semantic_catalog_rejects_duplicate_supplied_port_ids() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("math.binary")
                .port(NodePortSpec::new(
                    "lhs",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .port(NodePortSpec::new(
                    "rhs",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .unwrap();
    let duplicate = PortId::from_u128(72);

    assert!(matches!(
        catalog.instantiate_node_with_ids(
            &NodeKindKey::new("math.binary"),
            NodeId::from_u128(71),
            test_bounds(CanvasPoint::default()),
            [duplicate, duplicate],
        ),
        Err(NodeCatalogInstantiationError::DuplicatePortId { port }) if port == duplicate
    ));
}

#[test]
fn semantic_catalog_requires_valid_product_supplied_initial_bounds() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 1)).unwrap();
    let kind = NodeKindKey::new("math.add");
    let invalid = [
        CanvasRect {
            origin: CanvasPoint {
                x: f32::NAN,
                y: 0.0,
            },
            size: TEST_NODE_SIZE,
        },
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 0.0,
                height: 96.0,
            },
        },
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 180.0,
                height: f32::INFINITY,
            },
        },
    ];

    for bounds in invalid {
        assert_eq!(
            catalog.instantiate_node(&kind, bounds).unwrap_err(),
            NodeCatalogInstantiationError::InvalidInitialBounds
        );
    }
}

#[test]
fn semantic_port_default_data_is_cloned_per_creation() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 1)).unwrap();
    let kind = NodeKindKey::new("math.add");
    let first = catalog
        .instantiate_node(&kind, test_bounds(CanvasPoint::default()))
        .unwrap();
    let (_, _, mut first_ports) = first.into_parts();
    first_ports[0].1.data["slot"] = json!("changed");

    let second = catalog
        .instantiate_node(&kind, test_bounds(CanvasPoint::default()))
        .unwrap();
    assert_eq!(second.ports()[0].1.data, json!({ "slot": "lhs" }));
}

#[test]
fn catalog_registration_conflicts_never_depend_on_registration_order() {
    let mut alias_first = NodeCatalog::new();
    alias_first
        .register(add_spec("math.add", 1).with_alias("math.shared"))
        .unwrap();
    assert!(matches!(
        alias_first.register(add_spec("math.shared", 1)),
        Err(NodeCatalogRegistrationError::KindConflictsWithAlias { kind, canonical })
            if kind == NodeKindKey::new("math.shared")
                && canonical == NodeKindKey::new("math.add")
    ));
    assert_eq!(alias_first.kinds().count(), 1);
    assert_eq!(
        alias_first
            .get(&NodeKindKey::new("math.shared"))
            .unwrap()
            .kind(),
        &NodeKindKey::new("math.add")
    );

    let mut kind_first = NodeCatalog::new();
    kind_first.register(add_spec("math.shared", 1)).unwrap();
    assert!(matches!(
        kind_first.register(add_spec("math.add", 1).with_alias("math.shared")),
        Err(NodeCatalogRegistrationError::AliasConflict { alias })
            if alias == NodeKindKey::new("math.shared")
    ));
    assert_eq!(kind_first.kinds().count(), 1);
    assert_eq!(
        kind_first
            .get(&NodeKindKey::new("math.shared"))
            .unwrap()
            .kind(),
        &NodeKindKey::new("math.shared")
    );

    assert!(matches!(
        kind_first.register(add_spec("math.shared", 1)),
        Err(NodeCatalogRegistrationError::DuplicateKind { kind })
            if kind == NodeKindKey::new("math.shared")
    ));

    let duplicate_ports = NodeKindSpec::builder("math.duplicate")
        .port(NodePortSpec::new(
            "value",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        ))
        .port(NodePortSpec::new(
            "value",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ))
        .build();
    assert!(matches!(
        kind_first.register(duplicate_ports),
        Err(NodeCatalogRegistrationError::DuplicatePortKey { kind, port })
            if kind == NodeKindKey::new("math.duplicate") && port == PortKey::new("value")
    ));

    kind_first
        .register_migrator(NodeKindKey::new("math.shared"), Arc::new(DummyMigrator))
        .unwrap();
    assert!(matches!(
        kind_first.register_migrator(NodeKindKey::new("math.shared"), Arc::new(DummyMigrator)),
        Err(NodeCatalogRegistrationError::DuplicateMigrator { kind })
            if kind == NodeKindKey::new("math.shared")
    ));
}

#[test]
fn catalog_creation_can_only_publish_through_a_revisioned_graph_command() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 1)).unwrap();
    let node_id = NodeId::from_u128(1);
    let port_id = PortId::from_u128(2);
    let bounds = test_bounds(CanvasPoint { x: 24.0, y: 48.0 });
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(Graph::new(GraphId::from_u128(12)))
        .unwrap();
    let stale_version = runtime.snapshot().unwrap().version();

    let stale_creation: NodeCreation = catalog
        .instantiate_node_with_ids(&NodeKindKey::new("math.add"), node_id, bounds, [port_id])
        .unwrap();
    let prefix = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new("math.add"),
            NodeId::from_u128(3),
            test_bounds(CanvasPoint::default()),
            [PortId::from_u128(4)],
        )
        .unwrap();
    let prefix = runtime
        .execute(prefix.into_command(stale_version))
        .into_result()
        .unwrap();
    let current_version = prefix
        .applied()
        .expect("prefix advances the authoritative graph")
        .version();
    let stale = runtime
        .execute(stale_creation.into_command(stale_version))
        .into_result()
        .unwrap();

    assert!(stale.conflict().is_some());
    assert!(runtime.snapshot().unwrap().graph().node(&node_id).is_none());

    let creation = catalog
        .instantiate_node_with_ids(&NodeKindKey::new("math.add"), node_id, bounds, [port_id])
        .unwrap();
    let applied = runtime
        .execute(creation.into_command(current_version))
        .into_result()
        .unwrap();

    assert!(applied.applied().is_some());
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(snapshot.graph().node(&node_id).unwrap().ports, [port_id]);
    assert_eq!(snapshot.graph().port(&port_id).unwrap().node, node_id);
}

#[test]
fn create_from_wire_candidates_share_direction_kind_and_type_semantics() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("z.last")
                .port(
                    NodePortSpec::new(
                        "input",
                        PortDirection::In,
                        PortKind::Data,
                        PortCapacity::Single,
                    )
                    .with_type(TypeDesc::Int),
                )
                .build(),
        )
        .unwrap();
    catalog
        .register(
            NodeKindSpec::builder("a.first")
                .port(
                    NodePortSpec::new(
                        "compatible",
                        PortDirection::In,
                        PortKind::Data,
                        PortCapacity::Single,
                    )
                    .with_type(TypeDesc::Float),
                )
                .port(
                    NodePortSpec::new(
                        "type_mismatch",
                        PortDirection::In,
                        PortKind::Data,
                        PortCapacity::Single,
                    )
                    .with_type(TypeDesc::String),
                )
                .port(NodePortSpec::new(
                    "kind_mismatch",
                    PortDirection::In,
                    PortKind::Exec,
                    PortCapacity::Single,
                ))
                .port(
                    NodePortSpec::new(
                        "direction_mismatch",
                        PortDirection::Out,
                        PortKind::Data,
                        PortCapacity::Multi,
                    )
                    .with_type(TypeDesc::Int),
                )
                .port(NodePortSpec::new(
                    "unresolved_type",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .unwrap();
    let source = source_port(PortDirection::Out, PortKind::Data, Some(TypeDesc::Int));
    let compatibility = DefaultTypeCompatibility;

    let candidates = catalog.create_from_wire_candidates(
        &source,
        NodeGraphConnectionMode::Strict,
        &compatibility,
    );
    let keys = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.node_kind().0.as_str(),
                candidate.kind_version(),
                candidate.port().0.as_str(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        [
            ("a.first", 1, "compatible"),
            ("a.first", 1, "unresolved_type"),
            ("z.last", 1, "input"),
        ]
    );

    let rejecting = RejectResolvedTypes;
    let candidates =
        catalog.create_from_wire_candidates(&source, NodeGraphConnectionMode::Strict, &rejecting);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].port(), &PortKey::new("unresolved_type"));
}

#[test]
fn loose_create_from_wire_query_preserves_final_planner_type_direction() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("math.source")
                .port(
                    NodePortSpec::new(
                        "same_direction",
                        PortDirection::In,
                        PortKind::Data,
                        PortCapacity::Multi,
                    )
                    .with_type(TypeDesc::Int),
                )
                .build(),
        )
        .unwrap();
    let source = source_port(PortDirection::In, PortKind::Data, Some(TypeDesc::Float));

    let strict_compatibility = DefaultTypeCompatibility;
    assert!(
        catalog
            .create_from_wire_candidates(
                &source,
                NodeGraphConnectionMode::Strict,
                &strict_compatibility,
            )
            .is_empty()
    );

    let loose_compatibility = DefaultTypeCompatibility;
    let candidates = catalog.create_from_wire_candidates(
        &source,
        NodeGraphConnectionMode::Loose,
        &loose_compatibility,
    );
    assert_eq!(
        candidates.len(),
        1,
        "candidate Int is evaluated as Int -> source Float"
    );
}

#[test]
fn runtime_create_from_wire_query_is_revisioned_and_builds_one_atomic_command() {
    let source_id = PortId::from_u128(91);
    let source = source_port(PortDirection::Out, PortKind::Data, Some(TypeDesc::Int));
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("math.target")
                .port(
                    NodePortSpec::new(
                        "input",
                        PortDirection::In,
                        PortKind::Data,
                        PortCapacity::Single,
                    )
                    .with_type(TypeDesc::Float),
                )
                .build(),
        )
        .unwrap();
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(graph_with_source_port(source_id, source, true))
        .unwrap();
    let source_instance = runtime
        .port_instance_key(source_id)
        .unwrap()
        .expect("source port has an exact lifetime");

    let query = runtime
        .create_from_wire_candidates(source_instance, NodeGraphConnectionMode::Strict)
        .unwrap();
    assert_eq!(query.revision(), 0);
    assert_eq!(query.source_instance().port(), source_id);
    assert_eq!(query.candidates().len(), 1);
    assert_eq!(
        query.candidates()[0].node_kind(),
        &NodeKindKey::new("math.target")
    );
    assert_eq!(query.candidates()[0].port(), &PortKey::new("input"));

    let source_node = runtime
        .snapshot()
        .unwrap()
        .graph()
        .port(&source_id)
        .unwrap()
        .node;
    let source_version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_data(
            source_version,
            source_node,
            json!({ "changed": true }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .unwrap();
    let created_node = NodeId::from_u128(92);
    let created_port = PortId::from_u128(93);
    let edge = EdgeId::from_u128(94);
    let creation = catalog
        .instantiate_node_with_ids(
            query.candidates()[0].node_kind(),
            created_node,
            test_bounds(CanvasPoint { x: 200.0, y: 100.0 }),
            [created_port],
        )
        .unwrap();
    assert_eq!(
        creation.port_id(query.candidates()[0].port()),
        Some(created_port)
    );
    let command = GraphCommand::topology(
        query.version(),
        creation
            .into_topology_edit()
            .connection(ConnectionEdit::connect(
                edge,
                source_id,
                created_port,
                NodeGraphConnectionMode::Strict,
            )),
    );
    let history_before = runtime.history_state().unwrap();
    let stale = runtime.execute(command).into_result().unwrap();
    assert!(stale.conflict().is_some());
    assert_eq!(runtime.history_state().unwrap(), history_before);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().node(&created_node).is_none());
    assert!(snapshot.graph().edge(&edge).is_none());

    let refreshed = runtime
        .create_from_wire_candidates(source_instance, NodeGraphConnectionMode::Strict)
        .unwrap();
    let creation = catalog
        .instantiate_node_with_ids(
            refreshed.candidates()[0].node_kind(),
            created_node,
            test_bounds(CanvasPoint { x: 200.0, y: 100.0 }),
            [created_port],
        )
        .unwrap();
    runtime
        .execute(GraphCommand::topology(
            refreshed.version(),
            creation
                .into_topology_edit()
                .connection(ConnectionEdit::connect(
                    edge,
                    source_id,
                    created_port,
                    NodeGraphConnectionMode::Strict,
                )),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("node and edge commit atomically");
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().node(&created_node).is_some());
    assert!(snapshot.graph().port(&created_port).is_some());
    assert!(snapshot.graph().edge(&edge).is_some());
    assert_eq!(
        runtime.history_state().unwrap().undo_len(),
        history_before.undo_len() + 1
    );
}

#[test]
fn runtime_create_from_wire_query_uses_current_profile_and_source_policy() {
    let source_id = PortId::from_u128(101);
    let source = source_port(PortDirection::Out, PortKind::Data, Some(TypeDesc::Int));
    let graph = graph_with_source_port(source_id, source.clone(), true);
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.target", 1)).unwrap();
    let runtime = GraphRuntime::builder(catalog.clone())
        .with_profile(Box::new(RejectingStoredTypesProfile {
            compatibility: RejectResolvedTypes,
        }))
        .build(graph)
        .unwrap();
    let source_instance = runtime
        .port_instance_key(source_id)
        .unwrap()
        .expect("source port has an exact lifetime");

    let query = runtime
        .create_from_wire_candidates(source_instance, NodeGraphConnectionMode::Strict)
        .unwrap();
    assert!(query.candidates().is_empty());

    let snapshot = runtime.snapshot().unwrap();
    let source_node = snapshot.graph().port(&source_id).unwrap().node;
    let removed = runtime
        .execute(GraphCommand::topology(
            snapshot.version(),
            TopologyEdit::new().remove_node(source_node, IncidentEdgePolicy::Cascade),
        ))
        .into_result()
        .unwrap();
    let removed_version = removed.applied().unwrap().version();
    runtime
        .execute(GraphCommand::undo(removed_version))
        .into_result()
        .unwrap()
        .applied()
        .unwrap();
    assert!(matches!(
        runtime.create_from_wire_candidates(source_instance, NodeGraphConnectionMode::Strict),
        Err(CreateFromWireQueryError::SourcePortRetired { port }) if port == source_id
    ));

    let disabled = GraphRuntime::builder(catalog)
        .build(graph_with_source_port(source_id, source, false))
        .unwrap();
    let disabled_source = disabled
        .port_instance_key(source_id)
        .unwrap()
        .expect("disabled source port still has an exact lifetime");
    assert!(matches!(
        disabled.create_from_wire_candidates(
            disabled_source,
            NodeGraphConnectionMode::Strict,
        ),
        Err(CreateFromWireQueryError::SourceNotConnectable { port }) if port == source_id
    ));
}

#[test]
fn canonicalization_budget_wins_before_allocating_invalid_source_diagnostics() {
    let mut first = demo_add_node("invalid.first", 1, serde_json::Value::Null);
    first.ports = vec![PortId::from_u128(10)];
    let mut second = demo_add_node("invalid.second", 1, serde_json::Value::Null);
    second.ports = vec![PortId::from_u128(20)];
    let graph = GraphBuilder::new(GraphId::from_u128(1))
        .with_node(NodeId::from_u128(1), first)
        .with_node(NodeId::from_u128(2), second)
        .build_unchecked();

    let error = NodeCatalog::new()
        .canonicalize_graph_with_limits(&graph, PersistenceLimits::default().with_max_entities(1))
        .unwrap_err();
    let NodeCatalogMigrationError::ResourceLimit(error) = error else {
        panic!("expected the entity limit before source diagnostics, got {error}");
    };
    assert_eq!(error.resource(), crate::io::PersistenceResource::Entities);
    assert_eq!(error.limit(), 1);
    assert_eq!(error.actual(), 4);
}

#[test]
fn missing_migrator_does_not_consume_the_invocation_budget() {
    let kind = "math.missing-migrator";
    let node = NodeId::from_u128(1);
    let graph = GraphBuilder::new(GraphId::from_u128(2))
        .with_node(node, demo_add_node(kind, 1, serde_json::Value::Null))
        .build()
        .unwrap();
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec(kind, 2)).unwrap();

    let error = catalog
        .canonicalize_graph_with_limits(
            &graph,
            PersistenceLimits::default().with_max_migration_operations(0),
        )
        .unwrap_err();

    let NodeCatalogMigrationError::IncompatibleKinds { issues, .. } = error else {
        panic!("expected the missing-migrator diagnostic, got {error}");
    };
    assert!(matches!(
        issues.as_slice(),
        [NodeKindMigrationIssue::MissingMigrator {
            node: missing,
            kind: missing_kind,
            from: 1,
            to: 2,
        }] if *missing == node && missing_kind == &NodeKindKey::new(kind)
    ));
}

#[test]
fn canonicalization_is_atomic_idempotent_and_preserves_unknown_kinds_opaquely() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(add_spec("math.add", 2).with_alias("legacy.add"))
        .unwrap();
    catalog
        .register_migrator(NodeKindKey::new("math.add"), Arc::new(DummyMigrator))
        .unwrap();

    let known = NodeId::from_u128(1);
    let unknown = NodeId::from_u128(2);
    let mut builder = GraphBuilder::new(GraphId::from_u128(7));
    builder.insert_node(known, demo_add_node("legacy.add", 1, json!({ "lhs": 1.0 })));
    builder.insert_node(
        unknown,
        demo_add_node("plugin.future", 99, json!({ "sentinel": [1, 2, 3] })),
    );
    let graph = builder.build().unwrap();

    let canonical = catalog.canonicalize_graph(&graph).unwrap();

    assert_eq!(
        graph.node(&known).unwrap().kind,
        NodeKindKey::new("legacy.add"),
        "source graph must remain unchanged"
    );
    let migrated = canonical.graph().node(&known).unwrap();
    assert_eq!(migrated.kind, NodeKindKey::new("math.add"));
    assert_eq!(migrated.kind_version, 2);
    assert_eq!(migrated.data["migrated"], json!(true));
    assert_eq!(
        canonical.graph().node(&unknown).unwrap().data,
        json!({ "sentinel": [1, 2, 3] })
    );
    assert_eq!(canonical.report().upgraded().len(), 1);
    assert_eq!(canonical.report().canonicalized().len(), 1);
    assert_eq!(canonical.report().opaque().len(), 1);
    assert!(canonical.report().has_changes());

    let second = catalog.canonicalize_graph(canonical.graph()).unwrap();
    assert!(!second.report().has_changes());
    assert_eq!(second.report().opaque().len(), 1);
    assert_eq!(second.graph().node(&known).unwrap().data, migrated.data);
}

#[test]
fn runtime_canonicalizes_once_before_publishing_the_same_graph_to_every_view() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("math.add")
                .latest_kind_version(2)
                .alias("legacy.add")
                .build(),
        )
        .unwrap();
    catalog
        .register_migrator(
            NodeKindKey::new("math.add"),
            Arc::new(CountingMigrator {
                calls: Arc::clone(&calls),
            }),
        )
        .unwrap();

    let node = NodeId::from_u128(0xCA11);
    let graph = GraphBuilder::new(GraphId::from_u128(0xCA12))
        .with_node(
            node,
            demo_add_node("legacy.add", 1, json!({ "source": true })),
        )
        .build()
        .unwrap();
    let runtime = GraphRuntime::builder(catalog).build(graph).unwrap();
    let first = runtime.attach_view(Default::default()).unwrap();
    let second = runtime.attach_view(Default::default()).unwrap();
    let weak = first.downgrade();

    assert!(Arc::ptr_eq(
        &runtime.catalog_owned(),
        &first.catalog_owned()
    ));
    assert!(Arc::ptr_eq(&first.catalog_owned(), &second.catalog_owned()));
    assert!(Arc::ptr_eq(&second.catalog_owned(), &weak.catalog_owned()));

    let first_snapshot = first.graph_snapshot().unwrap();
    let second_snapshot = second.graph_snapshot().unwrap();
    assert!(first_snapshot.ptr_eq(&second_snapshot));
    for snapshot in [&first_snapshot, &second_snapshot] {
        let published = snapshot.graph().node(&node).unwrap();
        assert_eq!(published.kind, NodeKindKey::new("math.add"));
        assert_eq!(published.kind_version, 2);
        assert_eq!(published.data["migrated"], json!(true));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn runtime_rejects_a_registered_legacy_kind_before_any_view_can_attach() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("math.add")
                .latest_kind_version(2)
                .alias("legacy.add")
                .build(),
        )
        .unwrap();
    let node = NodeId::from_u128(0xCA21);
    let graph = GraphBuilder::new(GraphId::from_u128(0xCA22))
        .with_node(node, demo_add_node("legacy.add", 1, json!({})))
        .build()
        .unwrap();

    let error = GraphRuntime::builder(catalog).build(graph).unwrap_err();

    assert!(matches!(
        error,
        GraphRuntimeError::Canonicalization(NodeCatalogMigrationError::IncompatibleKinds {
            issues,
            omitted_count: 0,
        }) if matches!(
            issues.as_slice(),
            [NodeKindMigrationIssue::MissingMigrator {
                node: blocked,
                kind,
                from: 1,
                to: 2,
            }] if *blocked == node && kind == &NodeKindKey::new("math.add")
        )
    ));
}

#[test]
fn canonicalization_structurally_validates_the_migrated_result() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(SYMBOL_REF_NODE_KIND)
                .latest_kind_version(1)
                .alias("legacy.symbol-ref")
                .build(),
        )
        .unwrap();
    let node = NodeId::from_u128(3);
    let graph = GraphBuilder::new(GraphId::from_u128(14))
        .with_node(
            node,
            demo_add_node("legacy.symbol-ref", 1, serde_json::Value::Null),
        )
        .build()
        .expect("the opaque legacy source is structurally valid before canonicalization");

    let error = catalog.canonicalize_graph(&graph).unwrap_err();

    assert!(matches!(
        error,
        NodeCatalogMigrationError::InvalidResult { errors }
            if errors.iter().any(|error| matches!(
                error,
                GraphValidationError::SymbolRefNodeMissingSymbolId { node: invalid }
                    if *invalid == node
            ))
    ));
    assert_eq!(
        graph.node(&node).unwrap().kind,
        NodeKindKey::new("legacy.symbol-ref"),
        "failed scratch canonicalization must leave the source unchanged"
    );
}

#[test]
fn one_known_kind_failure_rejects_the_entire_scratch_migration() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 2)).unwrap();
    catalog
        .register(
            NodeKindSpec::builder("math.multiply")
                .latest_kind_version(2)
                .build(),
        )
        .unwrap();
    catalog
        .register_migrator(NodeKindKey::new("math.add"), Arc::new(DummyMigrator))
        .unwrap();

    let migratable = NodeId::from_u128(1);
    let blocked = NodeId::from_u128(2);
    let mut builder = GraphBuilder::new(GraphId::from_u128(8));
    builder.insert_node(
        migratable,
        demo_add_node("math.add", 1, json!({ "lhs": 1.0 })),
    );
    builder.insert_node(
        blocked,
        demo_add_node("math.multiply", 1, json!({ "lhs": 2.0 })),
    );
    let graph = builder.build().unwrap();

    let error = catalog.canonicalize_graph(&graph).unwrap_err();
    let NodeCatalogMigrationError::IncompatibleKinds { issues, .. } = error else {
        panic!("unexpected migration error: {error}");
    };
    assert!(matches!(
        issues.as_slice(),
        [NodeKindMigrationIssue::MissingMigrator { node, kind, from: 1, to: 2 }]
            if *node == blocked && kind == &NodeKindKey::new("math.multiply")
    ));
    assert_eq!(graph.node(&migratable).unwrap().kind_version, 1);
    assert_eq!(graph.node(&migratable).unwrap().data, json!({ "lhs": 1.0 }));
}

#[test]
fn future_registered_kind_is_rejected_while_future_unknown_kind_remains_opaque() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 2)).unwrap();

    let registered = NodeId::from_u128(1);
    let opaque = NodeId::from_u128(2);
    let mut builder = GraphBuilder::new(GraphId::from_u128(9));
    builder.insert_node(
        registered,
        demo_add_node("math.add", 3, json!({ "known_sentinel": true })),
    );
    builder.insert_node(
        opaque,
        demo_add_node("plugin.future", 500, json!({ "opaque_sentinel": true })),
    );
    let graph = builder.build().unwrap();

    let error = catalog.canonicalize_graph(&graph).unwrap_err();
    let NodeCatalogMigrationError::IncompatibleKinds { issues, .. } = error else {
        panic!("unexpected migration error: {error}");
    };
    assert!(matches!(
        issues.as_slice(),
        [NodeKindMigrationIssue::FutureVersion {
            node,
            kind,
            found: 3,
            supported: 2,
        }] if *node == registered && kind == &NodeKindKey::new("math.add")
    ));
    assert_eq!(
        graph.node(&opaque).unwrap().data,
        json!({ "opaque_sentinel": true })
    );
}

#[test]
fn migrator_errors_are_structured_and_publish_no_canonical_graph() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 2)).unwrap();
    catalog
        .register_migrator(NodeKindKey::new("math.add"), Arc::new(FailingMigrator))
        .unwrap();
    let node = NodeId::from_u128(1);
    let graph = GraphBuilder::new(GraphId::from_u128(10))
        .with_node(node, demo_add_node("math.add", 1, json!({ "lhs": 1.0 })))
        .build()
        .unwrap();

    let error = catalog.canonicalize_graph(&graph).unwrap_err();

    assert!(matches!(
        error,
        NodeCatalogMigrationError::IncompatibleKinds { issues, .. }
            if matches!(
                issues.as_slice(),
                [NodeKindMigrationIssue::MigrationFailed {
                    node: failed,
                    kind,
                    from: 1,
                    to: 2,
                    message,
                }] if *failed == node
                    && kind == &NodeKindKey::new("math.add")
                    && message == "fixture migration failed"
            )
    ));
    assert_eq!(graph.node(&node).unwrap().kind_version, 1);
}

#[test]
fn migration_issue_diagnostics_bound_entries_and_message_bytes() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 2)).unwrap();
    catalog
        .register_migrator(NodeKindKey::new("math.add"), Arc::new(FailingMigrator))
        .unwrap();
    let first = NodeId::from_u128(1);
    let second = NodeId::from_u128(2);
    let third = NodeId::from_u128(3);
    let future = NodeId::from_u128(4);
    let graph = GraphBuilder::new(GraphId::from_u128(16))
        .with_node(first, demo_add_node("math.add", 1, serde_json::Value::Null))
        .with_node(
            second,
            demo_add_node("math.add", 1, serde_json::Value::Null),
        )
        .with_node(third, demo_add_node("math.add", 1, serde_json::Value::Null))
        .with_node(
            future,
            demo_add_node("math.add", 3, serde_json::Value::Null),
        )
        .build()
        .unwrap();

    let error = catalog
        .canonicalize_graph_with_limits(
            &graph,
            PersistenceLimits::default()
                .with_max_migration_issues(2)
                .with_max_migration_issue_message_bytes("fixture migration failed".len()),
        )
        .unwrap_err();

    let NodeCatalogMigrationError::IncompatibleKinds {
        issues,
        omitted_count,
    } = error
    else {
        panic!("unexpected migration error: {error}");
    };
    assert_eq!(omitted_count, 2);
    assert!(matches!(
        issues.as_slice(),
        [
            NodeKindMigrationIssue::MigrationFailed { node, message, .. },
            NodeKindMigrationIssue::FutureVersion { node: future_node, .. },
        ] if *node == first
            && message == "fixture migration failed"
            && *future_node == future
    ));
}

#[test]
fn profile_validation_runs_after_canonicalization_and_before_runtime_publication() {
    let catalog = NodeCatalog::new();
    let graph = Graph::new(GraphId::from_u128(11));
    let canonical = catalog.canonicalize_graph(&graph).unwrap();

    assert!(matches!(
        GraphRuntime::from_canonical_with_profile(canonical, Box::new(RejectCanonicalGraphProfile),),
        Err(GraphRuntimeError::Apply(
            ApplyPipelineError::Rejected { .. }
        ))
    ));
}

#[test]
fn document_replacement_accepts_an_unmodified_catalog_clone_and_rejects_a_semantic_fork() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 1)).unwrap();
    let source = Graph::new(GraphId::from_u128(0xc001));
    let runtime =
        GraphRuntime::from_canonical(catalog.canonicalize_graph(&source).unwrap()).unwrap();

    let unchanged_clone = catalog.clone();
    let accepted = unchanged_clone
        .canonicalize_graph(&Graph::new(GraphId::from_u128(0xc002)))
        .unwrap();
    let patch = runtime.replace_document(accepted).unwrap();
    assert_eq!(patch.to_revision(), 1);

    let mut semantic_fork = catalog.clone();
    semantic_fork
        .register(add_spec("math.multiply", 1))
        .unwrap();
    let rejected = semantic_fork
        .canonicalize_graph(&Graph::new(GraphId::from_u128(0xc003)))
        .unwrap();
    let before = runtime.snapshot().unwrap();

    assert!(matches!(
        runtime.replace_document(rejected),
        Err(GraphRuntimeError::CatalogProvenanceMismatch)
    ));
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
}

#[test]
fn failed_catalog_registration_does_not_fork_document_provenance() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 1)).unwrap();
    let runtime = GraphRuntime::from_canonical(
        catalog
            .canonicalize_graph(&Graph::new(GraphId::from_u128(0xc011)))
            .unwrap(),
    )
    .unwrap();
    let mut unchanged_clone = catalog.clone();

    assert!(matches!(
        unchanged_clone.register(add_spec("math.add", 1)),
        Err(NodeCatalogRegistrationError::DuplicateKind { .. })
    ));
    let replacement = unchanged_clone
        .canonicalize_graph(&Graph::new(GraphId::from_u128(0xc012)))
        .unwrap();

    runtime.replace_document(replacement).unwrap();
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
}

#[test]
fn registering_a_migrator_forks_document_provenance() {
    let mut catalog = NodeCatalog::new();
    catalog.register(add_spec("math.add", 2)).unwrap();
    let runtime = GraphRuntime::from_canonical(
        catalog
            .canonicalize_graph(&Graph::new(GraphId::from_u128(0xc021)))
            .unwrap(),
    )
    .unwrap();
    let mut semantic_fork = catalog.clone();
    semantic_fork
        .register_migrator(NodeKindKey::new("math.add"), Arc::new(FailingMigrator))
        .unwrap();
    let replacement = semantic_fork
        .canonicalize_graph(&Graph::new(GraphId::from_u128(0xc022)))
        .unwrap();

    assert!(matches!(
        runtime.replace_document(replacement),
        Err(GraphRuntimeError::CatalogProvenanceMismatch)
    ));
}
