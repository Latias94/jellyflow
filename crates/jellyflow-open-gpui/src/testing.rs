//! Test helpers and regression gates for GPUI adapter conformance.

use std::collections::BTreeSet;

use jellyflow::{
    core::{CanvasRect, CanvasSize, Node, NodeId, NodeKindKey, PortKey},
    runtime::{
        runtime::{
            conformance::{ConformanceCapabilityKind, ConformanceSupportLevel},
            measurement::NodeMeasurement,
        },
        schema::{NodeKindViewDescriptor, NodeKitKey, NodeKitRegistry, NodeRegistry},
    },
};

use crate::{
    OpenGpuiActionSurface, OpenGpuiAdapter, OpenGpuiControlPrimitive, OpenGpuiInspectorSurface,
    OpenGpuiMeasuredRegion, OpenGpuiMeasurementContext, OpenGpuiMeasurementCoverage,
    OpenGpuiMeasurementId, OpenGpuiMeasurementMode, OpenGpuiNodeSurfaceLayout,
    OpenGpuiRepeatablePortDiagnostic, OpenGpuiViewBounds, OpenGpuiViewPoint, OpenGpuiViewSize,
    layout_pass_measurement_from_regions, measured_surface_anchors, primitive_for_kind,
    project_actions_for_surface, project_node_measurement, project_slot_controls,
    projected_node_surface_graph_layout, repeatable_item_projection, repeatable_port_diagnostics,
};

pub fn assert_layout_pass_capability_requires_real_bounds(adapter: &OpenGpuiAdapter) {
    let matrix = adapter.capability_matrix();
    if matrix.satisfies(
        ConformanceCapabilityKind::LayoutPassMeasurement,
        ConformanceSupportLevel::Full,
    ) {
        assert_eq!(
            adapter.measurement_mode(),
            crate::OpenGpuiMeasurementMode::LayoutPass
        );
        assert!(
            adapter
                .measurement_coverage()
                .is_some_and(|coverage| coverage.is_full_layout_pass()),
            "full layout-pass support requires measured-element source coverage"
        );
    }
}

/// A builtin product shape that GPUI adapter regression gates must keep covering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiProductFixtureKind {
    DifyWorkflow,
    ShaderBlueprint,
    ErdTable,
    MindMap,
}

/// Adapter-level evidence collected for one builtin product fixture.
#[derive(Debug, Clone)]
pub struct OpenGpuiProductFixtureReport {
    pub kind: OpenGpuiProductFixtureKind,
    pub kit_key: String,
    pub fixture_key: String,
    pub node_count: usize,
    pub measured_nodes: usize,
    pub slot_count: usize,
    pub anchor_count: usize,
    pub repeatable_item_count: usize,
    pub missing_dynamic_ports: Vec<OpenGpuiRepeatablePortDiagnostic>,
    pub actions: BTreeSet<String>,
    pub inspectors: BTreeSet<String>,
    pub control_primitives: BTreeSet<&'static str>,
    pub measured_control_regions: usize,
    pub measurement_mode: OpenGpuiMeasurementMode,
    pub measurement_coverage: OpenGpuiMeasurementCoverage,
}

/// Build adapter-level regression evidence for one builtin product fixture.
pub fn product_fixture_report(
    kind: OpenGpuiProductFixtureKind,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let spec = ProductFixtureSpec::for_kind(kind);
    let kit_registry = NodeKitRegistry::builtin();
    let node_registry = kit_registry.node_registry();
    let graph = kit_registry
        .fixture_graph(&NodeKitKey::new(spec.kit_key), spec.fixture_key)
        .map_err(|error| error.to_string())?;
    let mut report = OpenGpuiProductFixtureReport {
        kind,
        kit_key: spec.kit_key.to_owned(),
        fixture_key: spec.fixture_key.to_owned(),
        node_count: graph.nodes().len(),
        measured_nodes: 0,
        slot_count: 0,
        anchor_count: 0,
        repeatable_item_count: 0,
        missing_dynamic_ports: Vec::new(),
        actions: BTreeSet::new(),
        inspectors: BTreeSet::new(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: 0,
        measurement_mode: OpenGpuiMeasurementMode::ProjectionFallback,
        measurement_coverage: OpenGpuiMeasurementCoverage::default(),
    };

    for (node_id, node) in graph.nodes() {
        let Some(descriptor) = node_registry.view_descriptor(&node.kind) else {
            continue;
        };
        let size = node_size(node);
        let layout = projected_node_surface_graph_layout(&descriptor, node, &graph, node_id, size);
        assert_layout_regions_inside_node(&layout, size, &descriptor.kind.0);
        let measurement = project_node_measurement(node_id, node, &graph, &descriptor);
        assert_measurement_inside_node(&measurement, size, &descriptor.kind.0);

        report.measured_nodes += 1;
        report.slot_count += measurement.slots.len();
        report.anchor_count += measurement.anchors.len();
        report.measurement_coverage.projection_fallback_regions +=
            measurement.slots.len() + measurement.anchors.len();
        report.measurement_coverage.measured_slots += measurement.slots.len();
        report.measurement_coverage.measured_anchors += measurement.anchors.len();

        let repeatable_items = repeatable_item_projection(&descriptor, node, &graph, node_id);
        report.repeatable_item_count += repeatable_items.len();
        report
            .missing_dynamic_ports
            .extend(repeatable_port_diagnostics(&repeatable_items));
        report
            .actions
            .extend(descriptor.actions.iter().map(|action| action.key.clone()));
        report.inspectors.extend(
            descriptor
                .inspectors
                .iter()
                .map(|inspector| inspector.key.clone()),
        );
        collect_control_primitives(&mut report, &descriptor, node);
    }

    Ok(report)
}

/// Build product-shaped regression evidence from layout-pass semantic regions.
pub fn layout_pass_product_fixture_report(
    kind: OpenGpuiProductFixtureKind,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let spec = ProductFixtureSpec::for_kind(kind);
    let kit_registry = NodeKitRegistry::builtin();
    let node_registry = kit_registry.node_registry();
    let graph = kit_registry
        .fixture_graph(&NodeKitKey::new(spec.kit_key), spec.fixture_key)
        .map_err(|error| error.to_string())?;
    let mut report = OpenGpuiProductFixtureReport {
        kind,
        kit_key: spec.kit_key.to_owned(),
        fixture_key: spec.fixture_key.to_owned(),
        node_count: graph.nodes().len(),
        measured_nodes: 0,
        slot_count: 0,
        anchor_count: 0,
        repeatable_item_count: 0,
        missing_dynamic_ports: Vec::new(),
        actions: BTreeSet::new(),
        inspectors: BTreeSet::new(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: 0,
        measurement_mode: OpenGpuiMeasurementMode::LayoutPass,
        measurement_coverage: OpenGpuiMeasurementCoverage::default(),
    };

    for (node_id, node) in graph.nodes() {
        let Some(descriptor) = node_registry.view_descriptor(&node.kind) else {
            continue;
        };
        let size = node_size(node);
        let layout = projected_node_surface_graph_layout(&descriptor, node, &graph, node_id, size);
        let (regions, control_regions) = layout_pass_regions_for_node(node_id, node, &layout);
        let fallback_anchors = measured_surface_anchors(&descriptor, &graph, node_id, &layout);
        if regions.is_empty() && fallback_anchors.is_empty() {
            continue;
        }
        let context = OpenGpuiMeasurementContext::new(*node_id, node_view_origin(node), 1.0, size)
            .with_revision(2);
        let (measurement, coverage) =
            layout_pass_measurement_from_regions(context, regions, fallback_anchors);

        assert_measurement_inside_node(&measurement, size, &descriptor.kind.0);
        assert!(
            coverage.is_full_layout_pass(),
            "layout-pass fixture for `{}` must not depend on projection fallback: {coverage:?}",
            descriptor.kind.0
        );

        report.measured_nodes += 1;
        report.slot_count += measurement.slots.len();
        report.anchor_count += measurement.anchors.len();
        report.measured_control_regions += control_regions;
        accumulate_coverage(&mut report.measurement_coverage, coverage);

        let repeatable_items = repeatable_item_projection(&descriptor, node, &graph, node_id);
        report.repeatable_item_count += repeatable_items.len();
        report
            .missing_dynamic_ports
            .extend(repeatable_port_diagnostics(&repeatable_items));
        report
            .actions
            .extend(descriptor.actions.iter().map(|action| action.key.clone()));
        report.inspectors.extend(
            descriptor
                .inspectors
                .iter()
                .map(|inspector| inspector.key.clone()),
        );
        collect_control_primitives(&mut report, &descriptor, node);
    }

    Ok(report)
}

/// Assert the product fixtures that approximate Dify, shader/blueprint, ERD, and mind-map usage.
pub fn assert_product_fixture_regression_gates() {
    let workflow = product_fixture_report(OpenGpuiProductFixtureKind::DifyWorkflow)
        .expect("Dify-style workflow fixture");
    assert!(workflow.node_count >= 4);
    assert!(workflow.measured_nodes >= 4);
    assert!(workflow.slot_count >= 1);
    assert_eq!(
        workflow.measurement_mode,
        OpenGpuiMeasurementMode::ProjectionFallback
    );
    assert!(!workflow.measurement_coverage.is_full_layout_pass());
    assert_eq!(workflow.measurement_coverage.layout_pass_regions, 0);
    assert!(
        workflow.measurement_coverage.projection_fallback_regions
            >= workflow.slot_count + workflow.anchor_count
    );

    let dify_node = schema_node_report(OpenGpuiProductFixtureKind::DifyWorkflow, "demo.llm")
        .expect("Dify-style LLM schema");
    assert!(dify_node.slot_count >= 8);
    assert!(dify_node.anchor_count >= 2);
    assert!(dify_node.repeatable_item_count >= 2);
    assert!(dify_node.actions.contains("action.llm.run"));
    assert!(dify_node.actions.contains("action.insert.llm"));
    assert!(dify_node.inspectors.contains("inspector.llm"));
    assert!(dify_node.control_primitives.contains("textarea"));
    assert!(dify_node.control_primitives.contains("select"));
    assert!(
        dify_node
            .control_primitives
            .contains("variable_picker_stub")
    );
    assert_eq!(
        dify_node.measurement_mode,
        OpenGpuiMeasurementMode::ProjectionFallback
    );
    assert!(!dify_node.measurement_coverage.is_full_layout_pass());
    assert_eq!(dify_node.measurement_coverage.layout_pass_regions, 0);

    let dify_layout = layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::DifyWorkflow)
        .expect("Dify-style layout-pass fixture");
    assert_eq!(
        dify_layout.measurement_mode,
        OpenGpuiMeasurementMode::LayoutPass
    );
    assert!(dify_layout.measurement_coverage.is_full_layout_pass());
    assert_layout_pass_capability_requires_real_bounds(&OpenGpuiAdapter::layout_pass(
        dify_layout.measurement_coverage.clone(),
    ));
    let dify_node_layout =
        layout_pass_schema_node_report(OpenGpuiProductFixtureKind::DifyWorkflow, "demo.llm")
            .expect("Dify-style LLM layout-pass schema");
    assert!(
        dify_node_layout.measurement_coverage.is_full_layout_pass(),
        "Dify LLM layout-pass coverage must be full: {:?}",
        dify_node_layout.measurement_coverage
    );
    assert!(dify_node_layout.measured_control_regions >= 3);

    let shader = product_fixture_report(OpenGpuiProductFixtureKind::ShaderBlueprint)
        .expect("shader fixture");
    assert!(shader.node_count >= 2);
    assert!(shader.measured_nodes >= 2);
    assert!(shader.slot_count >= 6);
    assert!(shader.anchor_count >= 3);

    let shader_node = schema_node_report(
        OpenGpuiProductFixtureKind::ShaderBlueprint,
        "demo.shader.mix",
    )
    .expect("shader mix schema");
    assert!(shader_node.repeatable_item_count >= 3);
    assert!(shader_node.anchor_count >= 3);
    assert!(shader_node.actions.contains("action.shader_input.add"));
    assert!(shader_node.actions.contains("action.shader_input.remove"));
    assert!(shader_node.control_primitives.contains("slider"));
    assert!(shader_node.control_primitives.contains("select"));
    assert!(shader_node.missing_dynamic_ports.is_empty());
    let shader_layout =
        layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::ShaderBlueprint)
            .expect("shader layout-pass fixture");
    assert!(shader_layout.measurement_coverage.is_full_layout_pass());
    let shader_node_layout = layout_pass_schema_node_report(
        OpenGpuiProductFixtureKind::ShaderBlueprint,
        "demo.shader.mix",
    )
    .expect("shader mix layout-pass schema");
    assert!(
        shader_node_layout
            .measurement_coverage
            .is_full_layout_pass()
    );
    assert!(shader_node_layout.repeatable_item_count >= 3);
    assert!(shader_node_layout.anchor_count >= 3);

    let erd = product_fixture_report(OpenGpuiProductFixtureKind::ErdTable).expect("ERD fixture");
    assert!(erd.node_count >= 2);
    assert!(erd.repeatable_item_count >= 3);
    assert!(erd.anchor_count >= 2);
    assert!(erd.actions.contains("action.column.add"));
    assert!(erd.inspectors.contains("inspector.column.email"));
    assert!(erd.control_primitives.contains("text_input"));
    let erd_layout = layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::ErdTable)
        .expect("ERD layout-pass fixture");
    assert!(erd_layout.measurement_coverage.is_full_layout_pass());
    assert!(erd_layout.repeatable_item_count >= 3);
    assert!(erd_layout.measured_control_regions >= 3);

    let mind =
        product_fixture_report(OpenGpuiProductFixtureKind::MindMap).expect("mind-map fixture");
    assert!(mind.node_count >= 3);
    assert!(mind.measured_nodes >= 3);
    assert!(mind.slot_count >= 3);
    assert!(mind.control_primitives.contains("text_input"));
    let mind_layout = layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::MindMap)
        .expect("mind-map layout-pass fixture");
    assert!(mind_layout.measurement_coverage.is_full_layout_pass());
    assert!(mind_layout.slot_count >= 3);
}

/// Assert that descriptor-driven interaction states exist for the GPUI adapter to render locally.
pub fn assert_authoring_interaction_regression_gates() {
    let registry = NodeKitRegistry::builtin().node_registry();
    let llm = descriptor(&registry, "demo.llm");
    let dropped = crate::project_dropped_wire_menu(
        &registry,
        jellyflow::runtime::runtime::connection::ConnectionHandleRef::new(
            NodeId::from_u128(1),
            jellyflow::core::PortId::from_u128(2),
            jellyflow::core::PortDirection::Out,
        ),
        Some(&PortKey::new("completion")),
        jellyflow::core::CanvasPoint { x: 320.0, y: 160.0 },
    );
    assert!(
        dropped
            .actions
            .iter()
            .any(|action| { action.key == "action.insert.llm" && action.dispatchable() })
    );

    let node_menu = project_actions_for_surface(
        &llm,
        &OpenGpuiActionSurface::Node {
            node_kind: "demo.llm".to_owned(),
        },
    );
    assert!(
        node_menu
            .actions
            .iter()
            .any(|action| action.key == "action.llm.run")
    );

    let inspectors = crate::project_inspectors_for_surface(
        &llm,
        &llm.default_data,
        &OpenGpuiInspectorSurface::Node {
            node_kind: "demo.llm".to_owned(),
        },
    );
    assert!(inspectors.iter().any(|inspector| {
        inspector.key == "inspector.llm"
            && inspector
                .editable_controls()
                .any(|control| control.key == "inspector.model")
    }));
}

fn schema_node_report(
    fixture_kind: OpenGpuiProductFixtureKind,
    kind: &str,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let kit_registry = NodeKitRegistry::builtin();
    let registry = kit_registry.node_registry();
    let descriptor = descriptor(&registry, kind);
    let schema = registry
        .get(&descriptor.kind)
        .ok_or_else(|| format!("missing schema `{}`", descriptor.kind.0))?;
    let instantiation = schema.instantiate(jellyflow::core::CanvasPoint::default());
    let (node_id, node, ports) = instantiation.into_parts();
    let mut graph_builder =
        jellyflow::core::GraphBuilder::new(jellyflow::core::GraphId::from_u128(0x67_70_75_69))
            .with_node(node_id, node.clone());
    for (port_id, port) in ports {
        graph_builder = graph_builder.with_port(port_id, port);
    }
    let graph = graph_builder.build_unchecked();
    let layout =
        projected_node_surface_graph_layout(&descriptor, &node, &graph, &node_id, node_size(&node));
    assert_layout_regions_inside_node(&layout, node_size(&node), &descriptor.kind.0);
    let measurement = project_node_measurement(&node_id, &node, &graph, &descriptor);
    assert_measurement_inside_node(&measurement, node_size(&node), &descriptor.kind.0);

    let repeatable_items = repeatable_item_projection(&descriptor, &node, &graph, &node_id);
    let mut report = OpenGpuiProductFixtureReport {
        kind: fixture_kind,
        kit_key: kit_registry
            .manifest_for_kind(&descriptor.kind)
            .map(|manifest| manifest.key.0.clone())
            .unwrap_or_default(),
        fixture_key: kind.to_owned(),
        node_count: 1,
        measured_nodes: 1,
        slot_count: measurement.slots.len(),
        anchor_count: measurement.anchors.len(),
        repeatable_item_count: repeatable_items.len(),
        missing_dynamic_ports: repeatable_port_diagnostics(&repeatable_items),
        actions: descriptor
            .actions
            .iter()
            .map(|action| action.key.clone())
            .collect(),
        inspectors: descriptor
            .inspectors
            .iter()
            .map(|inspector| inspector.key.clone())
            .collect(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: 0,
        measurement_mode: OpenGpuiMeasurementMode::ProjectionFallback,
        measurement_coverage: OpenGpuiMeasurementCoverage {
            layout_pass_regions: 0,
            projection_fallback_regions: measurement.slots.len() + measurement.anchors.len(),
            missing_regions: 0,
            stale_regions: 0,
            partial_regions: 0,
            duplicate_regions: 0,
            measured_slots: measurement.slots.len(),
            measured_anchors: measurement.anchors.len(),
        },
    };
    collect_control_primitives(&mut report, &descriptor, &node);
    Ok(report)
}

fn layout_pass_schema_node_report(
    fixture_kind: OpenGpuiProductFixtureKind,
    kind: &str,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let kit_registry = NodeKitRegistry::builtin();
    let registry = kit_registry.node_registry();
    let descriptor = descriptor(&registry, kind);
    let schema = registry
        .get(&descriptor.kind)
        .ok_or_else(|| format!("missing schema `{}`", descriptor.kind.0))?;
    let instantiation = schema.instantiate(jellyflow::core::CanvasPoint::default());
    let (node_id, node, ports) = instantiation.into_parts();
    let mut graph_builder =
        jellyflow::core::GraphBuilder::new(jellyflow::core::GraphId::from_u128(0x67_70_75_69))
            .with_node(node_id, node.clone());
    for (port_id, port) in ports {
        graph_builder = graph_builder.with_port(port_id, port);
    }
    let graph = graph_builder.build_unchecked();
    let size = node_size(&node);
    let layout = projected_node_surface_graph_layout(&descriptor, &node, &graph, &node_id, size);
    let (regions, control_regions) = layout_pass_regions_for_node(&node_id, &node, &layout);
    let fallback_anchors = measured_surface_anchors(&descriptor, &graph, &node_id, &layout);
    let context = OpenGpuiMeasurementContext::new(node_id, node_view_origin(&node), 1.0, size)
        .with_revision(2);
    let (measurement, coverage) =
        layout_pass_measurement_from_regions(context, regions, fallback_anchors);
    assert_measurement_inside_node(&measurement, size, &descriptor.kind.0);

    let repeatable_items = repeatable_item_projection(&descriptor, &node, &graph, &node_id);
    let mut report = OpenGpuiProductFixtureReport {
        kind: fixture_kind,
        kit_key: kit_registry
            .manifest_for_kind(&descriptor.kind)
            .map(|manifest| manifest.key.0.clone())
            .unwrap_or_default(),
        fixture_key: kind.to_owned(),
        node_count: 1,
        measured_nodes: 1,
        slot_count: measurement.slots.len(),
        anchor_count: measurement.anchors.len(),
        repeatable_item_count: repeatable_items.len(),
        missing_dynamic_ports: repeatable_port_diagnostics(&repeatable_items),
        actions: descriptor
            .actions
            .iter()
            .map(|action| action.key.clone())
            .collect(),
        inspectors: descriptor
            .inspectors
            .iter()
            .map(|inspector| inspector.key.clone())
            .collect(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: control_regions,
        measurement_mode: OpenGpuiMeasurementMode::LayoutPass,
        measurement_coverage: coverage,
    };
    collect_control_primitives(&mut report, &descriptor, &node);
    Ok(report)
}

fn layout_pass_regions_for_node(
    node_id: &NodeId,
    node: &Node,
    layout: &OpenGpuiNodeSurfaceLayout,
) -> (Vec<OpenGpuiMeasuredRegion>, usize) {
    let node_view_origin = node_view_origin(node);
    let mut regions = Vec::new();
    let mut control_regions = 0;

    for slot in &layout.slots {
        regions.push(
            OpenGpuiMeasurementId::slot(*node_id, slot.slot.key.clone())
                .into_region(view_bounds_from_rect(node_view_origin, slot.rect)),
        );
        if let Some(anchor) = slot
            .descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.anchor.as_ref())
        {
            regions.push(
                OpenGpuiMeasurementId::anchor(*node_id, anchor.clone())
                    .into_region(view_bounds_from_rect(node_view_origin, slot.anchor_rect)),
            );
        }
        if let Some(descriptor) = &slot.descriptor {
            let controls = project_slot_controls(&node.data, descriptor);
            let control_count = controls.len();
            control_regions += control_count;
            for (index, control) in controls.into_iter().enumerate() {
                regions.push(
                    OpenGpuiMeasurementId::control_in_slot(
                        *node_id,
                        descriptor.key.as_str(),
                        control.key,
                    )
                    .into_region(view_bounds_from_rect(
                        node_view_origin,
                        control_region_rect(slot.rect, index, control_count.max(1)),
                    )),
                );
            }
        }
    }

    for repeatable in &layout.repeatables {
        regions.push(
            OpenGpuiMeasurementId::slot(*node_id, repeatable.projection.key.clone())
                .into_region(view_bounds_from_rect(node_view_origin, repeatable.rect)),
        );
    }

    for item in &layout.repeatable_items {
        regions.push(
            OpenGpuiMeasurementId::repeatable_item(
                *node_id,
                item.projection.slot_key.clone(),
                item.projection.item_id.clone(),
            )
            .into_region(view_bounds_from_rect(node_view_origin, item.rect)),
        );
        regions.push(
            OpenGpuiMeasurementId::anchor(*node_id, item.projection.anchor.clone())
                .into_region(view_bounds_from_rect(node_view_origin, item.anchor_rect)),
        );
    }

    (regions, control_regions)
}

fn control_region_rect(slot: CanvasRect, index: usize, count: usize) -> CanvasRect {
    let count = count.max(1) as f32;
    let track_width = (slot.size.width - 8.0).max(count);
    let segment_width = track_width / count;
    CanvasRect {
        origin: jellyflow::core::CanvasPoint {
            x: slot.origin.x + 4.0 + segment_width * index as f32,
            y: slot.origin.y + 4.0,
        },
        size: CanvasSize {
            width: (segment_width - 2.0).max(1.0),
            height: (slot.size.height - 8.0).max(1.0),
        },
    }
}

fn node_view_origin(node: &Node) -> OpenGpuiViewPoint {
    OpenGpuiViewPoint::new(node.pos.x + 13.0, node.pos.y + 29.0)
}

fn view_bounds_from_rect(origin: OpenGpuiViewPoint, rect: CanvasRect) -> OpenGpuiViewBounds {
    OpenGpuiViewBounds::new(
        OpenGpuiViewPoint::new(origin.x + rect.origin.x, origin.y + rect.origin.y),
        OpenGpuiViewSize::new(rect.size.width, rect.size.height),
    )
}

fn accumulate_coverage(
    target: &mut OpenGpuiMeasurementCoverage,
    coverage: OpenGpuiMeasurementCoverage,
) {
    target.layout_pass_regions += coverage.layout_pass_regions;
    target.projection_fallback_regions += coverage.projection_fallback_regions;
    target.missing_regions += coverage.missing_regions;
    target.stale_regions += coverage.stale_regions;
    target.partial_regions += coverage.partial_regions;
    target.duplicate_regions += coverage.duplicate_regions;
    target.measured_slots += coverage.measured_slots;
    target.measured_anchors += coverage.measured_anchors;
}

fn collect_control_primitives(
    report: &mut OpenGpuiProductFixtureReport,
    descriptor: &NodeKindViewDescriptor,
    node: &Node,
) {
    for slot in &descriptor.surface_slots {
        for control in project_slot_controls(&node.data, slot) {
            report
                .control_primitives
                .insert(control_primitive_name(control.primitive));
        }
    }
    for collection in &descriptor.repeatable_collections {
        for slot in &collection.item_template_slots {
            for control in &slot.controls {
                report
                    .control_primitives
                    .insert(control_primitive_name(primitive_for_kind(control.kind)));
            }
        }
    }
}

fn control_primitive_name(primitive: OpenGpuiControlPrimitive) -> &'static str {
    match primitive {
        OpenGpuiControlPrimitive::TextInput => "text_input",
        OpenGpuiControlPrimitive::TextArea => "textarea",
        OpenGpuiControlPrimitive::NumberInput => "number_input",
        OpenGpuiControlPrimitive::Select => "select",
        OpenGpuiControlPrimitive::MultiSelect => "multi_select",
        OpenGpuiControlPrimitive::Switch => "switch",
        OpenGpuiControlPrimitive::Slider => "slider",
        OpenGpuiControlPrimitive::CodeEditor => "code_editor",
        OpenGpuiControlPrimitive::ColorSwatch => "color_swatch",
        OpenGpuiControlPrimitive::AssetPickerStub => "asset_picker_stub",
        OpenGpuiControlPrimitive::VariablePickerStub => "variable_picker_stub",
        OpenGpuiControlPrimitive::PortBindingDisplay => "port_binding_display",
    }
}

fn descriptor(registry: &NodeRegistry, kind: &str) -> NodeKindViewDescriptor {
    registry
        .view_descriptor(&NodeKindKey::new(kind))
        .unwrap_or_else(|| panic!("missing descriptor `{kind}`"))
}

fn node_size(node: &Node) -> CanvasSize {
    node.size.unwrap_or(CanvasSize {
        width: 228.0,
        height: 168.0,
    })
}

fn assert_layout_regions_inside_node(
    layout: &OpenGpuiNodeSurfaceLayout,
    size: CanvasSize,
    label: &str,
) {
    for slot in &layout.slots {
        assert_rect_inside_node(slot.rect, size, label, &slot.slot.key);
        assert_rect_inside_node(slot.anchor_rect, size, label, &slot.slot.key);
    }
    for repeatable in &layout.repeatables {
        assert_rect_inside_node(repeatable.rect, size, label, &repeatable.projection.key);
        assert_rect_inside_node(
            repeatable.anchor_rect,
            size,
            label,
            &repeatable.projection.key,
        );
    }
    for item in &layout.repeatable_items {
        assert_rect_inside_node(item.rect, size, label, &item.projection.slot_key);
        assert_rect_inside_node(item.anchor_rect, size, label, &item.projection.anchor);
    }
}

fn assert_measurement_inside_node(measurement: &NodeMeasurement, size: CanvasSize, label: &str) {
    for slot in &measurement.slots {
        assert_rect_inside_node(slot.rect, size, label, &slot.key);
    }
    for anchor in &measurement.anchors {
        assert_rect_inside_node(anchor.rect, size, label, &anchor.anchor);
    }
}

fn assert_rect_inside_node(rect: CanvasRect, size: CanvasSize, node_label: &str, region: &str) {
    assert!(
        rect.is_positive_finite(),
        "{node_label}:{region} must have positive finite bounds: {rect:?}"
    );
    assert!(
        rect.origin.x >= 0.0
            && rect.origin.y >= 0.0
            && rect.origin.x + rect.size.width <= size.width + 0.01
            && rect.origin.y + rect.size.height <= size.height + 0.01,
        "{node_label}:{region} must stay inside node size {size:?}: {rect:?}"
    );
}

struct ProductFixtureSpec {
    kit_key: &'static str,
    fixture_key: &'static str,
}

impl ProductFixtureSpec {
    fn for_kind(kind: OpenGpuiProductFixtureKind) -> Self {
        match kind {
            OpenGpuiProductFixtureKind::DifyWorkflow => Self {
                kit_key: "workflow.automation",
                fixture_key: "workflow.review",
            },
            OpenGpuiProductFixtureKind::ShaderBlueprint => Self {
                kit_key: "shader.blueprint",
                fixture_key: "shader.material_mix",
            },
            OpenGpuiProductFixtureKind::ErdTable => Self {
                kit_key: "erd.table",
                fixture_key: "erd.customer_orders",
            },
            OpenGpuiProductFixtureKind::MindMap => Self {
                kit_key: "mind-map.knowledge-canvas",
                fixture_key: "mind-map.strategy",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OpenGpuiMeasurementMode;

    #[test]
    fn helper_rejects_full_claim_without_layout_pass_mode() {
        let adapter = OpenGpuiAdapter::projection_fallback();
        assert_layout_pass_capability_requires_real_bounds(&adapter);
        assert_eq!(
            adapter.measurement_mode(),
            OpenGpuiMeasurementMode::ProjectionFallback
        );
    }

    #[test]
    fn product_fixtures_cover_gpui_authoring_regressions() {
        assert_product_fixture_regression_gates();
    }

    #[test]
    fn interaction_fixtures_cover_gpui_authoring_states() {
        assert_authoring_interaction_regression_gates();
    }
}
