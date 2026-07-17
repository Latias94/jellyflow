//! Semantic resource validation for decoded persistence models.
//!
//! The two whole-document validators are the primary entry points. `PayloadCounter` is the
//! third narrow interface used by catalog migrations to preserve cumulative charging and stop
//! before invoking a later migrator after an oversized output.

use jellyflow_core::core::{BindingEndpoint, Graph};
use jellyflow_core::types::TypeDesc;
use serde_json::Value;

use crate::io::view_state::ViewSessionSeed;

use super::{PersistenceLimitError, PersistenceLimits, PersistenceResource};

pub(crate) fn validate_graph_resources(
    graph: &Graph,
    limits: PersistenceLimits,
) -> Result<(), PersistenceLimitError> {
    let map_entities = [
        graph.imports().len(),
        graph.symbols().len(),
        graph.nodes().len(),
        graph.ports().len(),
        graph.edges().len(),
        graph.groups().len(),
        graph.sticky_notes().len(),
        graph.bindings().len(),
    ]
    .into_iter()
    .fold(0_usize, usize::saturating_add);
    let ordered_port_references = graph
        .nodes()
        .values()
        .map(|node| node.ports.len())
        .fold(0_usize, usize::saturating_add);
    let entity_count = map_entities.saturating_add(ordered_port_references);
    limits.ensure(PersistenceResource::Entities, entity_count)?;

    let mut counter = PayloadCounter::new(limits);
    counter.entity_count = entity_count;
    for import in graph.imports().values() {
        counter.add_optional_string(import.alias.as_deref())?;
    }
    for symbol in graph.symbols().values() {
        counter.add_string(&symbol.name)?;
        if let Some(ty) = &symbol.ty {
            counter.visit_type(ty)?;
        }
        if let Some(value) = &symbol.default_value {
            counter.visit_json(value, 1)?;
        }
        counter.visit_json(&symbol.meta, 1)?;
    }
    for node in graph.nodes().values() {
        counter.add_string(&node.kind.0)?;
        counter.visit_json(&node.data, 1)?;
    }
    for port in graph.ports().values() {
        counter.add_string(&port.key.0)?;
        if let Some(ty) = &port.ty {
            counter.visit_type(ty)?;
        }
        counter.visit_json(&port.data, 1)?;
    }
    for edge in graph.edges().values() {
        counter.visit_json(&edge.data, 1)?;
        counter.add_optional_string(edge.view.renderer_key.as_deref())?;
        counter.add_optional_string(edge.view.label.as_deref())?;
        counter.add_optional_string(edge.view.source_marker_key.as_deref())?;
        counter.add_optional_string(edge.view.target_marker_key.as_deref())?;
        counter.add_optional_string(edge.view.style_token.as_deref())?;
    }
    for group in graph.groups().values() {
        counter.add_string(&group.title)?;
        counter.add_optional_string(group.color.as_deref())?;
    }
    for note in graph.sticky_notes().values() {
        counter.add_string(&note.text)?;
        counter.add_optional_string(note.color.as_deref())?;
    }
    for binding in graph.bindings().values() {
        counter.visit_binding_endpoint(&binding.subject)?;
        counter.visit_binding_endpoint(&binding.target)?;
        counter.add_optional_string(binding.kind.as_deref())?;
        counter.visit_json(&binding.meta, 1)?;
    }
    Ok(())
}

pub(crate) fn validate_view_session_seed_resources(
    seed: &ViewSessionSeed,
    limits: PersistenceLimits,
) -> Result<(), PersistenceLimitError> {
    let selection = seed.selection();
    let stacking = seed.stacking();
    let entity_count = [
        selection.nodes().len(),
        selection.edges().len(),
        selection.groups().len(),
        stacking.nodes().len(),
        stacking.edges().len(),
        stacking.groups().len(),
    ]
    .into_iter()
    .fold(0_usize, usize::saturating_add);
    limits.ensure(PersistenceResource::Entities, entity_count)
}

pub(crate) struct PayloadCounter {
    limits: PersistenceLimits,
    entity_count: usize,
    collection_items: usize,
    string_bytes: usize,
    json_values: usize,
}

impl PayloadCounter {
    pub(crate) fn new(limits: PersistenceLimits) -> Self {
        Self {
            limits,
            entity_count: 0,
            collection_items: 0,
            string_bytes: 0,
            json_values: 0,
        }
    }

    fn add_optional_string(&mut self, value: Option<&str>) -> Result<(), PersistenceLimitError> {
        if let Some(value) = value {
            self.add_string(value)?;
        }
        Ok(())
    }

    fn add_string(&mut self, value: &str) -> Result<(), PersistenceLimitError> {
        self.string_bytes = self.string_bytes.saturating_add(value.len());
        self.limits
            .ensure(PersistenceResource::StringBytes, self.string_bytes)
    }

    pub(crate) fn visit_json(
        &mut self,
        value: &Value,
        depth: usize,
    ) -> Result<(), PersistenceLimitError> {
        self.limits.ensure(PersistenceResource::JsonDepth, depth)?;
        self.json_values = self.json_values.saturating_add(1);
        self.limits
            .ensure(PersistenceResource::JsonValues, self.json_values)?;

        match value {
            Value::String(value) => self.add_string(value)?,
            Value::Array(values) => {
                for value in values {
                    self.add_collection_item()?;
                    self.visit_json(value, depth.saturating_add(1))?;
                }
            }
            Value::Object(values) => {
                for (key, value) in values {
                    self.add_collection_item()?;
                    self.add_string(key)?;
                    self.visit_json(value, depth.saturating_add(1))?;
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
        Ok(())
    }

    fn add_collection_item(&mut self) -> Result<(), PersistenceLimitError> {
        self.collection_items = self.collection_items.saturating_add(1);
        self.limits
            .ensure(PersistenceResource::CollectionItems, self.collection_items)
    }

    fn visit_type(&mut self, ty: &TypeDesc) -> Result<(), PersistenceLimitError> {
        enum Pending<'a> {
            Type(&'a TypeDesc, usize),
            ObjectFields(
                std::collections::btree_map::Iter<'a, String, TypeDesc>,
                usize,
            ),
            Types(std::slice::Iter<'a, TypeDesc>, usize),
        }

        let mut pending = vec![Pending::Type(ty, 1)];
        while let Some(next) = pending.pop() {
            match next {
                Pending::Type(ty, depth) => {
                    self.limits.ensure(PersistenceResource::JsonDepth, depth)?;
                    self.entity_count = self.entity_count.saturating_add(1);
                    self.limits
                        .ensure(PersistenceResource::Entities, self.entity_count)?;

                    let child_depth = depth.saturating_add(1);
                    match ty {
                        TypeDesc::List { of } | TypeDesc::Option { of } => {
                            pending.push(Pending::Type(of, child_depth));
                        }
                        TypeDesc::Map { key, value } => {
                            pending.push(Pending::Type(value, child_depth));
                            pending.push(Pending::Type(key, child_depth));
                        }
                        TypeDesc::Object { fields, .. } => {
                            pending.push(Pending::ObjectFields(fields.iter(), child_depth));
                        }
                        TypeDesc::Union { types } => {
                            pending.push(Pending::Types(types.iter(), child_depth));
                        }
                        TypeDesc::Opaque { key, params } => {
                            self.add_string(key)?;
                            pending.push(Pending::Types(params.iter(), child_depth));
                        }
                        TypeDesc::Any
                        | TypeDesc::Unknown
                        | TypeDesc::Never
                        | TypeDesc::Null
                        | TypeDesc::Bool
                        | TypeDesc::Int
                        | TypeDesc::Float
                        | TypeDesc::String
                        | TypeDesc::Bytes
                        | TypeDesc::Var { .. } => {}
                    }
                }
                Pending::ObjectFields(mut fields, depth) => {
                    if let Some((name, field)) = fields.next() {
                        self.add_string(name)?;
                        pending.push(Pending::ObjectFields(fields, depth));
                        pending.push(Pending::Type(field, depth));
                    }
                }
                Pending::Types(mut types, depth) => {
                    if let Some(ty) = types.next() {
                        pending.push(Pending::Types(types, depth));
                        pending.push(Pending::Type(ty, depth));
                    }
                }
            }
        }
        Ok(())
    }

    fn visit_binding_endpoint(
        &mut self,
        endpoint: &BindingEndpoint,
    ) -> Result<(), PersistenceLimitError> {
        if let BindingEndpoint::Source { anchor } = endpoint {
            self.add_string(&anchor.source_id)?;
            self.visit_json(&anchor.payload, 1)?;
        }
        Ok(())
    }
}
