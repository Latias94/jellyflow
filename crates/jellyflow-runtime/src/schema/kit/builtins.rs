use serde_json::json;

use super::{
    NodeKitFixture, NodeKitFixtureEdge, NodeKitFixtureNode, NodeKitLayoutHints, NodeKitManifest,
    NodeKitRegistry,
};
use crate::schema::{
    NodeChromeDescriptor, NodeChromePlacement, NodeSchema, NodeSurfaceSlotDescriptor, PortDecl,
    PortViewDescriptor,
};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, EdgeKind, EdgeLabelAnchor, EdgeViewDescriptor, PortCapacity,
    PortDirection, PortKind,
};
use jellyflow_core::types::TypeDesc;

pub fn builtin_node_kits() -> NodeKitRegistry {
    let mut registry = NodeKitRegistry::new();
    registry.register(workflow_automation_manifest());
    registry.register(shader_blueprint_manifest());
    registry.register(erd_table_manifest());
    registry.register(mind_map_knowledge_canvas_manifest());
    registry
}

pub fn workflow_automation_manifest() -> NodeKitManifest {
    NodeKitManifest::new("workflow.automation", "Workflow automation")
        .with_supported_adapter("egui")
        .with_supported_adapter("proof")
        .with_supported_adapter("gpui")
        .with_capability("workflow")
        .with_capability("automation")
        .with_layout_hints(
            NodeKitLayoutHints::default()
                .with_zoom_range(0.72, 0.9)
                .with_field_spacing(10.0)
                .with_action_spacing(8.0)
                .with_measurement_note(
                    "Compact action rows and badge blocks should collapse at low zoom.",
                ),
        )
        .recipe(workflow_start_schema())
        .recipe(workflow_task_schema())
        .recipe(workflow_trigger_schema())
        .recipe(workflow_tool_schema())
        .recipe(workflow_llm_schema())
        .recipe(workflow_decision_schema())
        .recipe(workflow_switch_schema())
        .recipe(workflow_output_schema())
        .recipe(workflow_return_schema())
        .recipe(workflow_error_schema())
        .fixture(workflow_fixture())
}

pub fn erd_table_manifest() -> NodeKitManifest {
    NodeKitManifest::new("erd.table", "ERD table")
        .with_supported_adapter("egui")
        .with_supported_adapter("proof")
        .with_supported_adapter("gpui")
        .with_capability("erd")
        .with_capability("table")
        .with_layout_hints(
            NodeKitLayoutHints::default()
                .with_zoom_range(0.72, 0.88)
                .with_field_spacing(6.0)
                .with_measurement_note("Field rows should stay aligned to anchored ports."),
        )
        .recipe(erd_table_schema())
        .fixture(erd_fixture())
}

pub fn shader_blueprint_manifest() -> NodeKitManifest {
    NodeKitManifest::new("shader.blueprint", "Shader and blueprint")
        .with_supported_adapter("egui")
        .with_supported_adapter("proof")
        .with_supported_adapter("gpui")
        .with_capability("shader")
        .with_capability("blueprint")
        .with_layout_hints(
            NodeKitLayoutHints::default()
                .with_zoom_range(0.74, 0.92)
                .with_field_spacing(6.0)
                .with_measurement_note(
                    "Typed port rails and preview regions should preserve handle alignment.",
                ),
        )
        .recipe(shader_texture_sample_schema())
        .recipe(shader_mix_schema())
        .fixture(shader_fixture())
}

pub fn mind_map_knowledge_canvas_manifest() -> NodeKitManifest {
    NodeKitManifest::new("mind-map.knowledge-canvas", "Mind map and knowledge canvas")
        .with_supported_adapter("egui")
        .with_supported_adapter("proof")
        .with_supported_adapter("gpui")
        .with_capability("mind-map")
        .with_capability("knowledge-canvas")
        .with_layout_hints(
            NodeKitLayoutHints::default()
                .with_zoom_range(0.68, 0.88)
                .with_measurement_note(
                    "Topic, summary, and source nodes should degrade to shells with stable anchors.",
                ),
        )
        .recipe(mind_topic_schema())
        .recipe(mind_idea_schema())
        .recipe(source_card_schema())
        .fixture(mind_map_fixture())
}

fn workflow_trigger_schema() -> NodeSchema {
    NodeSchema::builder("demo.trigger", "Trigger")
        .category(["Automation", "Workflow"])
        .keywords(["webhook", "schedule", "event"])
        .renderer_key("data-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 96.0,
        })
        .port(exec_output("event").on_right().with_view_group("exec"))
        .port(
            data_output("payload")
                .on_bottom()
                .with_view_group("data")
                .with_view_order(1),
        )
        .default_data(json!({ "title": "Trigger", "summary": "Starts an automation" }))
        .build()
}

fn workflow_start_schema() -> NodeSchema {
    NodeSchema::builder("demo.start", "Start")
        .category(["Workflow"])
        .renderer_key("data-card")
        .default_size(CanvasSize {
            width: 176.0,
            height: 80.0,
        })
        .port(output_port("out").on_right())
        .default_data(json!({ "title": "Start", "summary": "Entry point" }))
        .build()
}

fn workflow_task_schema() -> NodeSchema {
    NodeSchema::builder("demo.task", "Task")
        .category(["Workflow"])
        .renderer_key("task-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 96.0,
        })
        .port(PortDecl::data_input("in").with_label("in").on_left())
        .port(PortDecl::data_output("out").with_label("out").on_right())
        .default_data(json!({ "title": "Task", "summary": "Run a unit of work" }))
        .build()
}

fn workflow_tool_schema() -> NodeSchema {
    NodeSchema::builder("demo.tool", "Tool")
        .category(["Automation", "Workflow"])
        .keywords(["api", "function", "action"])
        .renderer_key("task-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 104.0,
        })
        .port(exec_input("in").on_left().with_view_group("exec"))
        .port(exec_output("out").on_right().with_view_group("exec"))
        .port(
            data_input("args")
                .on_top()
                .with_view_group("data")
                .with_view_order(0),
        )
        .port(
            data_output("result")
                .on_bottom()
                .with_view_group("data")
                .with_view_order(0),
        )
        .default_data(json!({ "title": "Tool", "summary": "Runs an external action" }))
        .build()
}

fn workflow_llm_schema() -> NodeSchema {
    NodeSchema::builder("demo.llm", "LLM")
        .category(["Automation", "AI"])
        .keywords(["prompt", "model", "dify"])
        .renderer_key("decision-card")
        .default_size(CanvasSize {
            width: 228.0,
            height: 196.0,
        })
        .port(exec_input("in").on_left().with_view_group("exec"))
        .port(exec_output("out").on_right().with_view_group("exec"))
        .port(
            data_input("prompt")
                .on_top()
                .with_view_anchor("field.prompt")
                .with_view_group("parameters")
                .with_view_order(0),
        )
        .port(
            data_output("completion")
                .on_bottom()
                .with_view_anchor("field.completion")
                .with_view_group("outputs")
                .with_view_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.prompt")
                .with_label("Prompt")
                .with_slot("prompt")
                .with_anchor("field.prompt")
                .with_lane("parameters")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.completion")
                .with_label("Completion")
                .with_slot("completion")
                .with_anchor("field.completion")
                .with_lane("outputs")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::badge("badge.model")
                .with_label("Model")
                .with_slot("meta.model")
                .with_anchor("meta.model")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::metric_badge("metric.latency")
                .with_label("Latency")
                .with_slot("metrics.latency")
                .with_anchor("metric.latency")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::config_group("config.model")
                .with_label("Config")
                .with_slot("config.model")
                .with_anchor("config.model")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::nested_region("nested.policy")
                .with_label("Policy")
                .with_slot("nested.policy")
                .with_anchor("nested.policy")
                .with_order(2),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::status_banner("status.validation")
                .with_label("Status")
                .with_slot("status.validation")
                .with_anchor("status.validation")
                .with_order(3),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::action_row("actions.primary")
                .with_label("Actions")
                .with_slot("actions.primary")
                .with_anchor("actions.primary")
                .with_order(4),
        )
        .chrome(NodeChromeDescriptor::resizer("resize.corner").with_order(0))
        .chrome(
            NodeChromeDescriptor::toolbar("toolbar.primary", NodeChromePlacement::TopRight)
                .with_label("Node tools")
                .with_renderer_key("node-toolbar")
                .with_icon_key("settings")
                .with_order(10),
        )
        .chrome(
            NodeChromeDescriptor::status_strip("status.run", NodeChromePlacement::InsideFooter)
                .with_label("Ready")
                .with_renderer_key("run-status")
                .with_icon_key("activity")
                .with_order(20),
        )
        .chrome(
            NodeChromeDescriptor::run_action_strip("actions.run", NodeChromePlacement::Bottom)
                .with_label("Run")
                .with_renderer_key("run-actions")
                .with_icon_key("play")
                .with_order(30),
        )
        .default_data(json!({
            "title": "LLM",
            "summary": "Prompt, model, tools, and variables",
            "meta": {
                "model": "gpt-4.1-mini"
            },
            "metrics": {
                "latency": "420ms"
            },
            "config": {
                "model": {
                    "temperature": 0.2,
                    "tools": "retrieval"
                }
            },
            "status": {
                "validation": "Ready"
            },
            "nested": {
                "policy": {
                    "guardrails": "Block PII",
                    "response": "Return structured route"
                }
            },
            "actions": {
                "primary": ["Test prompt", "Open trace", "Copy config"]
            },
            "fields": {
                "prompt": "Customer intake + policy",
                "completion": "Priority and route"
            }
        }))
        .build()
}

fn workflow_decision_schema() -> NodeSchema {
    NodeSchema::builder("demo.decision", "Decision")
        .category(["Workflow"])
        .renderer_key("decision-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 104.0,
        })
        .port(input_port("in"))
        .port(output_port("yes").on_top().with_view_order(0))
        .port(output_port("no").on_bottom().with_view_order(1))
        .surface_slot(
            NodeSurfaceSlotDescriptor::badge("badge.branch")
                .with_label("Branch")
                .with_slot("meta.branch")
                .with_anchor("meta.branch")
                .with_order(0),
        )
        .default_data(json!({ "title": "Decision", "summary": "Branch the flow" }))
        .build()
}

fn workflow_switch_schema() -> NodeSchema {
    NodeSchema::builder("demo.switch", "Switch")
        .category(["Automation", "Workflow"])
        .keywords(["branch", "condition", "router"])
        .renderer_key("decision-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 104.0,
        })
        .port(exec_input("in").on_left().with_view_group("exec"))
        .port(exec_output("yes").on_top().with_view_order(0))
        .port(exec_output("no").on_bottom().with_view_order(1))
        .default_data(json!({ "title": "Switch", "summary": "Branch execution" }))
        .build()
}

fn workflow_output_schema() -> NodeSchema {
    NodeSchema::builder("demo.output", "Output")
        .alias("demo.workflow_output")
        .category(["Workflow"])
        .renderer_key("output-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 96.0,
        })
        .port(PortDecl::data_input("in").with_label("in").on_left())
        .default_data(json!({ "title": "Output", "summary": "Publish the result" }))
        .build()
}

fn workflow_return_schema() -> NodeSchema {
    NodeSchema::builder("demo.workflow_output", "Workflow output")
        .category(["Automation", "Workflow"])
        .keywords(["return", "response", "result"])
        .renderer_key("output-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 96.0,
        })
        .port(exec_input("in").on_left().with_view_group("exec"))
        .port(
            data_input("result")
                .on_top()
                .with_view_group("data")
                .with_view_order(0),
        )
        .default_data(json!({ "title": "Workflow output", "summary": "Returns data to caller" }))
        .build()
}

fn workflow_error_schema() -> NodeSchema {
    NodeSchema::builder("demo.error", "Error")
        .category(["Automation", "Workflow"])
        .keywords(["failure", "retry", "fallback"])
        .renderer_key("output-card")
        .default_size(CanvasSize {
            width: 208.0,
            height: 96.0,
        })
        .port(exec_input("error").on_left().with_view_group("exec"))
        .port(exec_output("out").on_right().with_view_group("exec"))
        .default_data(json!({ "title": "Error path", "summary": "Retry or recover" }))
        .build()
}

fn erd_table_schema() -> NodeSchema {
    NodeSchema::builder("demo.table", "Table")
        .category(["ERD"])
        .keywords(["database", "schema", "relation"])
        .renderer_key("table-card")
        .default_size(CanvasSize {
            width: 226.0,
            height: 186.0,
        })
        .port(
            data_input("fk")
                .with_label("foreign key")
                .on_left()
                .with_view_anchor("field.foreign_key")
                .with_view_group("fields")
                .with_view_order(0),
        )
        .port(
            data_output("pk")
                .with_label("primary key")
                .on_right()
                .with_view_anchor("field.primary_key")
                .with_view_group("fields")
                .with_view_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.primary_key")
                .with_label("Primary key")
                .with_slot("primary_key")
                .with_anchor("field.primary_key")
                .with_lane("fields")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.field")
                .with_label("Field")
                .with_slot("field")
                .with_anchor("field.field")
                .with_lane("fields")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.foreign_key")
                .with_label("Foreign key")
                .with_slot("foreign_key")
                .with_anchor("field.foreign_key")
                .with_lane("fields")
                .with_order(2),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::badge("badge.cardinality")
                .with_label("1:N")
                .with_slot("meta.cardinality")
                .with_anchor("meta.cardinality")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::metric_badge("metric.rows")
                .with_label("Rows")
                .with_slot("metrics.rows")
                .with_anchor("metric.rows")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::action_row("actions.table")
                .with_label("Actions")
                .with_slot("actions.table")
                .with_anchor("actions.table")
                .with_order(2),
        )
        .default_data(json!({
            "title": "Table",
            "summary": "id · field · field",
            "meta": { "cardinality": "1:N" },
            "metrics": { "rows": "12k" },
            "actions": { "table": ["Add column", "Inspect relation"] },
            "field_order": ["primary_key", "field", "foreign_key"],
            "fields": {
                "primary_key": "id",
                "field": "field",
                "foreign_key": "field_id"
            }
        }))
        .build()
}

fn shader_texture_sample_schema() -> NodeSchema {
    NodeSchema::builder("demo.shader.texture_sample", "Texture Sample")
        .category(["Shader", "Blueprint"])
        .keywords(["shader", "texture", "unreal", "graph"])
        .renderer_key("shader-card")
        .default_size(CanvasSize {
            width: 224.0,
            height: 156.0,
        })
        .port(
            data_input("uv")
                .with_label("UV")
                .with_type(shader_vec(2))
                .on_left()
                .with_view_anchor("rail.inputs")
                .with_view_group("typed_ports")
                .with_view_order(0),
        )
        .port(
            data_output("color")
                .with_label("Color")
                .with_type(shader_vec(4))
                .on_right()
                .with_view_anchor("rail.outputs")
                .with_view_group("typed_ports")
                .with_view_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::port_rail("rail.inputs")
                .with_label("Inputs")
                .with_slot("ports.inputs")
                .with_anchor("rail.inputs")
                .with_lane("ports")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::preview("preview.texture")
                .with_label("Preview")
                .with_slot("preview.texture")
                .with_anchor("preview.texture")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::port_rail("rail.outputs")
                .with_label("Outputs")
                .with_slot("ports.outputs")
                .with_anchor("rail.outputs")
                .with_lane("ports")
                .with_order(2),
        )
        .default_data(json!({
            "title": "Texture Sample",
            "summary": "Samples albedo from UV",
            "ports": {
                "inputs": ["vec2 uv"],
                "outputs": ["vec4 color"]
            },
            "preview": {
                "texture": "checker"
            }
        }))
        .build()
}

fn shader_mix_schema() -> NodeSchema {
    NodeSchema::builder("demo.shader.mix", "Mix")
        .category(["Shader", "Blueprint"])
        .keywords(["shader", "mix", "lerp", "blueprint"])
        .renderer_key("shader-card")
        .default_size(CanvasSize {
            width: 224.0,
            height: 168.0,
        })
        .port(
            data_input("a")
                .with_label("A")
                .with_type(shader_vec(4))
                .on_left()
                .with_view_anchor("rail.inputs")
                .with_view_group("typed_ports")
                .with_view_order(0),
        )
        .port(
            data_input("b")
                .with_label("B")
                .with_type(shader_vec(4))
                .on_left()
                .with_view_anchor("rail.inputs")
                .with_view_group("typed_ports")
                .with_view_order(1),
        )
        .port(
            data_input("factor")
                .with_label("Factor")
                .with_type(TypeDesc::Float)
                .on_bottom()
                .with_view_anchor("config.factor")
                .with_view_group("config")
                .with_view_order(2),
        )
        .port(
            data_output("result")
                .with_label("Result")
                .with_type(shader_vec(4))
                .on_right()
                .with_view_anchor("rail.outputs")
                .with_view_group("typed_ports")
                .with_view_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::port_rail("rail.inputs")
                .with_label("Inputs")
                .with_slot("ports.inputs")
                .with_anchor("rail.inputs")
                .with_lane("ports")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::config_group("config.factor")
                .with_label("Factor")
                .with_slot("config.factor")
                .with_anchor("config.factor")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::preview("preview.result")
                .with_label("Preview")
                .with_slot("preview.result")
                .with_anchor("preview.result")
                .with_order(2),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::port_rail("rail.outputs")
                .with_label("Outputs")
                .with_slot("ports.outputs")
                .with_anchor("rail.outputs")
                .with_lane("ports")
                .with_order(3),
        )
        .default_data(json!({
            "title": "Mix",
            "summary": "Blend two color streams",
            "ports": {
                "inputs": ["vec4 a", "vec4 b"],
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
            }
        }))
        .build()
}

fn mind_topic_schema() -> NodeSchema {
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
        .surface_slot(
            NodeSurfaceSlotDescriptor::header("header.main")
                .with_label("Topic")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::body("body.summary")
                .with_label("Summary")
                .with_slot("summary")
                .with_anchor("body.summary")
                .with_order(1),
        )
        .default_data(json!({ "title": "Topic", "summary": "Central idea" }))
        .build()
}

fn mind_idea_schema() -> NodeSchema {
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
        .surface_slot(
            NodeSurfaceSlotDescriptor::header("header.main")
                .with_label("Idea")
                .with_order(0),
        )
        .default_data(json!({ "title": "Idea", "summary": "Branch note" }))
        .build()
}

fn source_card_schema() -> NodeSchema {
    NodeSchema::builder("demo.source", "Source")
        .category(["Knowledge"])
        .keywords(["paper", "quote", "annotation", "marginnote"])
        .renderer_key("source-card")
        .default_size(CanvasSize {
            width: 210.0,
            height: 92.0,
        })
        .port(output_port("out"))
        .surface_slot(
            NodeSurfaceSlotDescriptor::header("header.main")
                .with_label("Source")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::preview("preview.main")
                .with_label("Excerpt")
                .with_slot("preview")
                .with_anchor("preview.main")
                .with_order(1),
        )
        .default_data(json!({ "title": "Source", "summary": "Evidence card" }))
        .build()
}

fn workflow_fixture() -> NodeKitFixture {
    NodeKitFixture::new("workflow.review", "Workflow review")
        .with_description("A compact review loop with branch and output paths.")
        .node(
            NodeKitFixtureNode::new(
                "trigger",
                "demo.trigger",
                CanvasPoint {
                    x: -320.0,
                    y: -40.0,
                },
            )
            .with_data(json!({
                "title": "Trigger",
                "summary": "Starts an automation"
            })),
        )
        .node(
            NodeKitFixtureNode::new("tool", "demo.tool", CanvasPoint { x: -20.0, y: -80.0 })
                .with_data(json!({
                    "title": "Tool",
                    "summary": "Runs an external action"
                })),
        )
        .node(
            NodeKitFixtureNode::new(
                "decision",
                "demo.decision",
                CanvasPoint { x: 300.0, y: -80.0 },
            )
            .with_data(json!({
                "title": "Decision",
                "summary": "Branch the flow"
            })),
        )
        .node(
            NodeKitFixtureNode::new(
                "output",
                "demo.workflow_output",
                CanvasPoint { x: 620.0, y: -40.0 },
            )
            .with_data(json!({
                "title": "Workflow output",
                "summary": "Returns data to caller"
            })),
        )
        .edge(
            NodeKitFixtureEdge::new("trigger", "tool", EdgeKind::Exec)
                .with_from_port("event")
                .with_to_port("in")
                .with_data(json!({ "label": "event" }))
                .with_view(
                    EdgeViewDescriptor::new()
                        .with_label("event")
                        .with_label_anchor(EdgeLabelAnchor::Center)
                        .with_target_marker_key("arrow")
                        .with_style_token("workflow"),
                ),
        )
        .edge(
            NodeKitFixtureEdge::new("tool", "decision", EdgeKind::Exec)
                .with_from_port("out")
                .with_to_port("in")
                .with_data(json!({ "label": "flow" })),
        )
        .edge(
            NodeKitFixtureEdge::new("decision", "output", EdgeKind::Exec)
                .with_from_port("yes")
                .with_to_port("in")
                .with_data(json!({ "label": "yes" })),
        )
        .expect_counts(4, 3)
}

fn erd_fixture() -> NodeKitFixture {
    NodeKitFixture::new("erd.customer_orders", "Customer orders")
        .with_description("Three linked tables showing a classic 1:N relationship.")
        .node(
            NodeKitFixtureNode::new(
                "customers",
                "demo.table",
                CanvasPoint {
                    x: -240.0,
                    y: -80.0,
                },
            )
            .with_data(json!({
                "title": "customers",
                "summary": "id · email · plan_id",
                "field_order": ["primary_key", "field", "foreign_key"],
                "fields": {
                    "primary_key": "id",
                    "field": "email",
                    "foreign_key": "plan_id"
                },
                "meta": { "cardinality": "1:N" },
                "metrics": { "rows": "42k" },
                "actions": { "table": ["Add column", "Inspect relation"] }
            })),
        )
        .node(
            NodeKitFixtureNode::new("orders", "demo.table", CanvasPoint { x: 20.0, y: -100.0 })
                .with_data(json!({
                    "title": "orders",
                    "summary": "id · customer_id · total",
                    "field_order": ["primary_key", "field", "foreign_key"],
                    "fields": {
                        "primary_key": "id",
                        "field": "total",
                        "foreign_key": "customer_id"
                    },
                    "meta": { "cardinality": "1:N" },
                    "metrics": { "rows": "94k" },
                    "actions": { "table": ["Add column", "Inspect relation"] }
                })),
        )
        .node(
            NodeKitFixtureNode::new(
                "order_items",
                "demo.table",
                CanvasPoint { x: 300.0, y: -80.0 },
            )
            .with_data(json!({
                "title": "order_items",
                "summary": "id · order_id · sku_id · qty",
                "field_order": ["primary_key", "field", "foreign_key", "field"],
                "fields": {
                    "primary_key": "id",
                    "field": "qty",
                    "foreign_key": "order_id"
                },
                "meta": { "cardinality": "1:N" },
                "metrics": { "rows": "320k" },
                "actions": { "table": ["Add column", "Inspect relation"] }
            })),
        )
        .edge(
            NodeKitFixtureEdge::new("customers", "orders", EdgeKind::Data)
                .with_from_port("pk")
                .with_to_port("fk")
                .with_data(json!({ "label": "1:N" })),
        )
        .edge(
            NodeKitFixtureEdge::new("orders", "order_items", EdgeKind::Data)
                .with_from_port("pk")
                .with_to_port("fk")
                .with_data(json!({ "label": "1:N" })),
        )
        .expect_counts(3, 2)
}

fn shader_fixture() -> NodeKitFixture {
    NodeKitFixture::new("shader.material_mix", "Material mix")
        .with_description("A compact shader graph with typed rails and preview slots.")
        .node(
            NodeKitFixtureNode::new(
                "texture",
                "demo.shader.texture_sample",
                CanvasPoint {
                    x: -240.0,
                    y: -60.0,
                },
            )
            .with_data(json!({
                "title": "Albedo",
                "summary": "Texture sample",
                "ports": {
                    "inputs": ["vec2 uv"],
                    "outputs": ["vec4 color"]
                },
                "preview": {
                    "texture": "checker"
                }
            })),
        )
        .node(
            NodeKitFixtureNode::new("mix", "demo.shader.mix", CanvasPoint { x: 80.0, y: -60.0 })
                .with_data(json!({
                    "title": "Mix",
                    "summary": "Blend albedo with tint",
                    "ports": {
                        "inputs": ["vec4 albedo", "vec4 tint"],
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
                    }
                })),
        )
        .edge(
            NodeKitFixtureEdge::new("texture", "mix", EdgeKind::Data)
                .with_from_port("color")
                .with_to_port("a")
                .with_data(json!({ "label": "vec4" })),
        )
        .expect_counts(2, 1)
}

fn mind_map_fixture() -> NodeKitFixture {
    NodeKitFixture::new("mind-map.strategy", "Strategy map")
        .with_description("A radial map showing topic, ideas, and source cards.")
        .node(
            NodeKitFixtureNode::new("center", "demo.topic", CanvasPoint::default()).with_data(
                json!({
                    "title": "Product strategy",
                    "summary": "MindNode-style radial map"
                }),
            ),
        )
        .node(
            NodeKitFixtureNode::new("users", "demo.idea", CanvasPoint::default()).with_data(
                json!({
                    "title": "Users",
                    "summary": "Researchers, builders, editors"
                }),
            ),
        )
        .node(
            NodeKitFixtureNode::new("risks", "demo.idea", CanvasPoint::default()).with_data(
                json!({
                    "title": "Risks",
                    "summary": "Trust, scale, migration"
                }),
            ),
        )
        .node(
            NodeKitFixtureNode::new("sources", "demo.source", CanvasPoint::default()).with_data(
                json!({
                    "title": "Source",
                    "summary": "Evidence card"
                }),
            ),
        )
        .edge(
            NodeKitFixtureEdge::new("center", "users", EdgeKind::Data)
                .with_data(json!({ "label": "branch" })),
        )
        .edge(
            NodeKitFixtureEdge::new("center", "risks", EdgeKind::Data)
                .with_data(json!({ "label": "branch" })),
        )
        .edge(
            NodeKitFixtureEdge::new("sources", "center", EdgeKind::Data)
                .with_from_port("out")
                .with_to_port("in")
                .with_data(json!({ "label": "evidence" })),
        )
        .expect_counts(4, 3)
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

fn data_input(key: &str) -> PortDecl {
    PortDecl::new(key, PortDirection::In, PortKind::Data, PortCapacity::Multi).with_label(key)
}

fn data_output(key: &str) -> PortDecl {
    PortDecl::new(key, PortDirection::Out, PortKind::Data, PortCapacity::Multi).with_label(key)
}

fn shader_vec(width: u8) -> TypeDesc {
    TypeDesc::Opaque {
        key: format!("shader.vec{width}"),
        params: Vec::new(),
    }
}

fn exec_input(key: &str) -> PortDecl {
    PortDecl::new(key, PortDirection::In, PortKind::Exec, PortCapacity::Single).with_label(key)
}

fn exec_output(key: &str) -> PortDecl {
    PortDecl::new(key, PortDirection::Out, PortKind::Exec, PortCapacity::Multi).with_label(key)
}
