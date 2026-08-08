use std::{
    collections::BTreeMap,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use jellyflow_core::{CanvasRect, NodeKindKey, PortId};
use jellyflow_runtime::{
    graph::GraphRevision,
    schema::NodeCatalog,
    view::{FrameNode, NodeInstanceKey, RendererStamp},
};
use open_gpui::{AnyView, App, Entity, Render, Window};

use crate::binding::OpenGpuiNodeBinding;
use crate::panic_sink::OpenGpuiPanicAccumulator;
use crate::port_handle::OpenGpuiPortHandle;
use crate::scale_policy::OpenGpuiNodeScalePolicy;
use crate::size_policy::OpenGpuiNodeSizePolicy;

type ErasedNodeFactory = dyn Fn(
        Entity<OpenGpuiNodeBinding>,
        &mut Window,
        &mut App,
    ) -> Result<AnyView, OpenGpuiNodeMountError>
    + 'static;

type ErasedOverviewFactory = dyn Fn(OpenGpuiNodeOverviewMount, &mut Window, &mut App) -> Result<AnyView, OpenGpuiNodeMountError>
    + 'static;

/// Immutable frame facts captured when one exact native node mount is created.
///
/// These facts are intentionally small so parked mounts cannot retain historical graph snapshots.
/// Applications read changing node data, ports, selection, and viewport facts from the shared
/// [`OpenGpuiNodeBinding`] instead of treating this mount-time value as refreshable state.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiNodeMountFacts {
    instance: NodeInstanceKey,
    binding_revision: GraphRevision,
    layout_generation: u64,
    bounds: CanvasRect,
}

impl OpenGpuiNodeMountFacts {
    fn from_frame(frame: &FrameNode) -> Self {
        Self {
            instance: frame.instance().clone(),
            binding_revision: frame.binding_revision(),
            layout_generation: frame.layout_generation(),
            bounds: frame.bounds(),
        }
    }

    /// Returns the exact view, renderer, node, and node-incarnation identity of this mount.
    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    /// Returns the node-relevant graph revision captured when the mount was created.
    #[must_use]
    pub const fn binding_revision(&self) -> GraphRevision {
        self.binding_revision
    }

    /// Returns the runtime layout generation captured when the mount was created.
    #[must_use]
    pub const fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    /// Returns the authoritative canvas bounds captured when the mount was created.
    #[must_use]
    pub const fn bounds(&self) -> CanvasRect {
        self.bounds
    }
}

/// Native capabilities supplied to an application-owned semantic-LOD overview factory.
///
/// The detail and overview factories receive the same binding entity and therefore share one mount
/// capability, [`NodeInstanceKey`], presentation snapshot, and real-port capability set. Call
/// [`Self::port_handle`] during render instead of retaining a returned handle so dynamic topology
/// and presentation state remain current.
#[derive(Debug, Clone)]
pub struct OpenGpuiNodeOverviewMount {
    binding: Entity<OpenGpuiNodeBinding>,
    facts: OpenGpuiNodeMountFacts,
}

impl OpenGpuiNodeOverviewMount {
    fn new(binding: Entity<OpenGpuiNodeBinding>, frame: &FrameNode) -> Self {
        Self {
            binding,
            facts: OpenGpuiNodeMountFacts::from_frame(frame),
        }
    }

    /// Returns the same retained binding entity supplied to the detail factory.
    #[must_use]
    pub fn binding(&self) -> Entity<OpenGpuiNodeBinding> {
        self.binding.clone()
    }

    /// Returns immutable facts captured for this exact native mount.
    #[must_use]
    pub const fn facts(&self) -> &OpenGpuiNodeMountFacts {
        &self.facts
    }

    /// Returns the current overview-presentation handle for one real graph port.
    ///
    /// The handle shares its semantic identity and mount authority with the corresponding detail
    /// handle, but publishes geometry for the overview presentation. A removed or reincarnated port
    /// returns `None`; callers should request the handle again on each render pass.
    #[must_use]
    pub fn port_handle(&self, port: PortId, cx: &App) -> Option<OpenGpuiPortHandle> {
        self.binding
            .read(cx)
            .cached_port_handle(port)
            .map(OpenGpuiPortHandle::for_host_overview)
    }
}

/// A renderer-factory failure that the host can present through its diagnostic fallback.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("node renderer factory failed: {message}")]
pub struct OpenGpuiNodeMountError {
    message: String,
}

impl OpenGpuiNodeMountError {
    /// Creates a renderer-factory failure with an application-facing diagnostic message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the diagnostic message without the error's contextual prefix.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SemanticRendererKey {
    kind: NodeKindKey,
    kind_version: u32,
}

/// Minimal adapter-owned presentation required around an application-native node view.
///
/// Product discovery metadata and graph-port layout hints intentionally live outside this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiNodeAdapterPresentation {
    accessible_name: Arc<str>,
}

impl OpenGpuiNodeAdapterPresentation {
    /// Creates the presentation used by host accessibility and generic overview chrome.
    #[must_use]
    pub fn new(accessible_name: impl Into<Arc<str>>) -> Self {
        Self {
            accessible_name: accessible_name.into(),
        }
    }

    /// Returns the application-facing accessible node name.
    #[must_use]
    pub fn accessible_name(&self) -> &str {
        &self.accessible_name
    }
}

/// One complete semantic-to-native renderer registration.
///
/// Factories return a typed mount result so recoverable application failures remain explicit. The
/// descriptor is the only registration input; policy and overview configuration cannot drift into
/// separate builder paths.
pub struct OpenGpuiNodeRendererDescriptor {
    key: SemanticRendererKey,
    renderer: RendererStamp,
    presentation: OpenGpuiNodeAdapterPresentation,
    factory: Box<ErasedNodeFactory>,
    overview_factory: Option<Box<ErasedOverviewFactory>>,
    policy: OpenGpuiNodeRegistrationPolicy,
}

impl OpenGpuiNodeRendererDescriptor {
    /// Creates one complete fallible native renderer descriptor.
    #[must_use]
    pub fn new<K, F, V>(
        kind: K,
        kind_version: u32,
        renderer: RendererStamp,
        presentation: OpenGpuiNodeAdapterPresentation,
        factory: F,
    ) -> Self
    where
        K: Into<NodeKindKey>,
        F: Fn(
                Entity<OpenGpuiNodeBinding>,
                &mut Window,
                &mut App,
            ) -> Result<Entity<V>, OpenGpuiNodeMountError>
            + 'static,
        V: Render + 'static,
    {
        Self {
            key: SemanticRendererKey {
                kind: kind.into(),
                kind_version,
            },
            renderer,
            presentation,
            factory: Box::new(move |binding, window, cx| {
                factory(binding, window, cx).map(AnyView::from)
            }),
            overview_factory: None,
            policy: OpenGpuiNodeRegistrationPolicy::default(),
        }
    }

    /// Replaces the host scale and size policy for this native renderer.
    #[must_use]
    pub fn with_policy(mut self, policy: OpenGpuiNodeRegistrationPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Installs an application-native semantic-LOD overview factory.
    #[must_use]
    pub fn with_overview<O, V>(mut self, factory: O) -> Self
    where
        O: Fn(
                OpenGpuiNodeOverviewMount,
                &mut Window,
                &mut App,
            ) -> Result<Entity<V>, OpenGpuiNodeMountError>
            + 'static,
        V: Render + 'static,
    {
        self.overview_factory = Some(Box::new(move |mount, window, cx| {
            factory(mount, window, cx).map(AnyView::from)
        }));
        self
    }

    /// Returns the canonical semantic kind supplied by the application.
    #[must_use]
    pub fn kind(&self) -> &NodeKindKey {
        &self.key.kind
    }

    /// Returns the exact semantic kind version supplied by the application.
    #[must_use]
    pub const fn kind_version(&self) -> u32 {
        self.key.kind_version
    }

    /// Returns the opaque native renderer identity.
    #[must_use]
    pub const fn renderer_stamp(&self) -> &RendererStamp {
        &self.renderer
    }

    /// Returns the adapter-owned presentation paired with this renderer.
    #[must_use]
    pub const fn presentation(&self) -> &OpenGpuiNodeAdapterPresentation {
        &self.presentation
    }

    /// Returns the host scale and size policy paired with this renderer.
    #[must_use]
    pub const fn policy(&self) -> OpenGpuiNodeRegistrationPolicy {
        self.policy
    }

    /// Returns whether this renderer supplies an application-native overview.
    #[must_use]
    pub const fn has_overview(&self) -> bool {
        self.overview_factory.is_some()
    }
}

/// A descriptor rejected before it can mutate the registry builder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenGpuiNodeRegistryRegistrationError {
    /// The semantic kind is empty or whitespace-only.
    #[error("renderer semantic kind must not be empty")]
    EmptyKind,
    /// The semantic kind version is zero.
    #[error("renderer semantic kind {kind:?} must declare a positive version")]
    InvalidKindVersion { kind: NodeKindKey },
    /// The renderer stamp key is empty or whitespace-only.
    #[error("renderer stamp for {kind:?}@{kind_version} must not have an empty key")]
    EmptyRendererKey {
        kind: NodeKindKey,
        kind_version: u32,
    },
    /// The adapter presentation has no accessible name.
    #[error("renderer presentation for {kind:?}@{kind_version} must have an accessible name")]
    EmptyAccessibleName {
        kind: NodeKindKey,
        kind_version: u32,
    },
    /// The builder already contains this semantic kind and version.
    #[error("renderer semantic kind {kind:?}@{kind_version} is already registered")]
    DuplicateKindVersion {
        kind: NodeKindKey,
        kind_version: u32,
    },
    /// The builder already maps this complete renderer stamp to another semantic kind.
    #[error(
        "renderer stamp {renderer:?} is already registered for {registered_kind:?}@{registered_version}"
    )]
    DuplicateRendererStamp {
        renderer: RendererStamp,
        registered_kind: NodeKindKey,
        registered_version: u32,
    },
}

/// A semantic-catalog mismatch found while sealing the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenGpuiNodeRegistryIssueKind {
    /// The descriptor kind is not registered in the semantic catalog.
    MissingSemanticKind,
    /// The descriptor used a catalog alias instead of its canonical kind.
    NonCanonicalSemanticKind { canonical: NodeKindKey },
    /// The descriptor version does not equal the catalog's latest version.
    SemanticVersionMismatch { catalog_version: u32 },
}

/// A deterministic registry configuration diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiNodeRegistryIssue {
    kind: NodeKindKey,
    kind_version: u32,
    issue: OpenGpuiNodeRegistryIssueKind,
}

impl OpenGpuiNodeRegistryIssue {
    /// Returns the descriptor's semantic kind.
    #[must_use]
    pub fn semantic_kind(&self) -> &NodeKindKey {
        &self.kind
    }

    /// Returns the descriptor's semantic kind version.
    #[must_use]
    pub const fn semantic_version(&self) -> u32 {
        self.kind_version
    }

    /// Returns the catalog mismatch for this descriptor.
    #[must_use]
    pub const fn issue(&self) -> &OpenGpuiNodeRegistryIssueKind {
        &self.issue
    }
}

impl fmt::Display for OpenGpuiNodeRegistryIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.issue {
            OpenGpuiNodeRegistryIssueKind::MissingSemanticKind => write!(
                formatter,
                "renderer references missing semantic kind {:?}@{}",
                self.kind, self.kind_version
            ),
            OpenGpuiNodeRegistryIssueKind::NonCanonicalSemanticKind { canonical } => write!(
                formatter,
                "renderer semantic kind {:?}@{} is an alias for canonical kind {canonical:?}",
                self.kind, self.kind_version
            ),
            OpenGpuiNodeRegistryIssueKind::SemanticVersionMismatch { catalog_version } => write!(
                formatter,
                "renderer semantic kind {:?}@{} does not match catalog version {catalog_version}",
                self.kind, self.kind_version
            ),
        }
    }
}

/// Aggregate configuration error returned when a node registry cannot be sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiNodeRegistryConfigError {
    issues: Vec<OpenGpuiNodeRegistryIssue>,
}

impl OpenGpuiNodeRegistryConfigError {
    /// Returns every mismatch in deterministic semantic-key order.
    #[must_use]
    pub fn issues(&self) -> &[OpenGpuiNodeRegistryIssue] {
        &self.issues
    }
}

impl fmt::Display for OpenGpuiNodeRegistryConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Open GPUI node registry configuration has {} issue(s)",
            self.issues.len()
        )?;
        for issue in &self.issues {
            write!(formatter, "; {issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for OpenGpuiNodeRegistryConfigError {}

/// Host policy attached to one native renderer registration.
///
/// This is graph-adapter metadata, not a widget DSL. The registered entity remains an arbitrary
/// Open GPUI component tree and owns its native controls, layout, and paint.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OpenGpuiNodeRegistrationPolicy {
    scale: OpenGpuiNodeScalePolicy,
    size: OpenGpuiNodeSizePolicy,
}

impl OpenGpuiNodeRegistrationPolicy {
    /// Replaces the adapter's level-of-detail authority for this renderer.
    #[must_use]
    pub const fn with_scale_policy(mut self, scale: OpenGpuiNodeScalePolicy) -> Self {
        self.scale = scale;
        self
    }

    /// Replaces the graph-document size authority for this renderer.
    #[must_use]
    pub const fn with_size_policy(mut self, size: OpenGpuiNodeSizePolicy) -> Self {
        self.size = size;
        self
    }

    /// Returns the registered level-of-detail authority.
    #[must_use]
    pub const fn scale_policy(self) -> OpenGpuiNodeScalePolicy {
        self.scale
    }

    /// Returns the registered node-size authority.
    #[must_use]
    pub const fn size_policy(self) -> OpenGpuiNodeSizePolicy {
        self.size
    }
}

/// Mutable construction phase for a host-scoped Open GPUI node registry.
#[derive(Default)]
pub struct OpenGpuiNodeRegistryBuilder {
    registrations: BTreeMap<SemanticRendererKey, OpenGpuiNodeRendererDescriptor>,
    stamps: BTreeMap<RendererStamp, SemanticRendererKey>,
}

impl OpenGpuiNodeRegistryBuilder {
    /// Registers one complete descriptor without mutating the builder on validation failure.
    ///
    /// # Errors
    ///
    /// Returns a typed validation or duplicate-identity error before inserting any descriptor.
    pub fn register(
        &mut self,
        descriptor: OpenGpuiNodeRendererDescriptor,
    ) -> Result<(), OpenGpuiNodeRegistryRegistrationError> {
        if descriptor.key.kind.0.trim().is_empty() {
            return Err(OpenGpuiNodeRegistryRegistrationError::EmptyKind);
        }
        if descriptor.key.kind_version == 0 {
            return Err(OpenGpuiNodeRegistryRegistrationError::InvalidKindVersion {
                kind: descriptor.key.kind.clone(),
            });
        }
        if descriptor.renderer.key().trim().is_empty() {
            return Err(OpenGpuiNodeRegistryRegistrationError::EmptyRendererKey {
                kind: descriptor.key.kind.clone(),
                kind_version: descriptor.key.kind_version,
            });
        }
        if descriptor.presentation.accessible_name().trim().is_empty() {
            return Err(OpenGpuiNodeRegistryRegistrationError::EmptyAccessibleName {
                kind: descriptor.key.kind.clone(),
                kind_version: descriptor.key.kind_version,
            });
        }
        if self.registrations.contains_key(&descriptor.key) {
            return Err(
                OpenGpuiNodeRegistryRegistrationError::DuplicateKindVersion {
                    kind: descriptor.key.kind.clone(),
                    kind_version: descriptor.key.kind_version,
                },
            );
        }
        if let Some(registered) = self.stamps.get(&descriptor.renderer) {
            return Err(
                OpenGpuiNodeRegistryRegistrationError::DuplicateRendererStamp {
                    renderer: descriptor.renderer.clone(),
                    registered_kind: registered.kind.clone(),
                    registered_version: registered.kind_version,
                },
            );
        }

        self.stamps
            .insert(descriptor.renderer.clone(), descriptor.key.clone());
        self.registrations
            .insert(descriptor.key.clone(), descriptor);
        Ok(())
    }

    /// Validates every descriptor against one immutable semantic catalog and seals the mapping.
    ///
    /// # Errors
    ///
    /// Returns every unknown, aliased, or version-mismatched descriptor in deterministic order.
    pub fn seal(
        self,
        catalog: &NodeCatalog,
    ) -> Result<OpenGpuiNodeRegistry, OpenGpuiNodeRegistryConfigError> {
        let mut issues = Vec::new();
        for key in self.registrations.keys() {
            let issue = match catalog.resolve_kind(&key.kind) {
                None => Some(OpenGpuiNodeRegistryIssueKind::MissingSemanticKind),
                Some(canonical) if canonical != &key.kind => {
                    Some(OpenGpuiNodeRegistryIssueKind::NonCanonicalSemanticKind {
                        canonical: canonical.clone(),
                    })
                }
                Some(_) => catalog
                    .get(&key.kind)
                    .filter(|spec| spec.latest_kind_version() != key.kind_version)
                    .map(
                        |spec| OpenGpuiNodeRegistryIssueKind::SemanticVersionMismatch {
                            catalog_version: spec.latest_kind_version(),
                        },
                    ),
            };
            if let Some(issue) = issue {
                issues.push(OpenGpuiNodeRegistryIssue {
                    kind: key.kind.clone(),
                    kind_version: key.kind_version,
                    issue,
                });
            }
        }
        if !issues.is_empty() {
            return Err(OpenGpuiNodeRegistryConfigError { issues });
        }
        Ok(OpenGpuiNodeRegistry {
            renderers: self.registrations,
        })
    }
}

/// A sealed, host-scoped registry of retained Open GPUI node factories.
///
/// Sealing consumes the mutable builder, and the sealed registry exposes no registration method.
///
/// ```compile_fail
/// use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
///
/// fn mutate_after_seal(registry: &mut OpenGpuiNodeRegistry) {
///     registry.register(());
/// }
/// ```
pub struct OpenGpuiNodeRegistry {
    renderers: BTreeMap<SemanticRendererKey, OpenGpuiNodeRendererDescriptor>,
}

impl OpenGpuiNodeRegistry {
    /// Starts construction of a host-scoped renderer registry.
    #[must_use]
    pub fn builder() -> OpenGpuiNodeRegistryBuilder {
        OpenGpuiNodeRegistryBuilder::default()
    }

    /// Returns the number of valid renderer factories.
    #[must_use]
    pub fn len(&self) -> usize {
        self.renderers.len()
    }

    /// Returns whether the registry has no valid renderer factories.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.renderers.is_empty()
    }

    /// Resolves the opaque native renderer identity for one exact semantic kind and version.
    #[must_use]
    pub fn renderer_stamp(&self, kind: &NodeKindKey, kind_version: u32) -> Option<&RendererStamp> {
        self.renderers
            .get(&SemanticRendererKey {
                kind: kind.clone(),
                kind_version,
            })
            .map(|descriptor| &descriptor.renderer)
    }

    /// Resolves adapter presentation for one exact canonical semantic kind and version.
    #[must_use]
    pub fn presentation(
        &self,
        kind: &NodeKindKey,
        kind_version: u32,
    ) -> Option<&OpenGpuiNodeAdapterPresentation> {
        self.renderers
            .get(&SemanticRendererKey {
                kind: kind.clone(),
                kind_version,
            })
            .map(|descriptor| &descriptor.presentation)
    }

    pub(crate) fn mappings(&self) -> impl Iterator<Item = (&NodeKindKey, u32, &RendererStamp)> {
        self.renderers.values().map(|descriptor| {
            (
                &descriptor.key.kind,
                descriptor.key.kind_version,
                &descriptor.renderer,
            )
        })
    }

    pub(crate) fn mount(
        &self,
        frame: &FrameNode,
        binding: Entity<OpenGpuiNodeBinding>,
        window: &mut Window,
        cx: &mut App,
    ) -> OpenGpuiNodeMountOutcome {
        let Some(node) = frame.node() else {
            return OpenGpuiNodeMountOutcome::Missing;
        };
        let Some(renderer) = self.renderers.get(&SemanticRendererKey {
            kind: node.kind.clone(),
            kind_version: node.kind_version,
        }) else {
            return OpenGpuiNodeMountOutcome::Missing;
        };
        if &renderer.renderer != frame.renderer() {
            return OpenGpuiNodeMountOutcome::Missing;
        }

        match (renderer.factory)(binding.clone(), window, cx) {
            Ok(view) => {
                let overview = self.mount_overview(renderer, frame, binding, window, cx);
                OpenGpuiNodeMountOutcome::Mounted {
                    view,
                    overview,
                    policy: renderer.policy,
                }
            }
            Err(error) => OpenGpuiNodeMountOutcome::FactoryFailed(error),
        }
    }

    fn mount_overview(
        &self,
        renderer: &OpenGpuiNodeRendererDescriptor,
        frame: &FrameNode,
        binding: Entity<OpenGpuiNodeBinding>,
        window: &mut Window,
        cx: &mut App,
    ) -> OpenGpuiNodeOverviewMountOutcome {
        if renderer.policy.scale_policy() == OpenGpuiNodeScalePolicy::ComponentManaged {
            return OpenGpuiNodeOverviewMountOutcome::Generic;
        }
        let Some(factory) = renderer.overview_factory.as_ref() else {
            return OpenGpuiNodeOverviewMountOutcome::Generic;
        };
        let mount = OpenGpuiNodeOverviewMount::new(binding, frame);
        match catch_unwind(AssertUnwindSafe(|| (factory)(mount, window, cx))) {
            Ok(Ok(view)) => OpenGpuiNodeOverviewMountOutcome::Mounted(view),
            Ok(Err(error)) => OpenGpuiNodeOverviewMountOutcome::FactoryFailed(error),
            Err(payload) => {
                let mut panics = OpenGpuiPanicAccumulator::default();
                panics.push(Some(payload));
                let recursive_payload = panics.suppress();
                let message = if recursive_payload {
                    "application-native overview factory panicked; retained one bounded recursive panic payload sentinel"
                } else {
                    "application-native overview factory panicked"
                };
                OpenGpuiNodeOverviewMountOutcome::FactoryFailed(OpenGpuiNodeMountError::new(
                    message,
                ))
            }
        }
    }
}

pub(crate) enum OpenGpuiNodeMountOutcome {
    Mounted {
        view: AnyView,
        overview: OpenGpuiNodeOverviewMountOutcome,
        policy: OpenGpuiNodeRegistrationPolicy,
    },
    Missing,
    FactoryFailed(OpenGpuiNodeMountError),
}

pub(crate) enum OpenGpuiNodeOverviewMountOutcome {
    Generic,
    Mounted(AnyView),
    FactoryFailed(OpenGpuiNodeMountError),
}
