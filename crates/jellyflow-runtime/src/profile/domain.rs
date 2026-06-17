use serde::{Deserialize, Serialize};

use jellyflow_core::core::{EdgeKind, NodeKindKey, PortKey};
use jellyflow_core::types::TypeDesc;

/// Renderer-neutral domain metadata exposed by a graph profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GraphProfileMetadata {
    /// Stable profile key for adapters and docs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Human-readable title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Node field schemas keyed by node kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_fields: Vec<NodeFieldSchemaSet>,
    /// Variable surfaces available to nodes or edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variable_surfaces: Vec<VariableSurfaceDescriptor>,
    /// Connection rule labels for adapter display.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connection_rules: Vec<ConnectionRuleDescriptor>,
}

impl GraphProfileMetadata {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: Some(key.into()),
            title: Some(title.into()),
            ..Self::default()
        }
    }

    pub fn with_node_fields(mut self, fields: NodeFieldSchemaSet) -> Self {
        self.node_fields.push(fields);
        self
    }

    pub fn with_variable_surface(mut self, surface: VariableSurfaceDescriptor) -> Self {
        self.variable_surfaces.push(surface);
        self
    }

    pub fn with_connection_rule(mut self, rule: ConnectionRuleDescriptor) -> Self {
        self.connection_rules.push(rule);
        self
    }
}

/// Field schemas attached to one node kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeFieldSchemaSet {
    pub node_kind: NodeKindKey,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldSchema>,
}

impl NodeFieldSchemaSet {
    pub fn new(node_kind: impl Into<NodeKindKey>) -> Self {
        Self {
            node_kind: node_kind.into(),
            fields: Vec::new(),
        }
    }

    pub fn with_field(mut self, field: FieldSchema) -> Self {
        self.fields.push(field);
        self
    }
}

/// Renderer-neutral node parameter field schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeDesc>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<ValidationHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_anchor: Option<PortKey>,
}

impl FieldSchema {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            ty: None,
            required: false,
            hints: Vec::new(),
            port_anchor: None,
        }
    }

    pub fn with_type(mut self, ty: TypeDesc) -> Self {
        self.ty = Some(ty);
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_hint(mut self, hint: ValidationHint) -> Self {
        self.hints.push(hint);
        self
    }

    pub fn with_port_anchor(mut self, port: impl Into<PortKey>) -> Self {
        self.port_anchor = Some(port.into());
        self
    }
}

/// Adapter-facing validation hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationHint {
    pub code: String,
    pub message: String,
}

impl ValidationHint {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Variables available from one domain surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableSurfaceDescriptor {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<VariableDescriptor>,
}

impl VariableSurfaceDescriptor {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            variables: Vec::new(),
        }
    }

    pub fn with_variable(mut self, variable: VariableDescriptor) -> Self {
        self.variables.push(variable);
        self
    }
}

/// One variable exposed by a domain profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDescriptor {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeDesc>,
}

impl VariableDescriptor {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            ty: None,
        }
    }

    pub fn with_type(mut self, ty: TypeDesc) -> Self {
        self.ty = Some(ty);
        self
    }
}

/// Human-readable connection rule metadata for adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionRuleDescriptor {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_kind: Option<EdgeKind>,
}

impl ConnectionRuleDescriptor {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            edge_kind: None,
        }
    }

    pub fn for_edge_kind(mut self, edge_kind: EdgeKind) -> Self {
        self.edge_kind = Some(edge_kind);
        self
    }
}
