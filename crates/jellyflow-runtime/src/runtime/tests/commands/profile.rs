use super::*;

struct AddsLegacyKindProfile {
    node: NodeId,
    template: Node,
}

impl GraphProfile for AddsLegacyKindProfile {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn concretize(&self, graph: &Graph) -> TopologyEdit {
        if graph.node(&self.node).is_some() {
            TopologyEdit::new()
        } else {
            TopologyEdit::new().add_node(
                self.node,
                node_with_kind(self.template.clone(), "legacy.add", 2),
                [],
            )
        }
    }
}

#[test]
fn profile_concretization_cannot_reintroduce_a_registered_alias() {
    let (graph, nodes) = graph_with_nodes(1);
    let added = NodeId::from_u128(0xCA30);
    let template = graph.node(&nodes[0]).unwrap().clone();
    let runtime = GraphRuntime::builder(canonical_add_catalog())
        .with_profile(Box::new(AddsLegacyKindProfile {
            node: added,
            template,
        }))
        .build(graph)
        .unwrap();

    let rejected = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            nodes[0],
            serde_json::json!({ "changed": true }),
        ),
        "topology.node_kind_not_canonical",
    );

    assert!(matches!(
        rejected.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Node { id } if id == added
    ));
    assert!(runtime.node_lifetime_key(added).unwrap().is_none());
}

#[test]
fn profile_type_compatibility_is_shared_by_document_evaluation_and_execution() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.source_port)
                .map(|port| port.ty = Some(TypeDesc::String))
        })
        .unwrap();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.alternate_source_port)
                .map(|port| port.ty = Some(TypeDesc::String))
        })
        .unwrap();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.ty = Some(TypeDesc::Int))
        })
        .unwrap();
    let runtime = profiled_runtime(
        graph,
        Box::new(AcceptAllStoredTypesProfile {
            compatibility: AcceptAllTypeCompatibility,
        }),
    )
    .expect("custom compatibility validates the existing mismatched edge");
    let edge = EdgeId::from_u128(91_000);
    let edit = ConnectionEdit::connect(
        edge,
        ids.alternate_source_port,
        ids.target_port,
        NodeGraphConnectionMode::Strict,
    );

    let evaluation = runtime.evaluate_connection(edit).unwrap();
    assert!(evaluation.is_accept());
    assert_eq!(evaluation.revision(), 0);

    runtime
        .execute(GraphCommand::connection(evaluation.version(), edit))
        .into_result()
        .unwrap()
        .applied()
        .expect("execution uses the same custom compatibility");
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_some());
}

#[test]
fn symbol_type_changes_revalidate_profile_types_atomically() {
    struct SymbolBackedPortTypes {
        symbol: SymbolId,
        port: PortId,
    }

    impl GraphProfile for SymbolBackedPortTypes {
        fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            if port == self.port {
                return graph
                    .symbol(&self.symbol)
                    .and_then(|symbol| symbol.ty.clone());
            }
            graph.port(&port).and_then(|port| port.ty.clone())
        }

        fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
            Vec::new()
        }
    }

    let (graph, ids) = string_typed_routed_graph();
    let symbol_id = SymbolId::from_u128(91_100);
    let mut builder = GraphBuilder::new(graph.graph_id()).with_symbol(
        symbol_id,
        Symbol {
            name: "source value".to_owned(),
            ty: Some(TypeDesc::String),
            default_value: None,
            meta: serde_json::Value::Null,
        },
    );
    for (id, node) in graph.nodes() {
        builder.insert_node(*id, node.clone());
    }
    for (id, port) in graph.ports() {
        builder.insert_port(*id, port.clone());
    }
    for (id, edge) in graph.edges() {
        builder.insert_edge(*id, edge.clone());
    }
    let runtime = profiled_runtime(
        builder.build().expect("valid symbol-backed graph"),
        Box::new(SymbolBackedPortTypes {
            symbol: symbol_id,
            port: ids.source_port,
        }),
    )
    .expect("initial symbol type is compatible");
    let mut incompatible = runtime
        .snapshot()
        .unwrap()
        .graph()
        .symbol(&symbol_id)
        .expect("symbol")
        .clone();
    incompatible.ty = Some(TypeDesc::Int);

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().update(symbol_id, incompatible),
        ),
        "connect.type_mismatch",
    );
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .symbol(&symbol_id)
            .expect("committed symbol")
            .ty,
        Some(TypeDesc::String),
    );

    let mut compatible = runtime
        .snapshot()
        .unwrap()
        .graph()
        .symbol(&symbol_id)
        .expect("symbol")
        .clone();
    compatible.name = "renamed source value".to_owned();
    let outcome = runtime
        .execute(GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().update(symbol_id, compatible),
        ))
        .into_result()
        .unwrap();
    let planning_work = outcome
        .applied()
        .expect("compatible symbol update applies")
        .planning_work();
    assert_eq!(
        planning_work.full_type_revalidations(),
        1,
        "the conservative non-port fallback is observable",
    );
}

#[test]
fn profile_can_narrow_symbol_dependencies_through_indexed_referrers() {
    struct IndexedSymbolBackedTypes {
        symbol: SymbolId,
        port: PortId,
    }

    impl GraphProfile for IndexedSymbolBackedTypes {
        fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            if port == self.port {
                return graph
                    .symbol(&self.symbol)
                    .and_then(|symbol| symbol.ty.clone());
            }
            graph.port(&port).and_then(|port| port.ty.clone())
        }

        fn port_types_affected_by_mutation(
            &self,
            context: &mut PortTypeImpactContext<'_>,
        ) -> PortTypeRevalidation {
            let symbols = context
                .footprint()
                .symbols
                .iter()
                .copied()
                .collect::<Vec<_>>();
            let mut nodes = std::collections::BTreeSet::new();
            for symbol in symbols {
                nodes.extend(context.symbol_referrer_ids_before(symbol));
                nodes.extend(context.symbol_referrer_ids_after(symbol));
            }
            let mut ports = std::collections::BTreeSet::new();
            for node in nodes {
                ports.extend(context.node_port_ids_before(node));
                ports.extend(context.node_port_ids_after(node));
            }
            PortTypeRevalidation::Exact(ports.into_iter().collect())
        }

        fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
            Vec::new()
        }
    }

    let (graph, ids) = string_typed_routed_graph();
    let symbol_id = SymbolId::from_u128(91_101);
    let mut builder = GraphBuilder::new(graph.graph_id()).with_symbol(
        symbol_id,
        Symbol {
            name: "source value".to_owned(),
            ty: Some(TypeDesc::String),
            default_value: None,
            meta: serde_json::Value::Null,
        },
    );
    for (id, node) in graph.nodes() {
        let mut node = node.clone();
        if *id == ids.source {
            node.kind = NodeKindKey::new(SYMBOL_REF_NODE_KIND);
            node.data = symbol_ref_node_data(symbol_id);
        }
        builder.insert_node(*id, node);
    }
    for (id, port) in graph.ports() {
        builder.insert_port(*id, port.clone());
    }
    for (id, edge) in graph.edges() {
        builder.insert_edge(*id, edge.clone());
    }
    let graph = builder.build().expect("valid indexed symbol-ref graph");
    let mut renamed = graph.symbol(&symbol_id).expect("symbol").clone();
    renamed.name = "renamed source value".to_owned();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let command = GraphCommand::symbols(
        runtime.snapshot().unwrap().version(),
        SymbolEdit::new().update(symbol_id, renamed),
    );
    let planned = plan_graph_command(
        &graph,
        &GraphIndex::from_graph(&graph),
        &NodeCatalog::new(),
        &command,
        Some(&IndexedSymbolBackedTypes {
            symbol: symbol_id,
            port: ids.source_port,
        }),
        &crate::runtime::policy::GraphRuntimePolicy::default(),
    )
    .expect("indexed exact dependency plans");

    assert_eq!(planned.planning_work.full_type_revalidations(), 0);
    assert!(planned.planning_work.index_bucket_reads() >= 3);
    assert!(matches!(
        planned.transaction.ops(),
        [GraphOp::SetSymbolName { id, .. }] if *id == symbol_id
    ));
}

#[test]
fn stable_port_type_changes_revalidate_existing_edges_against_final_topology() {
    let (graph, ids) = string_typed_routed_graph();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let target_ports = topology_ports_with_type(
        runtime.snapshot().unwrap().graph(),
        ids.target,
        ids.target_port,
        TypeDesc::Int,
    );

    let rejected = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                ids.target,
                target_ports,
                IncidentEdgePolicy::Reject,
            ),
        ),
        "connect.type_mismatch",
    );
    assert!(matches!(
        rejected.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == ids.target_port
    ));
    assert_eq!(
        rejected
            .rejection()
            .unwrap()
            .planning_work()
            .operations_emitted(),
        1,
        "profile convergence rejection preserves work emitted before validation",
    );

    let (graph, ids) = string_typed_routed_graph();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let target_ports = topology_ports_with_type(
        runtime.snapshot().unwrap().graph(),
        ids.target,
        ids.target_port,
        TypeDesc::Int,
    );
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                ids.target,
                target_ports,
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("cascade removes the now-incompatible edge");
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().port(&ids.target_port).unwrap().ty,
        Some(TypeDesc::Int)
    );
    assert!(snapshot.graph().edge(&ids.edge).is_none());
    undo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().port(&ids.target_port).unwrap().ty,
        Some(TypeDesc::String)
    );
    assert!(snapshot.graph().edge(&ids.edge).is_some());

    let (graph, ids) = string_typed_routed_graph();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let snapshot = runtime.snapshot().unwrap();
    let source_ports =
        topology_ports_with_type(snapshot.graph(), ids.source, ids.source_port, TypeDesc::Int);
    let target_ports =
        topology_ports_with_type(snapshot.graph(), ids.target, ids.target_port, TypeDesc::Int);
    runtime
        .execute(GraphCommand::topology(
            snapshot.version(),
            TopologyEdit::new()
                .replace_node_ports(ids.source, source_ports, IncidentEdgePolicy::Reject)
                .replace_node_ports(ids.target, target_ports, IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("coordinated endpoint type update validates the final topology");
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().edge(&ids.edge).is_some());
    assert_eq!(
        snapshot.graph().port(&ids.source_port).unwrap().ty,
        Some(TypeDesc::Int)
    );
    assert_eq!(
        snapshot.graph().port(&ids.target_port).unwrap().ty,
        Some(TypeDesc::Int)
    );
}

#[test]
fn profile_declares_sibling_port_type_impact_from_port_data() {
    let (graph, ids) = graph_with_routed_edge();
    let trigger = PortId::from_u128(91_005);
    let mut builder = GraphBuilder::new(graph.graph_id());
    for (id, node) in graph.nodes() {
        let mut node = node.clone();
        if *id == ids.source {
            node.ports.push(trigger);
        }
        builder.insert_node(*id, node);
    }
    for (id, port) in graph.ports() {
        builder.insert_port(*id, port.clone());
    }
    for (id, edge) in graph.edges() {
        builder.insert_edge(*id, edge.clone());
    }
    builder.insert_port(trigger, port(ids.source, "type_control", PortDirection::In));
    let graph = builder.build().expect("graph with sibling control port");
    let runtime = profiled_runtime(
        graph,
        Box::new(PortDataDependentTypesProfile {
            trigger,
            dependent: ids.source_port,
            string_target: ids.target_port,
        }),
    )
    .unwrap();

    let rejected = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_port_data(
            runtime.snapshot().unwrap().version(),
            trigger,
            serde_json::Value::Bool(true),
        ),
        "connect.type_mismatch",
    );

    assert!(matches!(
        rejected.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == ids.target_port
    ));
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .port(&trigger)
            .unwrap()
            .data,
        serde_json::Value::Null
    );
}

#[test]
fn profile_declares_transitive_type_impact_from_connection_topology() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = profiled_runtime(
        graph,
        Box::new(ConnectionDependentTypesProfile {
            dependent: ids.source_port,
        }),
    )
    .unwrap();
    let new_edge = EdgeId::from_u128(91_003);

    let rejected = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            new_edge,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ),
        "connect.type_mismatch",
    );

    assert!(matches!(
        rejected.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == ids.target_port
    ));
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&new_edge)
            .is_none()
    );
}

#[test]
fn runtime_computes_transitive_profile_type_dependency_closure() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = profiled_runtime(
        graph,
        Box::new(ChainedPortTypesProfile {
            trigger: ids.alternate_source_port,
            intermediate: ids.target_port,
            dependent: ids.source_port,
            string_target: ids.target_port,
        }),
    )
    .unwrap();

    let rejected = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_port_data(
            runtime.snapshot().unwrap().version(),
            ids.alternate_source_port,
            serde_json::Value::Bool(true),
        ),
        "connect.type_mismatch",
    );

    assert!(matches!(
        rejected.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == ids.target_port
    ));
    assert_eq!(
        rejected
            .rejection()
            .unwrap()
            .planning_work()
            .operations_emitted(),
        1,
        "transitive profile rejection preserves the attempted connection operation",
    );
}

#[test]
fn profile_cannot_expand_type_dependencies_through_missing_ports() {
    let (graph, ids) = graph_with_routed_edge();
    let missing = PortId::from_u128(91_006);
    let runtime =
        profiled_runtime(graph, Box::new(MissingPortDependencyProfile { missing })).unwrap();

    let rejected = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_port_data(
            runtime.snapshot().unwrap().version(),
            ids.alternate_source_port,
            serde_json::Value::Bool(true),
        ),
        "profile.port_type_dependency_missing",
    );

    assert!(matches!(
        rejected.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == missing
    ));
}

#[test]
fn cascade_type_revalidation_keeps_policy_after_an_empty_first_drain() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = profiled_runtime(
        graph,
        Box::new(RemovedPortDependentTypesProfile {
            trigger: ids.alternate_source_port,
            dependent: ids.source_port,
            string_target: ids.target_port,
        }),
    )
    .unwrap();

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                ids.alternate_source,
                [],
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("cascade policy removes the indirectly incompatible edge");

    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().port(&ids.alternate_source_port).is_none());
    assert!(snapshot.graph().edge(&ids.edge).is_none());

    undo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().port(&ids.alternate_source_port).is_some());
    assert!(snapshot.graph().edge(&ids.edge).is_some());
}

#[test]
fn runtime_profile_rejects_an_initially_type_invalid_document() {
    let (mut graph, ids) = string_typed_routed_graph();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.ty = Some(TypeDesc::Int))
        })
        .expect("target port");

    let error = profiled_runtime(graph, Box::new(StoredPortTypesProfile))
        .expect_err("initial type mismatch must reject the document");

    assert_profile_error_key(error, "connect.type_mismatch");
}

#[test]
fn type_invalid_document_replacement_is_atomic() {
    let (graph, ids) = string_typed_routed_graph();
    let runtime = profiled_runtime(graph, Box::new(StoredPortTypesProfile)).unwrap();
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();
    let mut replacement = before.graph().clone();
    replacement
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.ty = Some(TypeDesc::Int))
        })
        .expect("target port");
    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();

    let error = runtime
        .replace_document(replacement)
        .expect_err("type-invalid replacement must reject atomically");

    assert_profile_error_key(error, "connect.type_mismatch");
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);
}

#[test]
fn runtime_profile_rejects_an_initially_disallowed_cycle() {
    let (graph, ids) = graph_with_routed_edge();
    let reverse_edge = EdgeId::from_u128(91_004);
    let mut builder = GraphBuilder::new(graph.graph_id());
    for (id, node) in graph.nodes() {
        builder.insert_node(*id, node.clone());
    }
    for (id, port) in graph.ports() {
        builder.insert_port(*id, port.clone());
    }
    for (id, edge) in graph.edges() {
        builder.insert_edge(*id, edge.clone());
    }
    builder.insert_edge(
        reverse_edge,
        Edge::new(EdgeKind::Data, ids.target_port, ids.source_port),
    );
    let graph = builder.build().expect("structurally valid cyclic graph");

    let error = profiled_runtime(graph, Box::new(NoDataCyclesProfile))
        .expect_err("profile-disallowed cycle must reject the document");

    assert_profile_error_key(error, "connect.cycle");
}

#[test]
fn connection_commands_use_profile_type_resolution() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = profiled_runtime(
        graph,
        Box::new(FixedPortTypesProfile {
            string_port: ids.alternate_source_port,
            int_port: ids.target_port,
        }),
    )
    .unwrap();
    let edge = EdgeId::from_u128(224);

    let outcome = runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            edge,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap();

    let rejection = outcome.rejection().expect("profile types reject");
    assert_eq!(rejection.diagnostics()[0].key, "connect.type_mismatch");
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[test]
fn connection_evaluation_and_execution_share_revisioned_semantics() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let edge = EdgeId::from_u128(223);
    let edit = ConnectionEdit::connect(
        edge,
        ids.alternate_source_port,
        ids.target_port,
        NodeGraphConnectionMode::Strict,
    );

    let evaluation = runtime.evaluate_connection(edit).unwrap();
    assert!(evaluation.is_accept());
    assert_eq!(evaluation.revision(), 0);
    assert!(evaluation.diagnostics().is_empty());
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_none());

    runtime
        .execute(GraphCommand::set_node_data(
            evaluation.version(),
            ids.source,
            serde_json::json!({ "changed": true }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("intervening command applies");
    let stale = runtime
        .execute(GraphCommand::connection(evaluation.version(), edit))
        .into_result()
        .unwrap();
    assert!(stale.conflict().is_some());
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_none());

    let refreshed = runtime.evaluate_connection(edit).unwrap();
    runtime
        .execute(GraphCommand::connection(refreshed.version(), edit))
        .into_result()
        .unwrap()
        .applied()
        .expect("refreshed connection applies");
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_some());
}

#[test]
fn connection_evaluation_and_execution_observe_the_same_pure_profile_policy() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.alternate_source_port)
                .map(|port| port.ty = Some(TypeDesc::String))
        })
        .expect("alternate source port");
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.ty = Some(TypeDesc::Int))
        })
        .expect("target port");
    let query_count = Rc::new(Cell::new(0));
    let runtime = profiled_runtime(
        graph,
        Box::new(CountingGraphTypesProfile {
            query_count: query_count.clone(),
        }),
    )
    .unwrap();
    let edge = EdgeId::from_u128(92_001);
    let edit = ConnectionEdit::connect(
        edge,
        ids.alternate_source_port,
        ids.target_port,
        NodeGraphConnectionMode::Strict,
    );
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let evaluation = runtime.evaluate_connection(edit).unwrap();
    assert!(!evaluation.is_accept());
    assert_eq!(evaluation.revision(), 0);
    assert_eq!(evaluation.diagnostics()[0].key, "connect.type_mismatch");
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);

    let outcome = runtime
        .execute(GraphCommand::connection(evaluation.version(), edit))
        .into_result()
        .unwrap();
    let rejection = outcome.rejection().expect("execution rejects identically");
    assert_eq!(
        rejection.diagnostics()[0].key,
        evaluation.diagnostics()[0].key
    );
    assert_eq!(
        rejection.diagnostics()[0].target,
        evaluation.diagnostics()[0].target
    );
    assert_eq!(rejection.planning_work(), evaluation.planning_work());
    assert!(
        query_count.get() >= 4,
        "both queries resolved both endpoint types"
    );
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);
}

#[test]
fn profile_cycle_policy_rejects_only_the_cycle_closing_connection() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = profiled_runtime(graph, Box::new(NoDataCyclesProfile)).unwrap();
    let a_in = PortId::from_u128(231);
    let a_out = PortId::from_u128(232);
    let b_in = PortId::from_u128(233);
    let b_out = PortId::from_u128(234);

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new()
                .replace_node_ports(
                    nodes[0],
                    [
                        TopologyPort::new(a_in, port(nodes[0], "in", PortDirection::In)),
                        TopologyPort::new(a_out, port(nodes[0], "out", PortDirection::Out)),
                    ],
                    IncidentEdgePolicy::Reject,
                )
                .replace_node_ports(
                    nodes[1],
                    [
                        TopologyPort::new(b_in, port(nodes[1], "in", PortDirection::In)),
                        TopologyPort::new(b_out, port(nodes[1], "out", PortDirection::Out)),
                    ],
                    IncidentEdgePolicy::Reject,
                ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("ports apply");
    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            EdgeId::from_u128(235),
            a_out,
            b_in,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("acyclic connection applies");
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();

    let outcome = runtime
        .execute(GraphCommand::connect(
            before.version(),
            EdgeId::from_u128(236),
            b_out,
            a_in,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap();

    let rejection = outcome.rejection().expect("cycle closes and rejects");
    assert_eq!(rejection.diagnostics()[0].key, "connect.cycle");
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
}

#[test]
fn history_replay_does_not_run_profile_concretization_again() {
    let (graph, nodes) = graph_with_nodes(1);
    let dynamic_port = PortId::from_u128(130);
    let concretize_calls = Rc::new(Cell::new(0));
    let runtime = profiled_runtime(
        graph,
        Box::new(DerivePortOnce {
            node: nodes[0],
            port: dynamic_port,
            concretize_calls: concretize_calls.clone(),
        }),
    )
    .unwrap();

    runtime
        .execute(GraphCommand::set_node_position(
            runtime.snapshot().unwrap().version(),
            nodes[0],
            CanvasPoint { x: 10.0, y: 20.0 },
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("profiled command applies");
    assert_eq!(concretize_calls.get(), 2, "initial fixed point converges");
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .port(&dynamic_port)
            .is_some()
    );

    undo(&runtime);
    assert_eq!(
        concretize_calls.get(),
        2,
        "undo replays recorded operations without re-running profile derivation"
    );
    let snapshot = runtime.snapshot().unwrap();
    let node = snapshot.graph().node(&nodes[0]).unwrap();
    assert_eq!(node.pos, CanvasPoint { x: 0.0, y: 0.0 });
    assert!(snapshot.graph().port(&dynamic_port).is_none());

    redo(&runtime);
    assert_eq!(
        concretize_calls.get(),
        2,
        "redo replays recorded operations without re-running profile derivation"
    );
    let snapshot = runtime.snapshot().unwrap();
    let node = snapshot.graph().node(&nodes[0]).unwrap();
    assert_eq!(node.pos, CanvasPoint { x: 10.0, y: 20.0 });
    assert!(snapshot.graph().port(&dynamic_port).is_some());
}

#[test]
fn profile_rejected_undo_preserves_graph_history_and_notifications() {
    let (graph, nodes) = graph_with_nodes(1);
    let reject_null = Rc::new(Cell::new(false));
    let runtime = profiled_runtime(
        graph,
        Box::new(RejectNullNodeDataWhenEnabled {
            node: nodes[0],
            reject_null: reject_null.clone(),
        }),
    )
    .unwrap();
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("seed command applies");
    reject_null.set(true);
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let outcome = runtime
        .execute(GraphCommand::undo(before.version()))
        .into_result()
        .unwrap();
    let rejection = outcome.rejection().expect("profile rejects undo replay");

    assert_eq!(
        rejection.diagnostics()[0].key,
        "profile.history_revert_forbidden"
    );
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);
}

#[test]
fn panicking_profile_validation_does_not_consume_prepared_history() {
    struct PanicOnHistoryValidation {
        armed: Rc<Cell<bool>>,
    }

    impl GraphProfile for PanicOnHistoryValidation {
        fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
            None
        }

        fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
            assert!(!self.armed.get(), "history profile validation panic");
            Vec::new()
        }
    }

    let (graph, nodes) = graph_with_nodes(1);
    let armed = Rc::new(Cell::new(false));
    let runtime = profiled_runtime(
        graph,
        Box::new(PanicOnHistoryValidation {
            armed: armed.clone(),
        }),
    )
    .unwrap();
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("seed command applies");

    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    armed.set(true);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime
            .execute(GraphCommand::undo(before.version()))
            .into_result()
    }));
    assert!(panic.is_err());
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);

    armed.set(false);
    undo(&runtime);
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&nodes[0])
            .expect("history node")
            .data
            .is_null()
    );
}

#[test]
fn multi_round_profile_derivation_is_one_exact_history_entry() {
    let (graph, nodes) = graph_with_nodes(1);
    let before = frozen_graph_value(&graph);
    let derived_port = PortId::from_u128(131);
    let concretize_calls = Rc::new(Cell::new(0));
    let runtime = profiled_runtime(
        graph,
        Box::new(DerivePayloadThenPort {
            node: nodes[0],
            port: derived_port,
            concretize_calls: concretize_calls.clone(),
        }),
    )
    .unwrap();

    runtime
        .execute(
            GraphCommand::set_node_data(
                runtime.snapshot().unwrap().version(),
                nodes[0],
                serde_json::json!({ "value": "user" }),
            )
            .with_label("update node and derived topology"),
        )
        .into_result()
        .unwrap()
        .applied()
        .expect("profiled command applies");

    assert_eq!(concretize_calls.get(), 3);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let after = frozen_graph_value(runtime.snapshot().unwrap().graph());
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().data,
        serde_json::json!({ "value": "user", "derived": true })
    );
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().ports,
        [derived_port]
    );
    assert!(snapshot.graph().port(&derived_port).is_some());

    undo(&runtime);
    assert_eq!(concretize_calls.get(), 3);
    assert_eq!(
        frozen_graph_value(runtime.snapshot().unwrap().graph()),
        before
    );
    redo(&runtime);
    assert_eq!(concretize_calls.get(), 3);
    assert_eq!(
        frozen_graph_value(runtime.snapshot().unwrap().graph()),
        after
    );
}

#[test]
fn multi_round_profile_journal_preserves_label_and_operation_order() {
    let (graph, nodes) = graph_with_nodes(1);
    let index = GraphIndex::from_graph(&graph);
    let derived_port = PortId::from_u128(132);
    let concretize_calls = Rc::new(Cell::new(0));
    let profile = DerivePayloadThenPort {
        node: nodes[0],
        port: derived_port,
        concretize_calls: concretize_calls.clone(),
    };
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let command = GraphCommand::set_node_data(
        runtime.snapshot().unwrap().version(),
        nodes[0],
        serde_json::json!({ "value": "user" }),
    )
    .with_label("ordered profile journal");

    let planned = plan_graph_command(
        &graph,
        &index,
        &NodeCatalog::new(),
        &command,
        Some(&profile),
        &crate::runtime::policy::GraphRuntimePolicy::default(),
    )
    .unwrap();

    assert_eq!(concretize_calls.get(), 3);
    assert_eq!(planned.transaction.label(), Some("ordered profile journal"));
    assert_eq!(planned.transaction.ops().len(), 3);
    assert!(matches!(
        &planned.transaction.ops()[0],
        GraphOp::SetNodeData { id, from, to }
            if *id == nodes[0]
                && *from == serde_json::Value::Null
                && *to == serde_json::json!({ "value": "user", "derived": true })
    ));
    assert!(matches!(
        &planned.transaction.ops()[1],
        GraphOp::AddPort { id, port }
            if *id == derived_port && port.node == nodes[0]
    ));
    assert!(matches!(
        &planned.transaction.ops()[2],
        GraphOp::SetNodePorts { id, from, to }
            if *id == nodes[0] && from.is_empty() && to == &[derived_port]
    ));
    assert_eq!(planned.planning_work.fact_reads(), 8);
    assert_eq!(planned.planning_work.operations_emitted(), 4);
}

#[test]
fn profile_validation_rejection_after_derivation_has_zero_runtime_side_effects() {
    let (graph, nodes) = graph_with_nodes(1);
    let derived_port = PortId::from_u128(133);
    let runtime = profiled_runtime(
        graph,
        Box::new(RejectDerivedPort {
            node: nodes[0],
            port: derived_port,
        }),
    )
    .unwrap();
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let outcome = runtime
        .execute(GraphCommand::set_node_position(
            before.version(),
            nodes[0],
            CanvasPoint { x: 10.0, y: 20.0 },
        ))
        .into_result()
        .unwrap();

    let rejection = outcome.rejection().expect("profile rejects derived graph");
    assert_eq!(
        rejection.diagnostics()[0].key,
        "profile.derived_port_forbidden"
    );
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);
}

#[test]
fn non_convergent_profile_has_zero_runtime_side_effects() {
    let (graph, nodes) = graph_with_nodes(1);
    let concretize_calls = Rc::new(Cell::new(0));
    let runtime = profiled_runtime(
        graph,
        Box::new(OscillatingProfile {
            node: nodes[0],
            concretize_calls: concretize_calls.clone(),
        }),
    )
    .unwrap();
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let outcome = runtime
        .execute(GraphCommand::set_node_position(
            before.version(),
            nodes[0],
            CanvasPoint { x: 10.0, y: 20.0 },
        ))
        .into_result()
        .unwrap();

    assert!(outcome.rejection().is_some());
    assert_eq!(concretize_calls.get(), 3);
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);
}
