use std::collections::BTreeMap;

use jellyflow::core::{
    CanvasPoint, CanvasSize, Graph, GraphId, NodeId, NodeKindKey, PortCapacity, PortDirection,
    PortId, PortKind,
};
use jellyflow::runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow::runtime::runtime::connection::ConnectEdgeRequest;
use jellyflow::runtime::runtime::create_node::CreateNodeRequest;
use jellyflow::runtime::schema::{NodeRegistry, NodeSchema, PortDecl};
use jellyflow::runtime::{DispatchOutcome, NodeGraphStore};
use serde_json::json;
use thiserror::Error;

use crate::bridge::{DEFAULT_NODE_HEIGHT, DEFAULT_NODE_WIDTH};
use crate::state::LayoutPresetChoice;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SampleGraphKind {
    #[default]
    Workflow,
    MindMap,
    Tree,
    KnowledgeBoard,
}

impl SampleGraphKind {
    pub const ALL: [Self; 4] = [
        Self::Workflow,
        Self::MindMap,
        Self::Tree,
        Self::KnowledgeBoard,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Workflow => "Workflow",
            Self::MindMap => "Mind map",
            Self::Tree => "Tree",
            Self::KnowledgeBoard => "Knowledge board",
        }
    }

    pub fn default_layout(self) -> LayoutPresetChoice {
        match self {
            Self::Workflow => LayoutPresetChoice::Workflow,
            Self::MindMap => LayoutPresetChoice::MindMap,
            Self::Tree => LayoutPresetChoice::Tree,
            Self::KnowledgeBoard => LayoutPresetChoice::Freeform,
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
        SampleGraphKind::MindMap => populate_mind_map(&mut builder)?,
        SampleGraphKind::Tree => populate_tree(&mut builder)?,
        SampleGraphKind::KnowledgeBoard => populate_knowledge_board(&mut builder)?,
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

    fn connect(&mut self, from_alias: &str, to_alias: &str) -> Result<(), SampleGraphError> {
        let from = self.node_id(from_alias)?;
        let to = self.node_id(to_alias)?;
        let source = self.port(from, from_alias, PortDirection::Out)?;
        let target = self.port(to, to_alias, PortDirection::In)?;
        let mode = self.store.resolved_interaction_state().connection_mode;
        self.store
            .apply_connect_edge(ConnectEdgeRequest::new(source, target, mode))
            .map(|_| ())
            .map_err(|err| SampleGraphError::Connect(err.to_string()))
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
    ) -> Result<PortId, SampleGraphError> {
        self.store
            .graph()
            .nodes()
            .get(&node)
            .and_then(|record| {
                record.ports.iter().copied().find(|port| {
                    self.store
                        .graph()
                        .ports()
                        .get(port)
                        .is_some_and(|record| record.dir == direction)
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
        let to = json!({
            "title": title,
            "summary": summary,
        });
        self.store
            .dispatch_transaction(
                &jellyflow::core::GraphTransaction::from_ops([
                    jellyflow::core::GraphOp::SetNodeData { id: node, from, to },
                ])
                .with_label("Set sample node data"),
            )
            .map_err(|err| err.to_string())
    }
}

fn sample_node_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::new();
    registry.register(
        NodeSchema::builder("demo.start", "Start")
            .category(["Workflow"])
            .renderer_key("data-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(PortDecl::data_output("out").with_label("out"))
            .default_data(json!({ "title": "Start", "summary": "Entry point" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.task", "Task")
            .category(["Workflow"])
            .renderer_key("task-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(PortDecl::data_input("in").with_label("in"))
            .port(PortDecl::data_output("out").with_label("out"))
            .default_data(json!({ "title": "Task", "summary": "Run a unit of work" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.decision", "Decision")
            .category(["Workflow"])
            .renderer_key("decision-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(input_port("in"))
            .port(output_port("yes"))
            .port(output_port("no"))
            .default_data(json!({ "title": "Decision", "summary": "Branch the flow" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.output", "Output")
            .category(["Workflow"])
            .renderer_key("output-card")
            .default_size(CanvasSize {
                width: DEFAULT_NODE_WIDTH,
                height: DEFAULT_NODE_HEIGHT,
            })
            .port(PortDecl::data_input("in").with_label("in"))
            .default_data(json!({ "title": "Output", "summary": "Publish the result" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.topic", "Topic")
            .category(["Mind map"])
            .keywords(["mindnode", "margin-note", "knowledge"])
            .renderer_key("topic-card")
            .default_size(CanvasSize {
                width: 210.0,
                height: 96.0,
            })
            .port(input_port("in"))
            .port(output_port("out"))
            .default_data(json!({ "title": "Topic", "summary": "Central idea" }))
            .build(),
    );
    registry.register(
        NodeSchema::builder("demo.idea", "Idea")
            .category(["Mind map"])
            .keywords(["branch", "note", "thought"])
            .renderer_key("idea-card")
            .default_size(CanvasSize {
                width: 176.0,
                height: 76.0,
            })
            .port(input_port("in"))
            .port(output_port("out"))
            .default_data(json!({ "title": "Idea", "summary": "Branch note" }))
            .build(),
    );
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
    PortDecl::new(key, PortDirection::In, PortKind::Data, PortCapacity::Multi).with_label(key)
}

fn output_port(key: &str) -> PortDecl {
    PortDecl::new(key, PortDirection::Out, PortKind::Data, PortCapacity::Multi).with_label(key)
}
