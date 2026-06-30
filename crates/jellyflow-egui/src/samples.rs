use std::collections::BTreeMap;

use jellyflow::core::{
    CanvasPoint, CanvasSize, EdgeId, EdgeLabelAnchor, EdgeRouteKind, EdgeViewDescriptor, Graph,
    GraphId, GraphOp, GraphTransaction, NodeId, NodeKindKey, PortCapacity, PortDirection, PortId,
    PortKind,
};
use jellyflow::runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow::runtime::runtime::connection::ConnectEdgeRequest;
use jellyflow::runtime::runtime::create_node::CreateNodeRequest;
use jellyflow::runtime::schema::{
    NodeKitRegistry, NodeRegistry, NodeSchema, PortDecl, PortViewDescriptor,
};
use jellyflow::runtime::{DispatchOutcome, NodeGraphStore};
use serde_json::json;
use thiserror::Error;

use crate::state::LayoutPresetChoice;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SampleGraphKind {
    #[default]
    Workflow,
    AutomationBuilder,
    MindMap,
    Tree,
    OrgChart,
    KnowledgeBoard,
    Erd,
    ShaderGraph,
}

impl SampleGraphKind {
    pub const ALL: [Self; 8] = [
        Self::Workflow,
        Self::AutomationBuilder,
        Self::MindMap,
        Self::Tree,
        Self::OrgChart,
        Self::KnowledgeBoard,
        Self::Erd,
        Self::ShaderGraph,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Workflow => "Workflow",
            Self::AutomationBuilder => "Automation builder",
            Self::MindMap => "Mind map",
            Self::Tree => "Tree",
            Self::OrgChart => "Org chart",
            Self::KnowledgeBoard => "Knowledge board",
            Self::Erd => "ERD",
            Self::ShaderGraph => "Shader graph",
        }
    }

    pub fn default_layout(self) -> LayoutPresetChoice {
        match self {
            Self::Workflow | Self::AutomationBuilder => LayoutPresetChoice::Workflow,
            Self::MindMap => LayoutPresetChoice::MindMap,
            Self::Tree | Self::OrgChart => LayoutPresetChoice::Tree,
            Self::KnowledgeBoard | Self::Erd | Self::ShaderGraph => LayoutPresetChoice::Freeform,
        }
    }
}

#[derive(Debug, Error)]
pub enum SampleGraphError {
    #[error("sample graph create node failed: {0}")]
    Create(String),
    #[error("sample graph connect failed: {0}")]
    Connect(String),
    #[error("sample graph missing node alias: {0}")]
    MissingNode(String),
    #[error("sample graph missing port for `{node}` direction {direction:?}")]
    MissingPort {
        node: String,
        direction: PortDirection,
    },
}

pub(crate) struct SampleGraph {
    pub store: NodeGraphStore,
    pub registry: NodeRegistry,
    pub default_layout: LayoutPresetChoice,
}

pub(crate) fn sample_graph(kind: SampleGraphKind) -> Result<SampleGraph, SampleGraphError> {
    let registry = sample_node_registry();
    let mut builder = SampleGraphBuilder::new(registry.clone());
    match kind {
        SampleGraphKind::Workflow => populate_workflow(&mut builder)?,
        SampleGraphKind::AutomationBuilder => populate_automation_builder(&mut builder)?,
        SampleGraphKind::MindMap => populate_mind_map(&mut builder)?,
        SampleGraphKind::Tree => populate_tree(&mut builder)?,
        SampleGraphKind::OrgChart => populate_org_chart(&mut builder)?,
        SampleGraphKind::KnowledgeBoard => populate_knowledge_board(&mut builder)?,
        SampleGraphKind::Erd => populate_erd(&mut builder)?,
        SampleGraphKind::ShaderGraph => populate_shader_graph(&mut builder)?,
    }
    builder.fit_view();

    Ok(SampleGraph {
        store: builder.store,
        registry,
        default_layout: kind.default_layout(),
    })
}

fn populate_workflow(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    builder.node(
        "brief",
        "demo.start",
        "Intake brief",
        "Collect source notes and constraints",
        CanvasPoint { x: -420.0, y: 20.0 },
    )?;
    builder.node(
        "research",
        "demo.task",
        "Research",
        "Cluster sources and claims",
        CanvasPoint {
            x: -120.0,
            y: -80.0,
        },
    )?;
    builder.node(
        "decide",
        "demo.decision",
        "Decision",
        "Enough signal to publish?",
        CanvasPoint { x: 210.0, y: -80.0 },
    )?;
    builder.node(
        "draft",
        "demo.task",
        "Draft",
        "Write sections and citations",
        CanvasPoint {
            x: 520.0,
            y: -150.0,
        },
    )?;
    builder.node(
        "review",
        "demo.task",
        "Review",
        "Resolve comments",
        CanvasPoint { x: 520.0, y: 20.0 },
    )?;
    builder.node(
        "publish",
        "demo.output",
        "Publish",
        "Export article and graph",
        CanvasPoint { x: 840.0, y: -60.0 },
    )?;

    builder.connect("brief", "research")?;
    builder.connect("research", "decide")?;
    builder.connect("decide", "draft")?;
    builder.connect("decide", "review")?;
    builder.connect("draft", "publish")?;
    builder.connect("review", "publish")?;
    builder.apply_default_layout(SampleGraphKind::Workflow.default_layout());
    Ok(())
}

fn populate_automation_builder(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    for (alias, kind, title, summary, pos) in [
        (
            "trigger",
            "demo.trigger",
            "Webhook trigger",
            "Receives customer intake events",
            CanvasPoint {
                x: -620.0,
                y: -40.0,
            },
        ),
        (
            "normalize",
            "demo.tool",
            "Normalize JSON",
            "Maps request fields into variables",
            CanvasPoint {
                x: -300.0,
                y: -120.0,
            },
        ),
        (
            "classify",
            "demo.llm",
            "Classify request",
            "LLM chooses priority and route",
            CanvasPoint { x: 20.0, y: -120.0 },
        ),
        (
            "condition",
            "demo.switch",
            "Needs human review?",
            "Branch on confidence and policy",
            CanvasPoint {
                x: 350.0,
                y: -120.0,
            },
        ),
        (
            "notify",
            "demo.tool",
            "Notify assignee",
            "Posts a Slack task",
            CanvasPoint {
                x: 680.0,
                y: -210.0,
            },
        ),
        (
            "error",
            "demo.error",
            "Error path",
            "Capture failed tool calls",
            CanvasPoint { x: 680.0, y: 10.0 },
        ),
        (
            "output",
            "demo.workflow_output",
            "Workflow output",
            "Return ticket id and route",
            CanvasPoint {
                x: 1010.0,
                y: -100.0,
            },
        ),
    ] {
        builder.node(alias, kind, title, summary, pos)?;
    }

    builder.connect("trigger", "normalize")?;
    builder.connect("normalize", "classify")?;
    builder.connect("classify", "condition")?;
    builder.connect_ports("condition", "yes", "notify", "in")?;
    builder.connect_ports("condition", "no", "error", "error")?;
    builder.connect("notify", "output")?;
    builder.connect("error", "output")?;
    builder.apply_default_layout(SampleGraphKind::AutomationBuilder.default_layout());
    Ok(())
}

fn populate_mind_map(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    builder.node(
        "center",
        "demo.topic",
        "Product strategy",
        "MindNode-style radial map",
        CanvasPoint::default(),
    )?;
    for (alias, title, summary) in [
        ("users", "Users", "Researchers, builders, editors"),
        ("jobs", "Jobs", "Capture, connect, explain"),
        ("channels", "Channels", "Desktop, native, embedded"),
        ("risks", "Risks", "Trust, scale, migration"),
        ("metrics", "Metrics", "Retention and graph reuse"),
    ] {
        builder.node(alias, "demo.idea", title, summary, CanvasPoint::default())?;
        builder.connect("center", alias)?;
    }
    for (parent, alias, title, summary) in [
        ("users", "researchers", "Researchers", "Long-form synthesis"),
        ("users", "operators", "Operators", "Repeatable workflows"),
        ("jobs", "clip", "Clip", "Save source fragments"),
        ("jobs", "connect", "Connect", "Show evidence paths"),
        ("channels", "egui", "egui", "Native embedded demo"),
        ("channels", "web", "Web", "Future adapter target"),
        ("risks", "perf", "Performance", "Large visible graphs"),
        (
            "risks",
            "sync",
            "Collaboration",
            "CRDT-safe mutation boundary",
        ),
        ("metrics", "reuse", "Reuse", "Subgraphs imported again"),
        ("metrics", "share", "Share", "Readable exported maps"),
    ] {
        builder.node(alias, "demo.idea", title, summary, CanvasPoint::default())?;
        builder.connect(parent, alias)?;
    }
    builder.apply_default_layout(SampleGraphKind::MindMap.default_layout());
    Ok(())
}

fn populate_tree(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    builder.node(
        "root",
        "demo.topic",
        "Research outline",
        "Tidy tree for hierarchy",
        CanvasPoint::default(),
    )?;
    for (alias, title, summary) in [
        ("intro", "1. Context", "Why this topic matters"),
        ("method", "2. Method", "How evidence was collected"),
        ("findings", "3. Findings", "Key claims and support"),
        ("next", "4. Next steps", "Open decisions"),
    ] {
        builder.node(
            alias,
            "demo.section",
            title,
            summary,
            CanvasPoint::default(),
        )?;
        builder.connect("root", alias)?;
    }
    for (parent, alias, title, summary) in [
        ("intro", "problem", "Problem", "Fragmented knowledge work"),
        ("intro", "audience", "Audience", "People building maps"),
        ("method", "sources", "Sources", "Notes, PDFs, web clips"),
        ("method", "criteria", "Criteria", "Trust and recency"),
        (
            "findings",
            "finding-a",
            "Finding A",
            "Graphs need semantics",
        ),
        (
            "findings",
            "finding-b",
            "Finding B",
            "Adapters own rendering",
        ),
        ("findings", "finding-c", "Finding C", "Layouts are presets"),
        ("next", "ship", "Ship", "Polish runnable demos"),
        ("next", "measure", "Measure", "Benchmark large graphs"),
    ] {
        builder.node(
            alias,
            "demo.section",
            title,
            summary,
            CanvasPoint::default(),
        )?;
        builder.connect(parent, alias)?;
    }
    builder.apply_default_layout(SampleGraphKind::Tree.default_layout());
    Ok(())
}

fn populate_org_chart(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    for (alias, kind, title, summary, pos) in [
        (
            "ceo",
            "demo.person",
            "Avery Chen",
            "CEO · strategy and capital",
            CanvasPoint::default(),
        ),
        (
            "product",
            "demo.department",
            "Product",
            "Roadmap, research, UX",
            CanvasPoint::default(),
        ),
        (
            "engineering",
            "demo.department",
            "Engineering",
            "Runtime, adapters, infra",
            CanvasPoint::default(),
        ),
        (
            "gtm",
            "demo.department",
            "Go to market",
            "Sales, success, community",
            CanvasPoint::default(),
        ),
        (
            "pm",
            "demo.person",
            "Mina Rao",
            "Head of Product",
            CanvasPoint::default(),
        ),
        (
            "design",
            "demo.person",
            "Noah Park",
            "Design systems",
            CanvasPoint::default(),
        ),
        (
            "platform",
            "demo.person",
            "Iris Lin",
            "Platform lead",
            CanvasPoint::default(),
        ),
        (
            "adapter",
            "demo.person",
            "Sam Patel",
            "Adapter lead",
            CanvasPoint::default(),
        ),
        (
            "success",
            "demo.person",
            "Leah Gomez",
            "Customer success",
            CanvasPoint::default(),
        ),
    ] {
        builder.node(alias, kind, title, summary, pos)?;
    }

    for (from, to) in [
        ("ceo", "product"),
        ("ceo", "engineering"),
        ("ceo", "gtm"),
        ("product", "pm"),
        ("product", "design"),
        ("engineering", "platform"),
        ("engineering", "adapter"),
        ("gtm", "success"),
    ] {
        builder.connect(from, to)?;
    }
    builder.apply_default_layout(SampleGraphKind::OrgChart.default_layout());
    Ok(())
}

fn populate_knowledge_board(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    for (alias, kind, title, summary, pos) in [
        (
            "paper",
            "demo.source",
            "Paper excerpt",
            "Evidence card imported from PDF",
            CanvasPoint {
                x: -520.0,
                y: -160.0,
            },
        ),
        (
            "clip",
            "demo.source",
            "Web clip",
            "Counterexample from a case study",
            CanvasPoint { x: -520.0, y: 80.0 },
        ),
        (
            "claim",
            "demo.topic",
            "Main claim",
            "Graph apps need semantic nodes",
            CanvasPoint {
                x: -120.0,
                y: -40.0,
            },
        ),
        (
            "question",
            "demo.decision",
            "Open question",
            "What should stay headless?",
            CanvasPoint {
                x: 260.0,
                y: -160.0,
            },
        ),
        (
            "action",
            "demo.task",
            "Action item",
            "Build adapter examples",
            CanvasPoint { x: 280.0, y: 80.0 },
        ),
        (
            "output",
            "demo.output",
            "Export",
            "Readable board snapshot",
            CanvasPoint { x: 660.0, y: -40.0 },
        ),
    ] {
        builder.node(alias, kind, title, summary, pos)?;
    }
    builder.connect("paper", "claim")?;
    builder.connect("clip", "claim")?;
    builder.connect("claim", "question")?;
    builder.connect("claim", "action")?;
    builder.connect("question", "output")?;
    builder.connect("action", "output")?;
    builder.apply_default_layout(SampleGraphKind::KnowledgeBoard.default_layout());
    Ok(())
}

fn populate_erd(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    for (alias, title, fields, pos) in [
        (
            "customers",
            "customers",
            ["id", "email", "plan_id"].as_slice(),
            CanvasPoint {
                x: -260.0,
                y: -100.0,
            },
        ),
        (
            "orders",
            "orders",
            ["id", "customer_id", "total"].as_slice(),
            CanvasPoint {
                x: -60.0,
                y: -120.0,
            },
        ),
        (
            "order_items",
            "order_items",
            ["id", "order_id", "sku_id", "qty"].as_slice(),
            CanvasPoint {
                x: 140.0,
                y: -120.0,
            },
        ),
        (
            "skus",
            "skus",
            ["id", "title", "price"].as_slice(),
            CanvasPoint {
                x: 340.0,
                y: -120.0,
            },
        ),
        (
            "plans",
            "plans",
            ["id", "name", "limits"].as_slice(),
            CanvasPoint {
                x: -260.0,
                y: 130.0,
            },
        ),
    ] {
        builder.table_node(alias, title, fields, pos)?;
    }

    builder.connect_ports("customers", "pk", "orders", "fk")?;
    builder.connect_ports("orders", "pk", "order_items", "fk")?;
    builder.connect_ports("skus", "pk", "order_items", "fk")?;
    builder.connect_ports("plans", "pk", "customers", "fk")?;
    builder.apply_default_layout(SampleGraphKind::Erd.default_layout());
    Ok(())
}

fn populate_shader_graph(builder: &mut SampleGraphBuilder) -> Result<(), SampleGraphError> {
    builder.shader_node(
        "texture",
        "demo.shader.texture_sample",
        json!({
            "title": "Texture Sample",
            "summary": "Sample base color from UV",
            "ports": {
                "inputs": ["vec2 uv"],
                "outputs": ["vec4 color"]
            },
            "preview": {
                "texture": "checker"
            }
        }),
        CanvasPoint {
            x: -320.0,
            y: -80.0,
        },
    )?;
    builder.shader_node(
        "tint",
        "demo.shader.texture_sample",
        json!({
            "title": "Tint Ramp",
            "summary": "Procedural color ramp",
            "ports": {
                "inputs": ["vec2 uv"],
                "outputs": ["vec4 color"]
            },
            "preview": {
                "texture": "warm ramp"
            }
        }),
        CanvasPoint {
            x: -320.0,
            y: 120.0,
        },
    )?;
    builder.shader_node(
        "mix",
        "demo.shader.mix",
        json!({
            "title": "Mix",
            "summary": "Blend albedo with tint",
            "ports": {
                "inputs": ["vec4 albedo", "vec4 tint", "float factor"],
                "outputs": ["vec4 result"]
            },
            "config": {
                "factor": {
                    "type": "float",
                    "default": 0.5
                }
            },
            "preview": {
                "result": "gradient"
            },
            "dynamic_inputs": [
                { "id": "a", "name": "Albedo", "ty": "vec4", "port": "a" },
                { "id": "b", "name": "Tint", "ty": "vec4", "port": "b" },
                { "id": "factor", "name": "Factor", "ty": "float", "port": "factor" }
            ]
        }),
        CanvasPoint { x: 40.0, y: 20.0 },
    )?;

    builder.connect_ports("texture", "color", "mix", "a")?;
    builder.connect_ports("tint", "color", "mix", "b")?;
    builder.apply_default_layout(SampleGraphKind::ShaderGraph.default_layout());
    Ok(())
}

struct SampleGraphBuilder {
    store: NodeGraphStore,
    registry: NodeRegistry,
    aliases: BTreeMap<String, NodeId>,
}

impl SampleGraphBuilder {
    fn new(registry: NodeRegistry) -> Self {
        Self {
            store: NodeGraphStore::new(
                Graph::new(GraphId::new()),
                NodeGraphViewState::default(),
                NodeGraphEditorConfig::default().with_spatial_index_enabled(true),
            ),
            registry,
            aliases: BTreeMap::new(),
        }
    }

    fn node(
        &mut self,
        alias: &str,
        kind: &str,
        title: &str,
        summary: &str,
        pos: CanvasPoint,
    ) -> Result<NodeId, SampleGraphError> {
        let outcome = self
            .store
            .apply_create_node_from_schema(
                &self.registry,
                CreateNodeRequest::new(NodeKindKey::from(kind), pos),
            )
            .map_err(|err| SampleGraphError::Create(err.to_string()))?;
        let node = outcome.node_id();
        self.set_node_payload(node, title, summary)
            .map_err(SampleGraphError::Create)?;
        self.aliases.insert(alias.to_owned(), node);
        Ok(node)
    }

    fn table_node(
        &mut self,
        alias: &str,
        title: &str,
        fields: &[&str],
        pos: CanvasPoint,
    ) -> Result<NodeId, SampleGraphError> {
        let summary = fields.join(" · ");
        let node = self.node(alias, "demo.table", title, &summary, pos)?;
        self.set_table_fields(node, fields)
            .map_err(SampleGraphError::Create)?;
        Ok(node)
    }

    fn shader_node(
        &mut self,
        alias: &str,
        kind: &str,
        data: serde_json::Value,
        pos: CanvasPoint,
    ) -> Result<NodeId, SampleGraphError> {
        let outcome = self
            .store
            .apply_create_node_from_schema(
                &self.registry,
                CreateNodeRequest::new(NodeKindKey::from(kind), pos),
            )
            .map_err(|err| SampleGraphError::Create(err.to_string()))?;
        let node = outcome.node_id();
        self.set_node_data(node, data)
            .map_err(SampleGraphError::Create)?;
        self.aliases.insert(alias.to_owned(), node);
        Ok(node)
    }

    fn connect(&mut self, from_alias: &str, to_alias: &str) -> Result<(), SampleGraphError> {
        self.connect_by(from_alias, None, to_alias, None)
    }

    fn connect_ports(
        &mut self,
        from_alias: &str,
        from_port_key: &str,
        to_alias: &str,
        to_port_key: &str,
    ) -> Result<(), SampleGraphError> {
        self.connect_by(from_alias, Some(from_port_key), to_alias, Some(to_port_key))
    }

    fn connect_by(
        &mut self,
        from_alias: &str,
        from_port_key: Option<&str>,
        to_alias: &str,
        to_port_key: Option<&str>,
    ) -> Result<(), SampleGraphError> {
        let from = self.node_id(from_alias)?;
        let to = self.node_id(to_alias)?;
        let source = self.port(from, from_alias, PortDirection::Out, from_port_key)?;
        let target = self.port(to, to_alias, PortDirection::In, to_port_key)?;
        let mode = self.store.resolved_interaction_state().connection_mode;
        let outcome = self
            .store
            .apply_connect_edge(ConnectEdgeRequest::new(source, target, mode))
            .map_err(|err| {
                SampleGraphError::Connect(format!(
                    "{from_alias}({from_port_key:?}) -> {to_alias}({to_port_key:?}): {err}"
                ))
            })?;
        if let Some(edge) = outcome.as_ref().and_then(edge_from_outcome) {
            self.decorate_edge(edge, from_alias, to_alias)?;
        }
        Ok(())
    }

    fn apply_default_layout(&mut self, choice: LayoutPresetChoice) {
        let request = choice.builder().all().build();
        let _ = self.store.apply_layout(
            &request,
            jellyflow::layout::builtin_layout_engine_registry(),
        );
    }

    fn fit_view(&mut self) {
        let nodes = self
            .store
            .graph()
            .nodes()
            .values()
            .filter_map(|node| {
                let size = node.size?;
                Some(jellyflow::runtime::runtime::fit_view::FitViewNodeInfo {
                    pos: node.pos,
                    origin: node.origin,
                    size_px: (size.width, size.height),
                })
            })
            .collect::<Vec<_>>();
        let Some((pan, zoom)) = jellyflow::runtime::runtime::fit_view::compute_fit_view_target(
            &nodes,
            jellyflow::runtime::runtime::fit_view::FitViewComputeOptions {
                viewport_width_px: 1100.0,
                viewport_height_px: 700.0,
                node_origin: (0.0, 0.0),
                padding: 0.14,
                margin_px_fallback: 56.0,
                min_zoom: 0.2,
                max_zoom: 2.5,
            },
        ) else {
            return;
        };
        self.store.set_viewport(pan, zoom);
    }

    fn node_id(&self, alias: &str) -> Result<NodeId, SampleGraphError> {
        self.aliases
            .get(alias)
            .copied()
            .ok_or_else(|| SampleGraphError::MissingNode(alias.to_owned()))
    }

    fn port(
        &self,
        node: NodeId,
        alias: &str,
        direction: PortDirection,
        key: Option<&str>,
    ) -> Result<PortId, SampleGraphError> {
        self.store
            .graph()
            .nodes()
            .get(&node)
            .and_then(|record| {
                record.ports.iter().copied().find(|port| {
                    self.store.graph().ports().get(port).is_some_and(|record| {
                        record.dir == direction
                            && key.is_none_or(|expected| record.key.0 == expected)
                    })
                })
            })
            .ok_or_else(|| SampleGraphError::MissingPort {
                node: alias.to_owned(),
                direction,
            })
    }

    fn set_node_payload(
        &mut self,
        node: NodeId,
        title: &str,
        summary: &str,
    ) -> Result<DispatchOutcome, String> {
        let from = self
            .store
            .graph()
            .nodes()
            .get(&node)
            .map(|node| node.data.clone())
            .ok_or_else(|| format!("missing node `{node:?}`"))?;
        let mut to = from.clone();
        if !to.is_object() {
            to = json!({});
        }
        if let Some(object) = to.as_object_mut() {
            object.insert("title".to_owned(), json!(title));
            object.insert("summary".to_owned(), json!(summary));
        }
        self.store
            .dispatch_transaction(
                &jellyflow::core::GraphTransaction::from_ops([
                    jellyflow::core::GraphOp::SetNodeData { id: node, from, to },
                ])
                .with_label("Set sample node data"),
            )
            .map_err(|err| err.to_string())
    }

    fn set_node_data(
        &mut self,
        node: NodeId,
        to: serde_json::Value,
    ) -> Result<DispatchOutcome, String> {
        let from = self
            .store
            .graph()
            .nodes()
            .get(&node)
            .map(|node| node.data.clone())
            .ok_or_else(|| format!("missing node `{node:?}`"))?;
        self.store
            .dispatch_transaction(
                &jellyflow::core::GraphTransaction::from_ops([
                    jellyflow::core::GraphOp::SetNodeData { id: node, from, to },
                ])
                .with_label("Set sample node data"),
            )
            .map_err(|err| err.to_string())
    }

    fn set_table_fields(
        &mut self,
        node: NodeId,
        fields: &[&str],
    ) -> Result<DispatchOutcome, String> {
        let from = self
            .store
            .graph()
            .nodes()
            .get(&node)
            .map(|node| node.data.clone())
            .ok_or_else(|| format!("missing node `{node:?}`"))?;
        let mut to = from.clone();
        if !to.is_object() {
            to = json!({});
        }
        let (field_order, field_map) = table_field_payload(fields);
        if let Some(object) = to.as_object_mut() {
            object.insert("field_order".to_owned(), json!(field_order));
            object.insert("fields".to_owned(), json!(field_map));
        }
        self.store
            .dispatch_transaction(
                &jellyflow::core::GraphTransaction::from_ops([
                    jellyflow::core::GraphOp::SetNodeData { id: node, from, to },
                ])
                .with_label("Set table sample fields"),
            )
            .map_err(|err| err.to_string())
    }

    fn decorate_edge(
        &mut self,
        edge: EdgeId,
        from_alias: &str,
        to_alias: &str,
    ) -> Result<(), SampleGraphError> {
        let label = edge_label_for_aliases(from_alias, to_alias);
        let data = json!({ "label": label, "from": from_alias, "to": to_alias });
        let view = EdgeViewDescriptor::new()
            .with_renderer_key("sample-edge")
            .with_label(label)
            .with_label_anchor(EdgeLabelAnchor::Center)
            .with_target_marker_key("arrow")
            .with_style_token("default")
            .with_route_kind(EdgeRouteKind::Orthogonal)
            .with_hit_target_width(24.0);
        self.store
            .dispatch_transaction(
                &GraphTransaction::from_ops([
                    GraphOp::SetEdgeData {
                        id: edge,
                        from: serde_json::Value::Null,
                        to: data,
                    },
                    GraphOp::SetEdgeView {
                        id: edge,
                        from: EdgeViewDescriptor::default(),
                        to: view,
                    },
                ])
                .with_label("Set sample edge metadata"),
            )
            .map(|_| ())
            .map_err(|err| SampleGraphError::Connect(err.to_string()))
    }
}

fn table_field_payload(fields: &[&str]) -> (Vec<String>, BTreeMap<String, String>) {
    let mut order = Vec::new();
    let mut data = BTreeMap::new();
    let mut has_foreign_key = false;
    for field in fields {
        let key = if *field == "id" {
            "primary_key".to_owned()
        } else if field.ends_with("_id") && !has_foreign_key {
            has_foreign_key = true;
            "foreign_key".to_owned()
        } else {
            (*field).to_owned()
        };
        order.push(key.clone());
        data.insert(key, (*field).to_owned());
    }
    (order, data)
}

fn edge_from_outcome(outcome: &DispatchOutcome) -> Option<EdgeId> {
    outcome.committed().ops().iter().find_map(|op| match op {
        GraphOp::AddEdge { id, .. } => Some(*id),
        _ => None,
    })
}

fn edge_label_for_aliases(from_alias: &str, to_alias: &str) -> &'static str {
    match (from_alias, to_alias) {
        ("decide", "draft") => "yes",
        ("decide", "review") => "no",
        ("trigger", "normalize") => "event",
        ("normalize", "classify") => "variables",
        ("classify", "condition") => "classification",
        ("condition", "notify") => "yes",
        ("condition", "error") => "error",
        ("notify", "output") => "success",
        ("error", "output") => "recovered",
        ("customers", "orders") => "1:N",
        ("orders", "order_items") => "1:N",
        ("skus", "order_items") => "1:N",
        ("plans", "customers") => "1:N",
        ("ceo", "product") | ("ceo", "engineering") | ("ceo", "gtm") => "reports",
        ("product", "pm")
        | ("product", "design")
        | ("engineering", "platform")
        | ("engineering", "adapter")
        | ("gtm", "success") => "member",
        ("question", "output") => "answer",
        ("action", "output") => "deliver",
        _ => "flow",
    }
}

fn sample_node_registry() -> NodeRegistry {
    let mut registry = NodeKitRegistry::builtin().node_registry();
    registry.register(
        NodeSchema::builder("demo.section", "Section")
            .category(["Tree"])
            .keywords(["outline", "heading", "chapter"])
            .renderer_key("section-card")
            .default_size(CanvasSize {
                width: 190.0,
                height: 78.0,
            })
            .port(input_port("parent"))
            .port(output_port("child"))
            .default_data(json!({ "title": "Section", "summary": "Outline item" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.department", "Department")
            .category(["Org chart"])
            .keywords(["team", "department", "hierarchy"])
            .renderer_key("section-card")
            .default_size(CanvasSize {
                width: 206.0,
                height: 84.0,
            })
            .port(input_port("manager"))
            .port(output_port("reports"))
            .default_data(json!({ "title": "Department", "summary": "Team branch" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.person", "Person")
            .category(["Org chart"])
            .keywords(["employee", "role", "reports"])
            .renderer_key("idea-card")
            .default_size(CanvasSize {
                width: 196.0,
                height: 78.0,
            })
            .port(input_port("manager"))
            .port(output_port("reports"))
            .default_data(json!({ "title": "Person", "summary": "Role and ownership" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.source", "Source")
            .category(["Knowledge"])
            .keywords(["paper", "quote", "annotation", "marginnote"])
            .renderer_key("source-card")
            .default_size(CanvasSize {
                width: 210.0,
                height: 92.0,
            })
            .port(output_port("out"))
            .default_data(json!({ "title": "Source", "summary": "Evidence card" }))
            .build(),
    );
    registry
}

fn input_port(key: &str) -> PortDecl {
    PortDecl::new(key, PortDirection::In, PortKind::Data, PortCapacity::Multi)
        .with_label(key)
        .on_left()
}

fn output_port(key: &str) -> PortDecl {
    PortDecl::new(key, PortDirection::Out, PortKind::Data, PortCapacity::Multi)
        .with_label(key)
        .with_view(PortViewDescriptor::right())
}
