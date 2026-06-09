use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasPoint, CanvasSize, Edge, EdgeId, Node, NodeId};

/// Adapter-owned node array element for exact XyFlow `applyNodeChanges` semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XyFlowNodeElement {
    pub id: NodeId,
    pub node: Node,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dragging: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resizing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured: Option<CanvasSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
}

impl XyFlowNodeElement {
    pub fn new(id: NodeId, node: Node) -> Self {
        Self {
            id,
            node,
            selected: None,
            dragging: None,
            resizing: None,
            measured: None,
            width: None,
            height: None,
        }
    }
}

/// Adapter-owned edge array element for exact XyFlow `applyEdgeChanges` semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XyFlowEdgeElement {
    pub id: EdgeId,
    pub edge: Edge,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
}

impl XyFlowEdgeElement {
    pub fn new(id: EdgeId, edge: Edge) -> Self {
        Self {
            id,
            edge,
            selected: None,
        }
    }
}

/// XyFlow's `setAttributes` dimension switch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XyFlowDimensionAttribute {
    Width,
    Height,
}

/// XyFlow's `boolean | 'width' | 'height'` dimension attribute contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
pub enum XyFlowDimensionsSetAttributes {
    Bool(bool),
    Attribute(XyFlowDimensionAttribute),
}

impl XyFlowDimensionsSetAttributes {
    pub fn writes_width(self) -> bool {
        matches!(
            self,
            Self::Bool(true) | Self::Attribute(XyFlowDimensionAttribute::Width)
        )
    }

    pub fn writes_height(self) -> bool {
        matches!(
            self,
            Self::Bool(true) | Self::Attribute(XyFlowDimensionAttribute::Height)
        )
    }
}

/// XyFlow node changes as applied to adapter-owned ordered node arrays.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum XyFlowNodeChange {
    Dimensions {
        id: NodeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dimensions: Option<CanvasSize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resizing: Option<bool>,
        #[serde(
            default,
            rename = "setAttributes",
            alias = "set_attributes",
            skip_serializing_if = "Option::is_none"
        )]
        set_attributes: Option<XyFlowDimensionsSetAttributes>,
    },
    Position {
        id: NodeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<CanvasPoint>,
        #[serde(
            default,
            rename = "positionAbsolute",
            alias = "position_absolute",
            skip_serializing_if = "Option::is_none"
        )]
        position_absolute: Option<CanvasPoint>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dragging: Option<bool>,
    },
    Select {
        id: NodeId,
        selected: bool,
    },
    Remove {
        id: NodeId,
    },
    Add {
        item: XyFlowNodeElement,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    Replace {
        id: NodeId,
        item: XyFlowNodeElement,
    },
}

/// XyFlow edge changes as applied to adapter-owned ordered edge arrays.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum XyFlowEdgeChange {
    Select {
        id: EdgeId,
        selected: bool,
    },
    Remove {
        id: EdgeId,
    },
    Add {
        item: XyFlowEdgeElement,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    Replace {
        id: EdgeId,
        item: XyFlowEdgeElement,
    },
}

pub fn apply_xyflow_node_changes(
    changes: &[XyFlowNodeChange],
    nodes: &[XyFlowNodeElement],
) -> Vec<XyFlowNodeElement> {
    let mut planner = XyFlowNodeApplyPlanner::new(changes, nodes);
    planner.apply()
}

pub fn apply_xyflow_edge_changes(
    changes: &[XyFlowEdgeChange],
    edges: &[XyFlowEdgeElement],
) -> Vec<XyFlowEdgeElement> {
    let mut planner = XyFlowEdgeApplyPlanner::new(changes, edges);
    planner.apply()
}

struct XyFlowNodeApplyPlanner<'a> {
    changes: &'a [XyFlowNodeChange],
    nodes: &'a [XyFlowNodeElement],
    changes_by_id: BTreeMap<NodeId, Vec<&'a XyFlowNodeChange>>,
    add_changes: Vec<&'a XyFlowNodeChange>,
}

impl<'a> XyFlowNodeApplyPlanner<'a> {
    fn new(changes: &'a [XyFlowNodeChange], nodes: &'a [XyFlowNodeElement]) -> Self {
        Self {
            changes,
            nodes,
            changes_by_id: BTreeMap::new(),
            add_changes: Vec::new(),
        }
    }

    fn apply(&mut self) -> Vec<XyFlowNodeElement> {
        self.index_changes();
        let mut updated = self.apply_existing_nodes();
        self.apply_adds(&mut updated);
        updated
    }

    fn index_changes(&mut self) {
        for change in self.changes {
            match change {
                XyFlowNodeChange::Add { .. } => self.add_changes.push(change),
                XyFlowNodeChange::Remove { id } | XyFlowNodeChange::Replace { id, .. } => {
                    self.changes_by_id.insert(*id, vec![change]);
                }
                XyFlowNodeChange::Dimensions { id, .. }
                | XyFlowNodeChange::Position { id, .. }
                | XyFlowNodeChange::Select { id, .. } => {
                    self.changes_by_id.entry(*id).or_default().push(change);
                }
            }
        }
    }

    fn apply_existing_nodes(&self) -> Vec<XyFlowNodeElement> {
        let mut updated = Vec::with_capacity(self.nodes.len() + self.add_changes.len());

        for node in self.nodes {
            let Some(changes) = self.changes_by_id.get(&node.id) else {
                updated.push(node.clone());
                continue;
            };

            match changes.first() {
                Some(XyFlowNodeChange::Remove { .. }) => continue,
                Some(XyFlowNodeChange::Replace { item, .. }) => {
                    updated.push(item.clone());
                    continue;
                }
                _ => {}
            }

            let mut node = node.clone();
            for change in changes {
                apply_xyflow_node_change(change, &mut node);
            }
            updated.push(node);
        }

        updated
    }

    fn apply_adds(&self, updated: &mut Vec<XyFlowNodeElement>) {
        for change in &self.add_changes {
            let XyFlowNodeChange::Add { item, index } = change else {
                continue;
            };
            if let Some(index) = index {
                updated.insert((*index).min(updated.len()), item.clone());
            } else {
                updated.push(item.clone());
            }
        }
    }
}

fn apply_xyflow_node_change(change: &XyFlowNodeChange, node: &mut XyFlowNodeElement) {
    match change {
        XyFlowNodeChange::Dimensions {
            dimensions,
            resizing,
            set_attributes,
            ..
        } => {
            if let Some(dimensions) = dimensions {
                node.measured = Some(*dimensions);
                if let Some(set_attributes) = set_attributes {
                    if set_attributes.writes_width() {
                        node.width = Some(dimensions.width);
                    }
                    if set_attributes.writes_height() {
                        node.height = Some(dimensions.height);
                    }
                }
            }
            if let Some(resizing) = resizing {
                node.resizing = Some(*resizing);
            }
        }
        XyFlowNodeChange::Position {
            position, dragging, ..
        } => {
            if let Some(position) = position {
                node.node.pos = *position;
            }
            if let Some(dragging) = dragging {
                node.dragging = Some(*dragging);
            }
        }
        XyFlowNodeChange::Select { selected, .. } => {
            node.selected = Some(*selected);
        }
        XyFlowNodeChange::Remove { .. }
        | XyFlowNodeChange::Add { .. }
        | XyFlowNodeChange::Replace { .. } => {}
    }
}

struct XyFlowEdgeApplyPlanner<'a> {
    changes: &'a [XyFlowEdgeChange],
    edges: &'a [XyFlowEdgeElement],
    changes_by_id: BTreeMap<EdgeId, Vec<&'a XyFlowEdgeChange>>,
    add_changes: Vec<&'a XyFlowEdgeChange>,
}

impl<'a> XyFlowEdgeApplyPlanner<'a> {
    fn new(changes: &'a [XyFlowEdgeChange], edges: &'a [XyFlowEdgeElement]) -> Self {
        Self {
            changes,
            edges,
            changes_by_id: BTreeMap::new(),
            add_changes: Vec::new(),
        }
    }

    fn apply(&mut self) -> Vec<XyFlowEdgeElement> {
        self.index_changes();
        let mut updated = self.apply_existing_edges();
        self.apply_adds(&mut updated);
        updated
    }

    fn index_changes(&mut self) {
        for change in self.changes {
            match change {
                XyFlowEdgeChange::Add { .. } => self.add_changes.push(change),
                XyFlowEdgeChange::Remove { id } | XyFlowEdgeChange::Replace { id, .. } => {
                    self.changes_by_id.insert(*id, vec![change]);
                }
                XyFlowEdgeChange::Select { id, .. } => {
                    self.changes_by_id.entry(*id).or_default().push(change);
                }
            }
        }
    }

    fn apply_existing_edges(&self) -> Vec<XyFlowEdgeElement> {
        let mut updated = Vec::with_capacity(self.edges.len() + self.add_changes.len());

        for edge in self.edges {
            let Some(changes) = self.changes_by_id.get(&edge.id) else {
                updated.push(edge.clone());
                continue;
            };

            match changes.first() {
                Some(XyFlowEdgeChange::Remove { .. }) => continue,
                Some(XyFlowEdgeChange::Replace { item, .. }) => {
                    updated.push(item.clone());
                    continue;
                }
                _ => {}
            }

            let mut edge = edge.clone();
            for change in changes {
                apply_xyflow_edge_change(change, &mut edge);
            }
            updated.push(edge);
        }

        updated
    }

    fn apply_adds(&self, updated: &mut Vec<XyFlowEdgeElement>) {
        for change in &self.add_changes {
            let XyFlowEdgeChange::Add { item, index } = change else {
                continue;
            };
            if let Some(index) = index {
                updated.insert((*index).min(updated.len()), item.clone());
            } else {
                updated.push(item.clone());
            }
        }
    }
}

fn apply_xyflow_edge_change(change: &XyFlowEdgeChange, edge: &mut XyFlowEdgeElement) {
    match change {
        XyFlowEdgeChange::Select { selected, .. } => {
            edge.selected = Some(*selected);
        }
        XyFlowEdgeChange::Remove { .. }
        | XyFlowEdgeChange::Add { .. }
        | XyFlowEdgeChange::Replace { .. } => {}
    }
}
