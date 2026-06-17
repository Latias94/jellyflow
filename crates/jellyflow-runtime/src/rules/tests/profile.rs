use super::fixtures::{insert_node, insert_typed_data_input, insert_typed_data_output};

use crate::profile::{
    ConnectionRuleDescriptor, FieldSchema, GraphProfile, GraphProfileMetadata, NodeFieldSchemaSet,
    ValidationHint, VariableDescriptor, VariableSurfaceDescriptor,
};
use crate::rules::{Diagnostic, DiagnosticTarget};
use jellyflow_core::core::{EdgeId, EdgeKind, Graph, GraphBuilder, NodeId, PortCapacity, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::TypeDesc;

#[test]
fn graph_profile_default_plan_connect_uses_type_of_port() {
    struct TypedProfile;

    impl GraphProfile for TypedProfile {
        fn type_of_port(&mut self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            graph.ports().get(&port).and_then(|port| port.ty.clone())
        }

        fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
            Vec::new()
        }
    }

    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_typed_data_output(
        &mut graph,
        out,
        a,
        "out",
        PortCapacity::Multi,
        TypeDesc::Int,
    );
    insert_typed_data_input(
        &mut graph,
        inn,
        b,
        "in",
        PortCapacity::Single,
        TypeDesc::String,
    );

    let mut profile = TypedProfile;
    let plan = profile.plan_connect(&graph, out, inn, NodeGraphConnectionMode::Strict);

    assert!(plan.is_reject());
    assert!(plan.ops().is_empty());
}

#[test]
fn graph_profile_metadata_describes_fields_variables_and_connection_rules() {
    let metadata = GraphProfileMetadata::new("workflow", "Workflow")
        .with_node_fields(
            NodeFieldSchemaSet::new("workflow.llm").with_field(
                FieldSchema::new("prompt", "Prompt")
                    .with_type(TypeDesc::String)
                    .required()
                    .with_hint(ValidationHint::new(
                        "workflow.prompt_required",
                        "Prompt is required",
                    ))
                    .with_port_anchor("prompt"),
            ),
        )
        .with_variable_surface(
            VariableSurfaceDescriptor::new("inputs", "Inputs").with_variable(
                VariableDescriptor::new("topic", "Topic").with_type(TypeDesc::String),
            ),
        )
        .with_connection_rule(
            ConnectionRuleDescriptor::new("exec.dag", "Execution flow must be acyclic")
                .for_edge_kind(EdgeKind::Exec),
        );

    let value = serde_json::to_value(&metadata).expect("serialize metadata");
    let roundtrip: GraphProfileMetadata =
        serde_json::from_value(value).expect("deserialize metadata");

    assert_eq!(roundtrip.key.as_deref(), Some("workflow"));
    assert_eq!(roundtrip.node_fields[0].fields[0].key, "prompt");
    assert!(roundtrip.node_fields[0].fields[0].required);
    assert_eq!(
        roundtrip.node_fields[0].fields[0].port_anchor,
        Some(jellyflow_core::core::PortKey::new("prompt"))
    );
    assert_eq!(roundtrip.variable_surfaces[0].variables[0].key, "topic");
    assert_eq!(
        roundtrip.connection_rules[0].edge_kind,
        Some(EdgeKind::Exec)
    );
}

#[test]
fn graph_profile_can_return_structured_metadata_and_targeted_diagnostics() {
    struct WorkflowProfile;

    impl GraphProfile for WorkflowProfile {
        fn metadata(&self) -> GraphProfileMetadata {
            GraphProfileMetadata::new("workflow", "Workflow").with_connection_rule(
                ConnectionRuleDescriptor::new("mind.single_parent", "Idea nodes have one parent"),
            )
        }

        fn type_of_port(&mut self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
            None
        }

        fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
            vec![
                Diagnostic::error(
                    "workflow.node.required_field",
                    DiagnosticTarget::Node {
                        id: NodeId::from_u128(1),
                    },
                    "node is missing a required field",
                ),
                Diagnostic::error(
                    "workflow.port.unbound",
                    DiagnosticTarget::Port {
                        id: PortId::from_u128(2),
                    },
                    "port is not bound",
                ),
                Diagnostic::error(
                    "workflow.edge.invalid_branch",
                    DiagnosticTarget::Edge {
                        id: EdgeId::from_u128(3),
                    },
                    "edge branch is invalid",
                ),
            ]
        }
    }

    let mut profile = WorkflowProfile;
    let diagnostics = profile.validate_graph(&Graph::default());

    assert_eq!(
        profile.metadata().connection_rules[0].key,
        "mind.single_parent"
    );
    assert!(diagnostics.iter().any(
        |diagnostic| matches!(diagnostic.target, DiagnosticTarget::Node { id } if id == NodeId::from_u128(1))
    ));
    assert!(diagnostics.iter().any(
        |diagnostic| matches!(diagnostic.target, DiagnosticTarget::Port { id } if id == PortId::from_u128(2))
    ));
    assert!(diagnostics.iter().any(
        |diagnostic| matches!(diagnostic.target, DiagnosticTarget::Edge { id } if id == EdgeId::from_u128(3))
    ));
}
