use serde::{Deserialize, Serialize};

use crate::core::{
    Binding, BindingEndpoint, BindingId, CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId,
    EdgeKind, EdgeReconnectable, GraphId, GraphImport, Group, GroupId, Node, NodeExtent, NodeId,
    NodeKindKey, NodeOrigin, Port, PortId, StickyNote, StickyNoteId, Symbol, SymbolId,
};
use crate::types::TypeDesc;

use super::endpoints::EdgeEndpoints;

/// A reversible edit operation.
///
/// Destructive variants carry the removed data so the operation can be inverted for undo/redo.
/// Higher-level tools should batch multiple ops into a single transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GraphOp {
    /// Adds a node.
    AddNode { id: NodeId, node: Node },
    /// Removes a node.
    ///
    /// This operation is expected to remove associated ports and edges as well.
    RemoveNode {
        id: NodeId,
        node: Node,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ports: Vec<(PortId, Port)>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        edges: Vec<(EdgeId, Edge)>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bindings: Vec<(BindingId, Binding)>,
    },
    /// Sets a node position.
    SetNodePos {
        id: NodeId,
        from: CanvasPoint,
        to: CanvasPoint,
    },
    /// Sets a node origin override.
    SetNodeOrigin {
        id: NodeId,
        from: Option<NodeOrigin>,
        to: Option<NodeOrigin>,
    },
    /// Sets a node kind identifier.
    SetNodeKind {
        id: NodeId,
        from: NodeKindKey,
        to: NodeKindKey,
    },
    /// Sets a node kind version (for per-kind migrations).
    SetNodeKindVersion { id: NodeId, from: u32, to: u32 },
    /// Sets a node selectable override.
    SetNodeSelectable {
        id: NodeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a node focusable override.
    SetNodeFocusable {
        id: NodeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a node draggable override.
    SetNodeDraggable {
        id: NodeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a node connectable override.
    SetNodeConnectable {
        id: NodeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a node deletable override.
    SetNodeDeletable {
        id: NodeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a node parent container (group frame).
    SetNodeParent {
        id: NodeId,
        from: Option<GroupId>,
        to: Option<GroupId>,
    },
    /// Sets a node extent override.
    SetNodeExtent {
        id: NodeId,
        from: Option<NodeExtent>,
        to: Option<NodeExtent>,
    },
    /// Sets a node expand-parent override.
    SetNodeExpandParent {
        id: NodeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a node explicit size.
    SetNodeSize {
        id: NodeId,
        from: Option<CanvasSize>,
        to: Option<CanvasSize>,
    },
    /// Sets a node hidden state.
    SetNodeHidden { id: NodeId, from: bool, to: bool },
    /// Sets a node collapsed state.
    SetNodeCollapsed { id: NodeId, from: bool, to: bool },
    /// Sets a node's port ordering.
    SetNodePorts {
        id: NodeId,
        from: Vec<PortId>,
        to: Vec<PortId>,
    },
    /// Sets a node's domain-owned data payload.
    ///
    /// This is the primary edit op for node parameters and is intentionally untyped at the model
    /// layer: typing and validation live in profiles/rules.
    SetNodeData {
        id: NodeId,
        from: serde_json::Value,
        to: serde_json::Value,
    },

    /// Adds a port.
    AddPort { id: PortId, port: Port },
    /// Removes a port.
    ///
    /// This operation is expected to remove associated edges as well.
    RemovePort {
        id: PortId,
        port: Port,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        edges: Vec<(EdgeId, Edge)>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bindings: Vec<(BindingId, Binding)>,
    },
    /// Sets a port connectable override.
    SetPortConnectable {
        id: PortId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a port start-connectable override.
    SetPortConnectableStart {
        id: PortId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a port end-connectable override.
    SetPortConnectableEnd {
        id: PortId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets a port type descriptor.
    SetPortType {
        id: PortId,
        from: Option<TypeDesc>,
        to: Option<TypeDesc>,
    },
    /// Sets a port domain-owned data payload.
    SetPortData {
        id: PortId,
        from: serde_json::Value,
        to: serde_json::Value,
    },

    /// Adds an edge.
    AddEdge { id: EdgeId, edge: Edge },
    /// Removes an edge.
    RemoveEdge {
        id: EdgeId,
        edge: Edge,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bindings: Vec<(BindingId, Binding)>,
    },
    /// Sets an edge kind.
    SetEdgeKind {
        id: EdgeId,
        from: EdgeKind,
        to: EdgeKind,
    },
    /// Sets an edge selectable override.
    SetEdgeSelectable {
        id: EdgeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets an edge focusable override.
    SetEdgeFocusable {
        id: EdgeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets an edge hidden flag.
    SetEdgeHidden { id: EdgeId, from: bool, to: bool },
    /// Sets an edge hit-test interaction width override.
    SetEdgeInteractionWidth {
        id: EdgeId,
        from: Option<f32>,
        to: Option<f32>,
    },
    /// Sets an edge deletable override.
    SetEdgeDeletable {
        id: EdgeId,
        from: Option<bool>,
        to: Option<bool>,
    },
    /// Sets an edge reconnectable override.
    SetEdgeReconnectable {
        id: EdgeId,
        from: Option<EdgeReconnectable>,
        to: Option<EdgeReconnectable>,
    },
    /// Sets an edge's endpoints (preserving edge identity for reconnection workflows).
    SetEdgeEndpoints {
        id: EdgeId,
        from: EdgeEndpoints,
        to: EdgeEndpoints,
    },

    /// Adds a graph import reference.
    AddImport { id: GraphId, import: GraphImport },
    /// Removes a graph import reference.
    RemoveImport { id: GraphId, import: GraphImport },
    /// Sets an import alias.
    SetImportAlias {
        id: GraphId,
        from: Option<String>,
        to: Option<String>,
    },

    /// Adds a symbol.
    AddSymbol { id: SymbolId, symbol: Symbol },
    /// Removes a symbol.
    RemoveSymbol { id: SymbolId, symbol: Symbol },
    /// Sets a symbol name.
    SetSymbolName {
        id: SymbolId,
        from: String,
        to: String,
    },
    /// Sets a symbol type descriptor.
    SetSymbolType {
        id: SymbolId,
        from: Option<TypeDesc>,
        to: Option<TypeDesc>,
    },
    /// Sets a symbol default value.
    SetSymbolDefaultValue {
        id: SymbolId,
        from: Option<serde_json::Value>,
        to: Option<serde_json::Value>,
    },
    /// Updates a symbol metadata payload (domain-owned).
    SetSymbolMeta {
        id: SymbolId,
        from: serde_json::Value,
        to: serde_json::Value,
    },

    /// Adds a group.
    AddGroup { id: GroupId, group: Group },
    /// Removes a group.
    ///
    /// This operation is expected to detach nodes that were parented to the group.
    RemoveGroup {
        id: GroupId,
        group: Group,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        detached: Vec<(NodeId, Option<GroupId>)>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bindings: Vec<(BindingId, Binding)>,
    },
    /// Sets a group's bounds.
    SetGroupRect {
        id: GroupId,
        from: CanvasRect,
        to: CanvasRect,
    },
    /// Sets a group's title.
    SetGroupTitle {
        id: GroupId,
        from: String,
        to: String,
    },
    /// Sets a group color override.
    SetGroupColor {
        id: GroupId,
        from: Option<String>,
        to: Option<String>,
    },

    /// Adds a sticky note.
    AddStickyNote { id: StickyNoteId, note: StickyNote },
    /// Removes a sticky note.
    RemoveStickyNote {
        id: StickyNoteId,
        note: StickyNote,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bindings: Vec<(BindingId, Binding)>,
    },
    /// Sets a sticky note text body.
    SetStickyNoteText {
        id: StickyNoteId,
        from: String,
        to: String,
    },
    /// Sets a sticky note bounds.
    SetStickyNoteRect {
        id: StickyNoteId,
        from: CanvasRect,
        to: CanvasRect,
    },
    /// Sets a sticky note color override.
    SetStickyNoteColor {
        id: StickyNoteId,
        from: Option<String>,
        to: Option<String>,
    },

    /// Adds a knowledge-canvas binding.
    AddBinding { id: BindingId, binding: Binding },
    /// Removes a knowledge-canvas binding.
    RemoveBinding { id: BindingId, binding: Binding },
    /// Sets a binding subject endpoint.
    SetBindingSubject {
        id: BindingId,
        from: BindingEndpoint,
        to: BindingEndpoint,
    },
    /// Sets a binding target endpoint.
    SetBindingTarget {
        id: BindingId,
        from: BindingEndpoint,
        to: BindingEndpoint,
    },
    /// Sets a binding relationship label.
    SetBindingKind {
        id: BindingId,
        from: Option<String>,
        to: Option<String>,
    },
    /// Updates a binding metadata payload (domain-owned).
    SetBindingMeta {
        id: BindingId,
        from: serde_json::Value,
        to: serde_json::Value,
    },
}
