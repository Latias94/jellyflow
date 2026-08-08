//! Application-owned workflow model option resources.

use std::fmt;

use open_gpui::Context;

/// Identifies one application request for workflow model options.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkflowModelOptionsRequestToken(u128);

impl WorkflowModelOptionsRequestToken {
    const INITIAL: Self = Self(1);

    fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

impl fmt::Display for WorkflowModelOptionsRequestToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// One model exposed by the application-owned provider catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowModelOption {
    value: String,
    label: String,
    disabled_reason: Option<String>,
}

impl WorkflowModelOption {
    #[must_use]
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled_reason: None,
        }
    }

    #[must_use]
    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(reason.into());
        self
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.disabled_reason.is_none()
    }
}

/// Declares whether a successful response is authoritative or intentionally incomplete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowModelOptionsCompleteness {
    Partial,
    Complete,
}

/// Structured provider failure with an explicit recovery policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowModelOptionsFailure {
    message: String,
    retryable: bool,
}

impl WorkflowModelOptionsFailure {
    #[must_use]
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }

    #[must_use]
    pub fn non_retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn is_retryable(&self) -> bool {
        self.retryable
    }
}

/// Terminal response published by an application service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkflowModelOptionsResponse {
    Empty,
    Failed(WorkflowModelOptionsFailure),
    Ready {
        options: Vec<WorkflowModelOption>,
        completeness: WorkflowModelOptionsCompleteness,
    },
}

/// Current application-owned resource state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkflowModelOptionsStatus {
    Loading,
    Empty,
    Failed(WorkflowModelOptionsFailure),
    Ready {
        options: Vec<WorkflowModelOption>,
        completeness: WorkflowModelOptionsCompleteness,
    },
}

impl WorkflowModelOptionsStatus {
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    #[must_use]
    pub fn options(&self) -> Option<&[WorkflowModelOption]> {
        match self {
            Self::Ready { options, .. } => Some(options),
            Self::Loading | Self::Empty | Self::Failed(_) => None,
        }
    }

    #[must_use]
    pub fn is_selectable(&self, value: &str) -> bool {
        self.options().is_some_and(|options| {
            options
                .iter()
                .any(|option| option.value() == value && option.is_enabled())
        })
    }
}

/// Result of publishing a provider response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowModelOptionsPublishOutcome {
    Published,
    Stale {
        expected: WorkflowModelOptionsRequestToken,
        received: WorkflowModelOptionsRequestToken,
    },
    RequestAlreadyCompleted {
        token: WorkflowModelOptionsRequestToken,
    },
}

/// Failure to allocate another request token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkflowModelOptionsTokenExhausted;

impl fmt::Display for WorkflowModelOptionsTokenExhausted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("workflow model option request tokens are exhausted")
    }
}

impl std::error::Error for WorkflowModelOptionsTokenExhausted {}

/// Why an explicit retry could not start a new request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowModelOptionsRetryError {
    NotFailed,
    NotRetryable,
    TokenExhausted,
}

impl fmt::Display for WorkflowModelOptionsRetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFailed => formatter.write_str("model options are not in a failed state"),
            Self::NotRetryable => formatter.write_str("model option failure is not retryable"),
            Self::TokenExhausted => {
                formatter.write_str("workflow model option request tokens are exhausted")
            }
        }
    }
}

impl std::error::Error for WorkflowModelOptionsRetryError {}

/// Mutable GPUI entity state advanced by the application service and product tests.
pub struct WorkflowModelOptionsState {
    request_token: WorkflowModelOptionsRequestToken,
    status: WorkflowModelOptionsStatus,
}

impl WorkflowModelOptionsState {
    #[must_use]
    pub const fn loading() -> Self {
        Self {
            request_token: WorkflowModelOptionsRequestToken::INITIAL,
            status: WorkflowModelOptionsStatus::Loading,
        }
    }

    #[must_use]
    pub fn ready_default() -> Self {
        Self {
            request_token: WorkflowModelOptionsRequestToken::INITIAL,
            status: WorkflowModelOptionsStatus::Ready {
                options: vec![
                    WorkflowModelOption::new("gpt-4.1", "GPT-4.1"),
                    WorkflowModelOption::new("claude-3.7-sonnet", "Claude 3.7 Sonnet"),
                ],
                completeness: WorkflowModelOptionsCompleteness::Complete,
            },
        }
    }

    #[must_use]
    pub const fn request_token(&self) -> WorkflowModelOptionsRequestToken {
        self.request_token
    }

    #[must_use]
    pub const fn status(&self) -> &WorkflowModelOptionsStatus {
        &self.status
    }

    pub fn begin_request(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<WorkflowModelOptionsRequestToken, WorkflowModelOptionsTokenExhausted> {
        let token = self
            .request_token
            .next()
            .ok_or(WorkflowModelOptionsTokenExhausted)?;
        self.request_token = token;
        self.status = WorkflowModelOptionsStatus::Loading;
        cx.notify();
        Ok(token)
    }

    pub fn retry(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<WorkflowModelOptionsRequestToken, WorkflowModelOptionsRetryError> {
        match &self.status {
            WorkflowModelOptionsStatus::Failed(failure) if failure.is_retryable() => {}
            WorkflowModelOptionsStatus::Failed(_) => {
                return Err(WorkflowModelOptionsRetryError::NotRetryable);
            }
            WorkflowModelOptionsStatus::Loading
            | WorkflowModelOptionsStatus::Empty
            | WorkflowModelOptionsStatus::Ready { .. } => {
                return Err(WorkflowModelOptionsRetryError::NotFailed);
            }
        }
        self.begin_request(cx)
            .map_err(|_| WorkflowModelOptionsRetryError::TokenExhausted)
    }

    pub fn publish(
        &mut self,
        token: WorkflowModelOptionsRequestToken,
        response: WorkflowModelOptionsResponse,
        cx: &mut Context<Self>,
    ) -> WorkflowModelOptionsPublishOutcome {
        if token != self.request_token {
            return WorkflowModelOptionsPublishOutcome::Stale {
                expected: self.request_token,
                received: token,
            };
        }
        if !self.status.is_loading() {
            return WorkflowModelOptionsPublishOutcome::RequestAlreadyCompleted { token };
        }
        self.status = match response {
            WorkflowModelOptionsResponse::Empty => WorkflowModelOptionsStatus::Empty,
            WorkflowModelOptionsResponse::Failed(failure) => {
                WorkflowModelOptionsStatus::Failed(failure)
            }
            WorkflowModelOptionsResponse::Ready {
                options,
                completeness: _,
            } if options.is_empty() => WorkflowModelOptionsStatus::Empty,
            WorkflowModelOptionsResponse::Ready {
                options,
                completeness,
            } => WorkflowModelOptionsStatus::Ready {
                options,
                completeness,
            },
        };
        cx.notify();
        WorkflowModelOptionsPublishOutcome::Published
    }
}
