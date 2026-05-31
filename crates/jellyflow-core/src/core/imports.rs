use std::collections::{BTreeMap, BTreeSet};

use super::{Graph, GraphId};

/// Reserved metadata for a graph import.
///
/// The presence of an entry declares a dependency from the importing graph onto the imported
/// graph. Additional metadata (aliasing, pinning, namespaces) can be added later without changing
/// the key contract.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GraphImport {
    /// Optional import alias for UI and authoring ergonomics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum GraphImportError {
    #[error("import references missing graph: from={from} to={to}")]
    MissingGraph { from: GraphId, to: GraphId },

    #[error("import cycle detected")]
    Cycle { cycle: Vec<GraphId> },
}

#[derive(Debug, Clone)]
pub struct GraphImportClosure {
    /// All reachable imports (excluding the root graph id).
    pub reachable: BTreeSet<GraphId>,
    /// Deterministic DFS postorder (dependencies first), excluding the root graph id.
    pub order: Vec<GraphId>,
}

impl GraphImportClosure {
    pub fn contains(&self, graph: GraphId) -> bool {
        self.reachable.contains(&graph)
    }
}

/// Resolves the transitive import closure for a graph id with deterministic semantics.
///
/// Determinism contract:
/// - imports are traversed in `BTreeMap` key order (`GraphId` total order),
/// - the resulting `order` is DFS postorder (dependencies appear before dependents).
pub fn resolve_import_closure<'a>(
    root_graph: &'a Graph,
    resolver: impl FnMut(GraphId) -> Option<&'a Graph>,
) -> Result<GraphImportClosure, GraphImportError> {
    ImportClosureResolver::new(root_graph, resolver).finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportVisitMark {
    Temporary,
    Permanent,
}

struct ImportClosureResolver<'a, R>
where
    R: FnMut(GraphId) -> Option<&'a Graph>,
{
    root: GraphId,
    root_graph: &'a Graph,
    resolver: R,
    marks: BTreeMap<GraphId, ImportVisitMark>,
    stack: Vec<GraphId>,
    closure: GraphImportClosure,
}

impl<'a, R> ImportClosureResolver<'a, R>
where
    R: FnMut(GraphId) -> Option<&'a Graph>,
{
    fn new(root_graph: &'a Graph, resolver: R) -> Self {
        Self {
            root: root_graph.graph_id,
            root_graph,
            resolver,
            marks: BTreeMap::new(),
            stack: Vec::new(),
            closure: GraphImportClosure {
                reachable: BTreeSet::new(),
                order: Vec::new(),
            },
        }
    }

    fn finish(mut self) -> Result<GraphImportClosure, GraphImportError> {
        self.visit(self.root, self.root)?;
        Ok(self.closure)
    }

    fn visit(&mut self, node: GraphId, from: GraphId) -> Result<(), GraphImportError> {
        match self.marks.get(&node).copied() {
            Some(ImportVisitMark::Permanent) => return Ok(()),
            Some(ImportVisitMark::Temporary) => return Err(self.cycle_error(node)),
            None => {}
        }

        self.marks.insert(node, ImportVisitMark::Temporary);
        self.stack.push(node);

        for dep in self.dependencies(node, from)? {
            self.closure.reachable.insert(dep);
            self.visit(dep, node)?;
        }

        self.stack.pop();
        self.marks.insert(node, ImportVisitMark::Permanent);

        if node != self.root {
            self.closure.order.push(node);
        }

        Ok(())
    }

    fn dependencies(
        &mut self,
        node: GraphId,
        from: GraphId,
    ) -> Result<Vec<GraphId>, GraphImportError> {
        let imports = if node == self.root {
            &self.root_graph.imports
        } else {
            let Some(graph) = (self.resolver)(node) else {
                return Err(GraphImportError::MissingGraph { from, to: node });
            };
            &graph.imports
        };

        Ok(imports.keys().copied().collect())
    }

    fn cycle_error(&self, node: GraphId) -> GraphImportError {
        if let Some(pos) = self.stack.iter().position(|id| *id == node) {
            let mut cycle = self.stack[pos..].to_vec();
            cycle.push(node);
            return GraphImportError::Cycle { cycle };
        }

        GraphImportError::Cycle {
            cycle: vec![node, node],
        }
    }
}
