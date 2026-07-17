use std::{collections::BTreeMap, fmt};

use open_gpui::{AnyView, App, Entity, Render, Window};

use crate::binding::OpenGpuiNodeBinding;

type ErasedNodeFactory = dyn Fn(
        Entity<OpenGpuiNodeBinding>,
        &mut Window,
        &mut App,
    ) -> Result<AnyView, OpenGpuiNodeMountError>
    + 'static;

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

/// A configuration problem found while sealing an Open GPUI node registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiNodeRegistryIssueKind {
    /// A renderer key contained only whitespace.
    EmptyRendererKey,
    /// More than one factory was registered for the same renderer key.
    DuplicateRendererKey,
}

/// The registration state for a renderer key in a sealed registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiRendererRegistrationStatus {
    /// The key has one retained renderer factory.
    Registered,
    /// The key is present but cannot be mounted because its registration is invalid.
    Invalid(OpenGpuiNodeRegistryIssueKind),
    /// No registration was supplied for the key.
    Missing,
}

/// A deterministic registry configuration diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiNodeRegistryIssue {
    renderer_key: String,
    kind: OpenGpuiNodeRegistryIssueKind,
}

impl OpenGpuiNodeRegistryIssue {
    /// Returns the exact renderer key supplied to the builder.
    #[must_use]
    pub fn renderer_key(&self) -> &str {
        &self.renderer_key
    }

    /// Returns the configuration problem associated with the key.
    #[must_use]
    pub fn kind(&self) -> OpenGpuiNodeRegistryIssueKind {
        self.kind
    }
}

impl fmt::Display for OpenGpuiNodeRegistryIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            OpenGpuiNodeRegistryIssueKind::EmptyRendererKey => write!(
                formatter,
                "renderer key {:?} is empty or contains only whitespace",
                self.renderer_key
            ),
            OpenGpuiNodeRegistryIssueKind::DuplicateRendererKey => write!(
                formatter,
                "renderer key {:?} has duplicate registrations",
                self.renderer_key
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
    /// Returns every configuration issue ordered by renderer key.
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

enum PendingRegistration {
    Factory(Box<ErasedNodeFactory>),
    Invalid(OpenGpuiNodeRegistryIssueKind),
}

/// Mutable construction phase for a host-scoped Open GPUI node registry.
#[derive(Default)]
pub struct OpenGpuiNodeRegistryBuilder {
    registrations: BTreeMap<String, PendingRegistration>,
}

impl OpenGpuiNodeRegistryBuilder {
    /// Registers a retained Open GPUI entity factory for a semantic renderer key.
    ///
    /// Registering the same non-empty key more than once invalidates that key; neither factory
    /// wins and unrelated registrations remain usable after [`Self::seal`].
    pub fn register<K, F, V>(&mut self, renderer_key: K, factory: F)
    where
        K: Into<String>,
        F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
        V: Render + 'static,
    {
        let factory = Box::new(move |binding, window: &mut Window, cx: &mut App| {
            Ok(AnyView::from(factory(binding, window, cx)))
        });
        self.insert(renderer_key.into(), factory);
    }

    /// Registers a retained entity factory that can report a recoverable mount failure.
    ///
    /// A returned error is handled by the host's renderer fallback. Panics are not converted into
    /// mount failures because a panicking factory may have left its Open GPUI context inconsistent.
    pub fn try_register<K, F, V>(&mut self, renderer_key: K, factory: F)
    where
        K: Into<String>,
        F: Fn(
                Entity<OpenGpuiNodeBinding>,
                &mut Window,
                &mut App,
            ) -> Result<Entity<V>, OpenGpuiNodeMountError>
            + 'static,
        V: Render + 'static,
    {
        let factory = Box::new(move |binding, window: &mut Window, cx: &mut App| {
            factory(binding, window, cx).map(AnyView::from)
        });
        self.insert(renderer_key.into(), factory);
    }

    fn insert(&mut self, renderer_key: String, factory: Box<ErasedNodeFactory>) {
        if renderer_key.trim().is_empty() {
            self.registrations.insert(
                renderer_key,
                PendingRegistration::Invalid(OpenGpuiNodeRegistryIssueKind::EmptyRendererKey),
            );
            return;
        }

        match self.registrations.entry(renderer_key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(PendingRegistration::Factory(factory));
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.insert(PendingRegistration::Invalid(
                    OpenGpuiNodeRegistryIssueKind::DuplicateRendererKey,
                ));
            }
        }
    }

    /// Validates and seals the registry, making all renderer registrations immutable.
    ///
    /// # Errors
    ///
    /// Returns every configuration issue ordered by renderer key when any whitespace-only key
    /// prevents sealing. Duplicate non-empty keys remain isolated invalid entries in the sealed
    /// registry; when sealing also fails, they are included in the aggregate diagnostic.
    pub fn seal(self) -> Result<OpenGpuiNodeRegistry, OpenGpuiNodeRegistryConfigError> {
        let mut factories = BTreeMap::new();
        let mut invalid = BTreeMap::new();
        let mut issues = Vec::new();
        let mut prevents_seal = false;

        for (renderer_key, registration) in self.registrations {
            match registration {
                PendingRegistration::Factory(factory) => {
                    factories.insert(renderer_key, factory);
                }
                PendingRegistration::Invalid(kind) => {
                    let issue = OpenGpuiNodeRegistryIssue { renderer_key, kind };
                    match kind {
                        OpenGpuiNodeRegistryIssueKind::EmptyRendererKey => prevents_seal = true,
                        OpenGpuiNodeRegistryIssueKind::DuplicateRendererKey => {
                            invalid.insert(issue.renderer_key.clone(), issue.clone());
                        }
                    }
                    issues.push(issue);
                }
            }
        }

        if prevents_seal {
            Err(OpenGpuiNodeRegistryConfigError { issues })
        } else {
            Ok(OpenGpuiNodeRegistry { factories, invalid })
        }
    }
}

/// A sealed, host-scoped registry of retained Open GPUI node factories.
pub struct OpenGpuiNodeRegistry {
    factories: BTreeMap<String, Box<ErasedNodeFactory>>,
    invalid: BTreeMap<String, OpenGpuiNodeRegistryIssue>,
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
        self.factories.len()
    }

    /// Returns whether the registry has no valid renderer factories.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Returns the sealed registration state for an exact renderer key.
    #[must_use]
    pub fn status(&self, renderer_key: &str) -> OpenGpuiRendererRegistrationStatus {
        if self.factories.contains_key(renderer_key) {
            OpenGpuiRendererRegistrationStatus::Registered
        } else if let Some(issue) = self.invalid.get(renderer_key) {
            OpenGpuiRendererRegistrationStatus::Invalid(issue.kind())
        } else {
            OpenGpuiRendererRegistrationStatus::Missing
        }
    }

    pub(crate) fn mount(
        &self,
        renderer_key: &str,
        binding: Entity<OpenGpuiNodeBinding>,
        window: &mut Window,
        cx: &mut App,
    ) -> OpenGpuiNodeMountOutcome {
        if let Some(issue) = self.invalid.get(renderer_key) {
            return OpenGpuiNodeMountOutcome::Invalid(issue.clone());
        }
        let Some(factory) = self.factories.get(renderer_key) else {
            return OpenGpuiNodeMountOutcome::Missing;
        };

        match factory(binding, window, cx) {
            Ok(view) => OpenGpuiNodeMountOutcome::Mounted(view),
            Err(error) => OpenGpuiNodeMountOutcome::FactoryFailed(error),
        }
    }
}

pub(crate) enum OpenGpuiNodeMountOutcome {
    Mounted(AnyView),
    Invalid(OpenGpuiNodeRegistryIssue),
    Missing,
    FactoryFailed(OpenGpuiNodeMountError),
}
