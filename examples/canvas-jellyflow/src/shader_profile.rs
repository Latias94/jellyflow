use jellyflow_core::types::TypeDesc;
use jellyflow_core::{EdgeKind, Graph, PortId};
use jellyflow_runtime::graph::Diagnostic;
use jellyflow_runtime::profile::{GraphProfile, PortTypeImpactContext, PortTypeRevalidation};

/// Product policy for Shader Graph and Blueprint data-flow documents.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShaderGraphProfile;

impl GraphProfile for ShaderGraphProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn port_types_affected_by_mutation(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
    ) -> PortTypeRevalidation {
        // Shader types are stored on ports, so the runtime's edited-port seeds are complete.
        PortTypeRevalidation::Exact(Vec::new())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn allow_cycles(&self, edge_kind: EdgeKind) -> bool {
        edge_kind != EdgeKind::Data
    }
}
