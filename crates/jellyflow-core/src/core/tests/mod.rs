use crate::core::{
    Binding, BindingEndpoint, BindingId, CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphId,
    GraphImport, GraphImportError, GraphLocalBindingTarget, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind, SourceAnchor, Symbol, SymbolId,
    collect_subgraph_targets, collect_symbol_ref_targets, resolve_import_closure, validate_graph,
};
use crate::core::{CanvasSize, GraphValidationError, GroupId, validate_graph_structural};
use crate::core::{
    SUBGRAPH_NODE_KIND, SYMBOL_REF_NODE_KIND, validate_subgraph_targets_are_imported,
    validate_symbol_ref_targets_are_declared,
};

fn make_node(kind: &str) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 0,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn make_port(
    node: NodeId,
    key: &str,
    dir: PortDirection,
    kind: PortKind,
    capacity: PortCapacity,
) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir,
        kind,
        capacity,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn attach_port(graph: &mut Graph, port_id: PortId, port: Port) {
    let node_id = port.node;
    graph.ports.insert(port_id, port);
    graph
        .nodes
        .get_mut(&node_id)
        .expect("port owner node")
        .ports
        .push(port_id);
}

mod binding;
mod imports;
mod structural;
mod subgraph;
mod symbol_ref;
