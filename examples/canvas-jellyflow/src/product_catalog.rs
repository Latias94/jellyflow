//! Flagship-product discovery, creation, and static port presentation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize, NodeKindKey, PortKey};
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::schema::{
    CreateFromWireCandidate, NodeCatalog, NodeCatalogInstantiationError, NodeCreation,
};

use crate::nodes::{
    BLUEPRINT_CALL_DEFAULT_SIZE, BLUEPRINT_CALL_KIND, SHADER_MULTIPLY_DEFAULT_SIZE,
    SHADER_MULTIPLY_KIND, WORKFLOW_CONDITION_DEFAULT_SIZE, WORKFLOW_CONDITION_KIND,
    WORKFLOW_LLM_DEFAULT_SIZE, WORKFLOW_LLM_KIND,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeProductPortPresentation {
    key: PortKey,
    label: Arc<str>,
    side: HandlePosition,
    order: i32,
    group: Option<Arc<str>>,
}

impl NativeProductPortPresentation {
    fn new(
        key: impl Into<PortKey>,
        label: impl Into<Arc<str>>,
        side: HandlePosition,
        order: i32,
        group: Option<impl Into<Arc<str>>>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            side,
            order,
            group: group.map(Into::into),
        }
    }

    #[must_use]
    pub fn key(&self) -> &PortKey {
        &self.key
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn side(&self) -> HandlePosition {
        self.side
    }

    #[must_use]
    pub const fn order(&self) -> i32 {
        self.order
    }

    #[must_use]
    pub fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeProductNodePresentation {
    kind: NodeKindKey,
    kind_version: u32,
    title: Arc<str>,
    category: Arc<[Arc<str>]>,
    keywords: Arc<[Arc<str>]>,
    initial_size: CanvasSize,
    ports: BTreeMap<PortKey, NativeProductPortPresentation>,
}

impl NativeProductNodePresentation {
    fn new(
        kind: impl Into<NodeKindKey>,
        kind_version: u32,
        title: impl Into<Arc<str>>,
        category: impl IntoIterator<Item = impl Into<Arc<str>>>,
        keywords: impl IntoIterator<Item = impl Into<Arc<str>>>,
        initial_size: CanvasSize,
        ports: impl IntoIterator<Item = NativeProductPortPresentation>,
    ) -> Result<Self, NativeProductCatalogError> {
        let kind = kind.into();
        let title = title.into();
        let category = category.into_iter().map(Into::into).collect::<Vec<_>>();
        let keywords = keywords.into_iter().map(Into::into).collect::<Vec<_>>();
        if kind.0.trim().is_empty() {
            return Err(NativeProductCatalogError::EmptyKind);
        }
        if kind_version == 0 {
            return Err(NativeProductCatalogError::InvalidKindVersion { kind });
        }
        if title.trim().is_empty() {
            return Err(NativeProductCatalogError::EmptyTitle { kind });
        }
        if category.is_empty() || category.iter().any(|segment| segment.trim().is_empty()) {
            return Err(NativeProductCatalogError::InvalidCategory { kind });
        }
        if keywords.iter().any(|keyword| keyword.trim().is_empty()) {
            return Err(NativeProductCatalogError::EmptyKeyword { kind });
        }
        if !initial_size.is_positive_finite() {
            return Err(NativeProductCatalogError::InvalidInitialSize { kind });
        }

        let mut by_key = BTreeMap::new();
        let mut slots = BTreeSet::new();
        for port in ports {
            if port.label.trim().is_empty() {
                return Err(NativeProductCatalogError::EmptyPortLabel {
                    kind,
                    port: port.key,
                });
            }
            if port
                .group
                .as_deref()
                .is_some_and(|group| group.trim().is_empty())
            {
                return Err(NativeProductCatalogError::EmptyPortGroup {
                    kind,
                    port: port.key,
                });
            }
            if !slots.insert((side_key(port.side), port.group.clone(), port.order)) {
                return Err(NativeProductCatalogError::DuplicatePortOrder {
                    kind,
                    side: port.side,
                    group: port.group,
                    order: port.order,
                });
            }
            let key = port.key.clone();
            if by_key.insert(key.clone(), port).is_some() {
                return Err(NativeProductCatalogError::DuplicatePortPresentation {
                    kind,
                    port: key,
                });
            }
        }

        Ok(Self {
            kind,
            kind_version,
            title,
            category: category.into(),
            keywords: keywords.into(),
            initial_size,
            ports: by_key,
        })
    }

    #[must_use]
    pub fn kind(&self) -> &NodeKindKey {
        &self.kind
    }

    #[must_use]
    pub const fn kind_version(&self) -> u32 {
        self.kind_version
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn category(&self) -> &[Arc<str>] {
        &self.category
    }

    #[must_use]
    pub fn keywords(&self) -> &[Arc<str>] {
        &self.keywords
    }

    #[must_use]
    pub const fn initial_size(&self) -> CanvasSize {
        self.initial_size
    }

    #[must_use]
    pub fn port(&self, key: &PortKey) -> Option<&NativeProductPortPresentation> {
        self.ports.get(key)
    }

    pub fn ports(&self) -> impl Iterator<Item = &NativeProductPortPresentation> {
        self.ports.values()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeProductCatalogError {
    #[error("product node kind must not be empty")]
    EmptyKind,
    #[error("product node kind `{kind:?}` must declare a positive version")]
    InvalidKindVersion { kind: NodeKindKey },
    #[error("product node kind `{kind:?}` must declare a non-empty title")]
    EmptyTitle { kind: NodeKindKey },
    #[error("product node kind `{kind:?}` must declare a non-empty category path")]
    InvalidCategory { kind: NodeKindKey },
    #[error("product node kind `{kind:?}` declares an empty search keyword")]
    EmptyKeyword { kind: NodeKindKey },
    #[error("product node kind `{kind:?}` declares invalid initial size")]
    InvalidInitialSize { kind: NodeKindKey },
    #[error("product node kind `{kind:?}` declares an empty label for port `{port:?}`")]
    EmptyPortLabel { kind: NodeKindKey, port: PortKey },
    #[error("product node kind `{kind:?}` declares an empty group for port `{port:?}`")]
    EmptyPortGroup { kind: NodeKindKey, port: PortKey },
    #[error("product node kind `{kind:?}` declares port `{port:?}` more than once")]
    DuplicatePortPresentation { kind: NodeKindKey, port: PortKey },
    #[error(
        "product node kind `{kind:?}` reuses port order {order} on {side:?} in group {group:?}"
    )]
    DuplicatePortOrder {
        kind: NodeKindKey,
        side: HandlePosition,
        group: Option<Arc<str>>,
        order: i32,
    },
    #[error("product node kind `{kind:?}` is registered more than once")]
    DuplicateKind { kind: NodeKindKey },
}

#[derive(Debug, Clone)]
pub struct NativeProductCatalog {
    entries: Arc<BTreeMap<NodeKindKey, NativeProductNodePresentation>>,
}

impl NativeProductCatalog {
    fn from_entries(
        entries: impl IntoIterator<Item = NativeProductNodePresentation>,
    ) -> Result<Self, NativeProductCatalogError> {
        let mut by_kind = BTreeMap::new();
        for entry in entries {
            let kind = entry.kind.clone();
            if by_kind.insert(kind.clone(), entry).is_some() {
                return Err(NativeProductCatalogError::DuplicateKind { kind });
            }
        }
        Ok(Self {
            entries: Arc::new(by_kind),
        })
    }

    #[must_use]
    pub fn get(
        &self,
        kind: &NodeKindKey,
        kind_version: u32,
    ) -> Option<&NativeProductNodePresentation> {
        self.entries
            .get(kind)
            .filter(|entry| entry.kind_version == kind_version)
    }

    pub fn entries(&self) -> impl Iterator<Item = &NativeProductNodePresentation> {
        self.entries.values()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeProductCompositionIssueKind {
    MissingSemanticKind,
    NonCanonicalSemanticKind { canonical: NodeKindKey },
    SemanticVersionMismatch { semantic_version: u32 },
    MissingRenderer,
    MissingPortPresentation { port: PortKey },
    ExtraPortPresentation { port: PortKey },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeProductCompositionIssue {
    kind: NodeKindKey,
    kind_version: u32,
    issue: NativeProductCompositionIssueKind,
}

impl NativeProductCompositionIssue {
    #[must_use]
    pub fn kind(&self) -> &NodeKindKey {
        &self.kind
    }

    #[must_use]
    pub const fn kind_version(&self) -> u32 {
        self.kind_version
    }

    #[must_use]
    pub const fn issue(&self) -> &NativeProductCompositionIssueKind {
        &self.issue
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("native product composition is invalid: {issues:?}")]
pub struct NativeProductCompositionError {
    issues: Vec<NativeProductCompositionIssue>,
}

impl NativeProductCompositionError {
    #[must_use]
    pub fn issues(&self) -> &[NativeProductCompositionIssue] {
        &self.issues
    }
}

#[derive(Debug, Clone)]
pub struct NativeProductComposition {
    semantic: Arc<NodeCatalog>,
    product: NativeProductCatalog,
}

impl NativeProductComposition {
    pub fn compose(
        semantic: Arc<NodeCatalog>,
        product: NativeProductCatalog,
        registry: &OpenGpuiNodeRegistry,
    ) -> Result<Self, NativeProductCompositionError> {
        let mut issues = Vec::new();
        for entry in product.entries() {
            let kind = entry.kind();
            let semantic = match semantic.resolve_kind(kind) {
                None => {
                    issues.push(composition_issue(
                        entry,
                        NativeProductCompositionIssueKind::MissingSemanticKind,
                    ));
                    continue;
                }
                Some(canonical) if canonical != kind => {
                    issues.push(composition_issue(
                        entry,
                        NativeProductCompositionIssueKind::NonCanonicalSemanticKind {
                            canonical: canonical.clone(),
                        },
                    ));
                    continue;
                }
                Some(_) => semantic
                    .get(kind)
                    .expect("a canonical semantic kind resolves to its specification"),
            };
            if semantic.latest_kind_version() != entry.kind_version() {
                issues.push(composition_issue(
                    entry,
                    NativeProductCompositionIssueKind::SemanticVersionMismatch {
                        semantic_version: semantic.latest_kind_version(),
                    },
                ));
            }
            if registry
                .renderer_stamp(kind, entry.kind_version())
                .is_none()
            {
                issues.push(composition_issue(
                    entry,
                    NativeProductCompositionIssueKind::MissingRenderer,
                ));
            }

            let semantic_ports = semantic
                .ports()
                .iter()
                .map(|port| port.key().clone())
                .collect::<BTreeSet<_>>();
            let product_ports = entry.ports.keys().cloned().collect::<BTreeSet<_>>();
            for port in semantic_ports.difference(&product_ports) {
                issues.push(composition_issue(
                    entry,
                    NativeProductCompositionIssueKind::MissingPortPresentation {
                        port: port.clone(),
                    },
                ));
            }
            for port in product_ports.difference(&semantic_ports) {
                issues.push(composition_issue(
                    entry,
                    NativeProductCompositionIssueKind::ExtraPortPresentation { port: port.clone() },
                ));
            }
        }
        if !issues.is_empty() {
            return Err(NativeProductCompositionError { issues });
        }
        Ok(Self { semantic, product })
    }

    #[must_use]
    pub fn product(&self) -> &NativeProductCatalog {
        &self.product
    }

    #[must_use]
    pub fn project_candidates(
        &self,
        candidates: &[CreateFromWireCandidate],
    ) -> Vec<NativeProductCandidate> {
        candidates
            .iter()
            .enumerate()
            .filter_map(|(authoritative_index, candidate)| {
                let node = self
                    .product
                    .get(candidate.node_kind(), candidate.kind_version())?;
                node.port(candidate.port())?;
                Some(NativeProductCandidate {
                    authoritative_index,
                    kind: candidate.node_kind().clone(),
                    kind_version: candidate.kind_version(),
                    port: candidate.port().clone(),
                    title: Arc::clone(&node.title),
                    category: Arc::clone(&node.category),
                    keywords: Arc::clone(&node.keywords),
                })
            })
            .collect()
    }

    pub fn instantiate(
        &self,
        kind: &NodeKindKey,
        kind_version: u32,
        origin: CanvasPoint,
    ) -> Result<NodeCreation, NativeProductCreationError> {
        let Some(entry) = self.product.get(kind, kind_version) else {
            return Err(NativeProductCreationError::Unavailable {
                kind: kind.clone(),
                kind_version,
            });
        };
        self.semantic
            .instantiate_node(
                kind,
                CanvasRect {
                    origin,
                    size: entry.initial_size,
                },
            )
            .map_err(NativeProductCreationError::Semantic)
    }
}

fn composition_issue(
    entry: &NativeProductNodePresentation,
    issue: NativeProductCompositionIssueKind,
) -> NativeProductCompositionIssue {
    NativeProductCompositionIssue {
        kind: entry.kind.clone(),
        kind_version: entry.kind_version,
        issue,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeProductCreationError {
    #[error("node kind `{kind:?}` version {kind_version} is not creatable in this product")]
    Unavailable {
        kind: NodeKindKey,
        kind_version: u32,
    },
    #[error(transparent)]
    Semantic(#[from] NodeCatalogInstantiationError),
}

#[derive(Debug, Clone)]
pub struct NativeProductCandidate {
    authoritative_index: usize,
    kind: NodeKindKey,
    kind_version: u32,
    port: PortKey,
    title: Arc<str>,
    category: Arc<[Arc<str>]>,
    keywords: Arc<[Arc<str>]>,
}

impl NativeProductCandidate {
    #[must_use]
    pub const fn authoritative_index(&self) -> usize {
        self.authoritative_index
    }

    #[must_use]
    pub fn kind(&self) -> &NodeKindKey {
        &self.kind
    }

    #[must_use]
    pub const fn kind_version(&self) -> u32 {
        self.kind_version
    }

    #[must_use]
    pub fn port(&self) -> &PortKey {
        &self.port
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn category(&self) -> &[Arc<str>] {
        &self.category
    }

    #[must_use]
    pub fn keywords(&self) -> &[Arc<str>] {
        &self.keywords
    }
}

pub fn native_product_catalog() -> Result<NativeProductCatalog, NativeProductCatalogError> {
    NativeProductCatalog::from_entries([
        NativeProductNodePresentation::new(
            SHADER_MULTIPLY_KIND,
            1,
            "Multiply",
            ["Shader", "Math"],
            ["multiply", "float", "vector"],
            SHADER_MULTIPLY_DEFAULT_SIZE,
            [
                product_port("a", "A", HandlePosition::Left, 0, "inputs"),
                product_port("b", "B", HandlePosition::Left, 1, "inputs"),
                product_port("result", "Result", HandlePosition::Right, 0, "outputs"),
            ],
        )?,
        NativeProductNodePresentation::new(
            BLUEPRINT_CALL_KIND,
            1,
            "Function Call",
            ["Blueprint", "Functions"],
            ["call", "function", "dynamic input"],
            BLUEPRINT_CALL_DEFAULT_SIZE,
            [
                product_port("exec_in", "In", HandlePosition::Left, 0, "exec"),
                product_port("value", "Value", HandlePosition::Left, 0, "data"),
                product_port("exec_out", "Out", HandlePosition::Right, 0, "exec"),
            ],
        )?,
        NativeProductNodePresentation::new(
            WORKFLOW_LLM_KIND,
            1,
            "LLM",
            ["Workflow", "AI"],
            ["llm", "model", "prompt", "generate"],
            WORKFLOW_LLM_DEFAULT_SIZE,
            [
                product_port("input", "Input", HandlePosition::Left, 0, "exec"),
                product_port("success", "Success", HandlePosition::Right, 0, "exec"),
            ],
        )?,
        NativeProductNodePresentation::new(
            WORKFLOW_CONDITION_KIND,
            1,
            "Condition",
            ["Workflow", "Logic"],
            ["condition", "case", "route", "if"],
            WORKFLOW_CONDITION_DEFAULT_SIZE,
            [
                product_port("input", "Input", HandlePosition::Left, 0, "exec"),
                product_port("else", "Else", HandlePosition::Right, 0, "branches"),
            ],
        )?,
    ])
}

fn product_port(
    key: &'static str,
    label: &'static str,
    side: HandlePosition,
    order: i32,
    group: &'static str,
) -> NativeProductPortPresentation {
    NativeProductPortPresentation::new(key, label, side, order, Some(group))
}

const fn side_key(side: HandlePosition) -> u8 {
    match side {
        HandlePosition::Top => 0,
        HandlePosition::Right => 1,
        HandlePosition::Bottom => 2,
        HandlePosition::Left => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jellyflow_core::{
        CanvasPoint, CanvasSize, DefaultTypeCompatibility, NodeGraphConnectionMode, NodeId,
        NodeKindKey, Port, PortCapacity, PortDirection, PortKey, PortKind,
    };
    use jellyflow_open_gpui::registry::{
        OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
    };
    use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
    use jellyflow_runtime::view::RendererStamp;
    use open_gpui::{AppContext, Context, Render, Window, div};

    use super::{
        NativeProductCatalog, NativeProductCatalogError, NativeProductComposition,
        NativeProductCompositionIssueKind, NativeProductCreationError,
        NativeProductNodePresentation, NativeProductPortPresentation, native_product_catalog,
    };
    use crate::nodes::{NativeNodeResources, native_node_catalog, native_node_registry};
    use jellyflow_runtime::geometry::HandlePosition;

    const TEST_SIZE: CanvasSize = CanvasSize {
        width: 240.0,
        height: 144.0,
    };

    struct TestRenderer;

    impl Render for TestRenderer {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> impl open_gpui::IntoElement {
            div()
        }
    }

    fn semantic_catalog(entries: impl IntoIterator<Item = NodeKindSpec>) -> NodeCatalog {
        let mut catalog = NodeCatalog::new();
        for entry in entries {
            catalog
                .register(entry)
                .expect("test semantic kinds must be unique and valid");
        }
        catalog
    }

    fn semantic_port(key: &str) -> NodePortSpec {
        NodePortSpec::new(key, PortDirection::In, PortKind::Data, PortCapacity::Single)
    }

    fn product_port(key: &str, order: i32) -> NativeProductPortPresentation {
        NativeProductPortPresentation::new(
            key,
            format!("{key} label"),
            HandlePosition::Left,
            order,
            Some("inputs"),
        )
    }

    fn product_node(
        kind: &str,
        kind_version: u32,
        initial_size: CanvasSize,
        ports: impl IntoIterator<Item = NativeProductPortPresentation>,
    ) -> NativeProductNodePresentation {
        NativeProductNodePresentation::new(
            kind,
            kind_version,
            format!("{kind} title"),
            ["Test", "Nodes"],
            ["searchable", "candidate"],
            initial_size,
            ports,
        )
        .expect("valid test product presentation")
    }

    fn product_catalog_with(
        entries: impl IntoIterator<Item = NativeProductNodePresentation>,
    ) -> NativeProductCatalog {
        NativeProductCatalog::from_entries(entries).expect("unique test product entries")
    }

    fn registry_for(
        catalog: &NodeCatalog,
        entries: impl IntoIterator<Item = (&'static str, u32)>,
    ) -> OpenGpuiNodeRegistry {
        let mut registry = OpenGpuiNodeRegistry::builder();
        for (kind, version) in entries {
            registry
                .register(OpenGpuiNodeRendererDescriptor::new(
                    kind,
                    version,
                    RendererStamp::new(format!("test.renderer.{kind}"), 1),
                    OpenGpuiNodeAdapterPresentation::new(format!("{kind} renderer")),
                    |_binding, _window, cx| Ok(cx.new(|_| TestRenderer)),
                ))
                .expect("unique valid test renderer");
        }
        registry
            .seal(catalog)
            .expect("test registry must match its semantic catalog")
    }

    fn empty_registry(catalog: &NodeCatalog) -> OpenGpuiNodeRegistry {
        OpenGpuiNodeRegistry::builder()
            .seal(catalog)
            .expect("an empty registry is valid")
    }

    #[test]
    fn product_catalog_rejects_invalid_local_presentation() {
        let invalid_size = NativeProductNodePresentation::new(
            "test.invalid-size",
            1,
            "Invalid size",
            ["Test"],
            ["invalid"],
            CanvasSize {
                width: 0.0,
                height: 10.0,
            },
            [],
        )
        .expect_err("zero-width product nodes are invalid");
        assert!(matches!(
            invalid_size,
            NativeProductCatalogError::InvalidInitialSize { .. }
        ));

        let duplicate_port = NativeProductNodePresentation::new(
            "test.duplicate-port",
            1,
            "Duplicate port",
            ["Test"],
            ["duplicate"],
            TEST_SIZE,
            [product_port("input", 0), product_port("input", 1)],
        )
        .expect_err("static port keys must be unique");
        assert!(matches!(
            duplicate_port,
            NativeProductCatalogError::DuplicatePortPresentation { .. }
        ));

        let duplicate_order = NativeProductNodePresentation::new(
            "test.duplicate-order",
            1,
            "Duplicate order",
            ["Test"],
            ["duplicate"],
            TEST_SIZE,
            [product_port("first", 0), product_port("second", 0)],
        )
        .expect_err("one side and group cannot reuse a layout order");
        assert!(matches!(
            duplicate_order,
            NativeProductCatalogError::DuplicatePortOrder { .. }
        ));

        let entry = product_node("test.duplicate-kind", 1, TEST_SIZE, []);
        let duplicate_kind = NativeProductCatalog::from_entries([entry.clone(), entry])
            .expect_err("product node kinds must be unique");
        assert!(matches!(
            duplicate_kind,
            NativeProductCatalogError::DuplicateKind { .. }
        ));
    }

    #[open_gpui::test]
    fn built_in_semantic_product_and_renderer_catalogs_compose(cx: &mut open_gpui::TestAppContext) {
        let semantic = Arc::new(native_node_catalog().expect("valid built-in semantic catalog"));
        let product = native_product_catalog().expect("valid built-in product catalog");
        let registry = cx.update(|cx| {
            native_node_registry(
                NativeNodeResources::ready_default(cx).with_product_catalog(product.clone()),
                semantic.as_ref(),
            )
        });

        let composition =
            NativeProductComposition::compose(Arc::clone(&semantic), product, &registry)
                .expect("built-in semantic, product, and renderer catalogs must agree");

        assert_eq!(composition.product().entries().count(), 4);
        assert!(composition.product().entries().all(|entry| {
            semantic.get(entry.kind()).is_some()
                && registry
                    .renderer_stamp(entry.kind(), entry.kind_version())
                    .is_some()
        }));
    }

    #[test]
    fn composition_rejects_missing_and_noncanonical_semantic_kinds() {
        let empty_semantic = Arc::new(NodeCatalog::new());
        let missing_product =
            product_catalog_with([product_node("test.missing", 1, TEST_SIZE, [])]);
        let missing = NativeProductComposition::compose(
            Arc::clone(&empty_semantic),
            missing_product,
            &empty_registry(&empty_semantic),
        )
        .expect_err("a product entry needs a semantic kind");
        assert_eq!(missing.issues().len(), 1);
        assert!(matches!(
            missing.issues()[0].issue(),
            NativeProductCompositionIssueKind::MissingSemanticKind
        ));

        let canonical = "test.canonical";
        let alias = "test.legacy";
        let semantic = Arc::new(semantic_catalog([NodeKindSpec::builder(canonical)
            .alias(alias)
            .build()]));
        let aliased_product = product_catalog_with([product_node(alias, 1, TEST_SIZE, [])]);
        let noncanonical = NativeProductComposition::compose(
            Arc::clone(&semantic),
            aliased_product,
            &empty_registry(&semantic),
        )
        .expect_err("product identity must use the canonical semantic kind");
        assert_eq!(noncanonical.issues().len(), 1);
        assert!(matches!(
            noncanonical.issues()[0].issue(),
            NativeProductCompositionIssueKind::NonCanonicalSemanticKind { canonical: found }
                if found == &NodeKindKey::new(canonical)
        ));
    }

    #[test]
    fn composition_rejects_version_and_renderer_drift() {
        const KIND: &str = "test.versioned";
        let semantic = Arc::new(semantic_catalog([NodeKindSpec::builder(KIND)
            .latest_kind_version(2)
            .build()]));
        let product = product_catalog_with([product_node(KIND, 1, TEST_SIZE, [])]);
        let registry = registry_for(&semantic, [(KIND, 2)]);
        let drift = NativeProductComposition::compose(Arc::clone(&semantic), product, &registry)
            .expect_err("product version must join the exact semantic renderer version");
        assert_eq!(drift.issues().len(), 2);
        assert!(drift.issues().iter().any(|issue| matches!(
            issue.issue(),
            NativeProductCompositionIssueKind::SemanticVersionMismatch {
                semantic_version: 2
            }
        )));
        assert!(drift.issues().iter().any(|issue| matches!(
            issue.issue(),
            NativeProductCompositionIssueKind::MissingRenderer
        )));

        let semantic = Arc::new(semantic_catalog([NodeKindSpec::builder(KIND).build()]));
        let product = product_catalog_with([product_node(KIND, 1, TEST_SIZE, [])]);
        let missing_renderer = NativeProductComposition::compose(
            Arc::clone(&semantic),
            product,
            &empty_registry(&semantic),
        )
        .expect_err("every product entry needs an exact native renderer");
        assert_eq!(missing_renderer.issues().len(), 1);
        assert!(matches!(
            missing_renderer.issues()[0].issue(),
            NativeProductCompositionIssueKind::MissingRenderer
        ));
    }

    #[test]
    fn composition_reports_missing_and_extra_static_port_presentations() {
        const KIND: &str = "test.port-drift";
        let semantic = Arc::new(semantic_catalog([NodeKindSpec::builder(KIND)
            .port(semantic_port("required"))
            .build()]));
        let product =
            product_catalog_with([product_node(KIND, 1, TEST_SIZE, [product_port("extra", 0)])]);
        let registry = registry_for(&semantic, [(KIND, 1)]);

        let drift = NativeProductComposition::compose(Arc::clone(&semantic), product, &registry)
            .expect_err("static semantic and product port key sets must match");

        assert_eq!(drift.issues().len(), 2);
        assert!(matches!(
            drift.issues()[0].issue(),
            NativeProductCompositionIssueKind::MissingPortPresentation { port }
                if port == &PortKey::new("required")
        ));
        assert!(matches!(
            drift.issues()[1].issue(),
            NativeProductCompositionIssueKind::ExtraPortPresentation { port }
                if port == &PortKey::new("extra")
        ));
    }

    #[test]
    fn composition_allows_persisted_only_semantics_and_renderers() {
        const PRODUCT_KIND: &str = "test.product";
        const PERSISTED_KIND: &str = "test.persisted-only";
        let semantic = Arc::new(semantic_catalog([
            NodeKindSpec::builder(PRODUCT_KIND)
                .port(semantic_port("input"))
                .build(),
            NodeKindSpec::builder(PERSISTED_KIND)
                .port(semantic_port("input"))
                .build(),
        ]));
        let product = product_catalog_with([product_node(
            PRODUCT_KIND,
            1,
            TEST_SIZE,
            [product_port("input", 0)],
        )]);
        let registry = registry_for(&semantic, [(PRODUCT_KIND, 1), (PERSISTED_KIND, 1)]);

        let composition =
            NativeProductComposition::compose(Arc::clone(&semantic), product, &registry).expect(
                "persisted-only semantic and renderer entries do not need palette metadata",
            );

        assert!(
            composition
                .product()
                .get(&NodeKindKey::new(PERSISTED_KIND), 1)
                .is_none()
        );
        assert!(
            registry
                .renderer_stamp(&NodeKindKey::new(PERSISTED_KIND), 1)
                .is_some()
        );
    }

    #[test]
    fn candidate_projection_preserves_authoritative_index_and_product_metadata() {
        const PERSISTED_KIND: &str = "a.persisted-only";
        const PRODUCT_KIND: &str = "b.product";
        let semantic = Arc::new(semantic_catalog([
            NodeKindSpec::builder(PERSISTED_KIND)
                .port(semantic_port("input"))
                .build(),
            NodeKindSpec::builder(PRODUCT_KIND)
                .latest_kind_version(3)
                .port(semantic_port("input"))
                .build(),
        ]));
        let product = product_catalog_with([NativeProductNodePresentation::new(
            PRODUCT_KIND,
            3,
            "Search result title",
            ["Workflow", "Search"],
            ["needle", "creation"],
            TEST_SIZE,
            [product_port("input", 0)],
        )
        .expect("valid searchable product entry")]);
        let registry = registry_for(&semantic, [(PERSISTED_KIND, 1), (PRODUCT_KIND, 3)]);
        let composition =
            NativeProductComposition::compose(Arc::clone(&semantic), product, &registry)
                .expect("valid searchable product composition");
        let source = Port {
            node: NodeId::from_u128(1),
            key: PortKey::new("source"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        };
        let candidates = semantic.create_from_wire_candidates(
            &source,
            NodeGraphConnectionMode::Strict,
            &DefaultTypeCompatibility,
        );
        assert_eq!(candidates.len(), 2);

        let projected = composition.project_candidates(&candidates);

        assert_eq!(projected.len(), 1);
        let candidate = &projected[0];
        assert_eq!(candidate.authoritative_index(), 1);
        assert_eq!(candidate.kind(), &NodeKindKey::new(PRODUCT_KIND));
        assert_eq!(candidate.kind_version(), 3);
        assert_eq!(candidate.port(), &PortKey::new("input"));
        assert_eq!(candidate.title(), "Search result title");
        assert_eq!(
            candidate
                .category()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<&str>>(),
            ["Workflow", "Search"]
        );
        assert_eq!(
            candidate
                .keywords()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<&str>>(),
            ["needle", "creation"]
        );
    }

    #[test]
    fn product_creation_uses_explicit_product_size_and_exact_version() {
        const KIND: &str = "test.sized";
        const SIZE: CanvasSize = CanvasSize {
            width: 321.0,
            height: 123.0,
        };
        let semantic = Arc::new(semantic_catalog([NodeKindSpec::builder(KIND)
            .latest_kind_version(7)
            .build()]));
        let product = product_catalog_with([product_node(KIND, 7, SIZE, [])]);
        let registry = registry_for(&semantic, [(KIND, 7)]);
        let composition =
            NativeProductComposition::compose(Arc::clone(&semantic), product, &registry)
                .expect("valid explicitly sized product composition");
        let origin = CanvasPoint { x: 42.0, y: -9.0 };

        let creation = composition
            .instantiate(&NodeKindKey::new(KIND), 7, origin)
            .expect("product kind is creatable");

        assert_eq!(creation.node().kind, NodeKindKey::new(KIND));
        assert_eq!(creation.node().kind_version, 7);
        assert_eq!(creation.node().pos, origin);
        assert_eq!(creation.node().size, Some(SIZE));
        assert!(matches!(
            composition.instantiate(&NodeKindKey::new(KIND), 6, origin),
            Err(NativeProductCreationError::Unavailable {
                kind,
                kind_version: 6
            }) if kind == NodeKindKey::new(KIND)
        ));
    }
}
