use std::collections::{HashMap, HashSet};

use crate::runtime::graph_runtime::{GraphRevision, GraphRuntimeError, GraphVersion};

use super::{GraphCommandIdentity, GraphCommandOutcome};

/// The terminal classification shared by every command execution consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandExecutionKind {
    Applied,
    Noop,
    Rejected,
    Conflict,
    Failed,
}

/// One terminal result correlated to the opaque identity of the admitted command.
#[derive(Debug)]
#[must_use = "command execution completions contain authoritative outcomes or failures"]
pub struct CommandExecutionCompletion {
    identity: GraphCommandIdentity,
    result: Result<GraphCommandOutcome, GraphRuntimeError>,
}

impl CommandExecutionCompletion {
    pub(crate) fn from_outcome(outcome: GraphCommandOutcome) -> Self {
        Self {
            identity: outcome.identity().clone(),
            result: Ok(outcome),
        }
    }

    pub(crate) fn from_error(identity: GraphCommandIdentity, error: GraphRuntimeError) -> Self {
        Self {
            identity,
            result: Err(error),
        }
    }

    pub fn identity(&self) -> &GraphCommandIdentity {
        &self.identity
    }

    pub fn result(&self) -> Result<&GraphCommandOutcome, &GraphRuntimeError> {
        self.result.as_ref()
    }

    pub fn into_result(self) -> Result<GraphCommandOutcome, GraphRuntimeError> {
        self.result
    }

    pub fn into_parts(
        self,
    ) -> (
        GraphCommandIdentity,
        Result<GraphCommandOutcome, GraphRuntimeError>,
    ) {
        (self.identity, self.result)
    }

    pub fn kind(&self) -> CommandExecutionKind {
        match &self.result {
            Ok(GraphCommandOutcome::Applied(_)) => CommandExecutionKind::Applied,
            Ok(GraphCommandOutcome::Noop(_)) => CommandExecutionKind::Noop,
            Ok(GraphCommandOutcome::Rejected(_)) => CommandExecutionKind::Rejected,
            Ok(GraphCommandOutcome::Conflict(_)) => CommandExecutionKind::Conflict,
            Err(_) => CommandExecutionKind::Failed,
        }
    }

    /// Returns the authoritative version observed by a semantic command outcome.
    ///
    /// Execution failures do not necessarily observe a graph version.
    pub fn version(&self) -> Option<GraphVersion> {
        self.result.as_ref().ok().map(GraphCommandOutcome::version)
    }

    pub fn applied_revision(&self) -> Option<GraphRevision> {
        self.result
            .as_ref()
            .ok()
            .and_then(GraphCommandOutcome::applied)
            .map(|receipt| receipt.revision())
    }

    pub fn observed_revision(&self) -> Option<GraphRevision> {
        self.version().map(|version| version.revision())
    }

    pub fn diagnostic_message(&self) -> Option<String> {
        match &self.result {
            Ok(GraphCommandOutcome::Rejected(rejection)) => Some(rejection.message().to_owned()),
            Ok(GraphCommandOutcome::Conflict(conflict)) => Some(format!(
                "command expected graph version {:?} but current version is {:?}",
                conflict.expected_version(),
                conflict.current_version(),
            )),
            Err(error) => Some(error.to_string()),
            Ok(GraphCommandOutcome::Applied(_) | GraphCommandOutcome::Noop(_)) => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandCompletionCorrelationError {
    #[error("command completion identity is duplicated: {identity:?}")]
    DuplicateIdentity { identity: GraphCommandIdentity },
    #[error("command completion is missing for identity: {identity:?}")]
    MissingIdentity { identity: GraphCommandIdentity },
    #[error("command completion identity is foreign to this submission: {identity:?}")]
    ForeignIdentity { identity: GraphCommandIdentity },
    #[error("command completion was already consumed for identity: {identity:?}")]
    AlreadyConsumed { identity: GraphCommandIdentity },
}

/// Ordered terminal command results with an identity index for exact correlation.
#[derive(Debug, Default)]
pub struct CommandExecutionCompletions {
    ordered: Vec<Option<CommandExecutionCompletion>>,
    by_identity: HashMap<GraphCommandIdentity, usize>,
}

impl CommandExecutionCompletions {
    pub fn try_from_ordered(
        completions: Vec<CommandExecutionCompletion>,
    ) -> Result<Self, CommandCompletionCorrelationError> {
        let mut by_identity = HashMap::with_capacity(completions.len());
        for (index, completion) in completions.iter().enumerate() {
            let identity = completion.identity().clone();
            if by_identity.insert(identity.clone(), index).is_some() {
                return Err(CommandCompletionCorrelationError::DuplicateIdentity { identity });
            }
        }
        Ok(Self {
            ordered: completions.into_iter().map(Some).collect(),
            by_identity,
        })
    }

    pub(crate) fn from_prevalidated_ordered(completions: Vec<CommandExecutionCompletion>) -> Self {
        Self::try_from_ordered(completions)
            .expect("prevalidated command completion identities remain unique")
    }

    pub fn len(&self) -> usize {
        self.ordered.iter().filter(|entry| entry.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.ordered.iter().all(Option::is_none)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CommandExecutionCompletion> {
        self.ordered.iter().filter_map(Option::as_ref)
    }

    pub fn get(
        &self,
        identity: &GraphCommandIdentity,
    ) -> Result<&CommandExecutionCompletion, CommandCompletionCorrelationError> {
        let Some(index) = self.by_identity.get(identity).copied() else {
            return Err(CommandCompletionCorrelationError::MissingIdentity {
                identity: identity.clone(),
            });
        };
        self.ordered[index].as_ref().ok_or_else(|| {
            CommandCompletionCorrelationError::AlreadyConsumed {
                identity: identity.clone(),
            }
        })
    }

    pub fn take(
        &mut self,
        identity: &GraphCommandIdentity,
    ) -> Result<CommandExecutionCompletion, CommandCompletionCorrelationError> {
        let Some(index) = self.by_identity.get(identity).copied() else {
            return Err(CommandCompletionCorrelationError::MissingIdentity {
                identity: identity.clone(),
            });
        };
        self.ordered[index].take().ok_or_else(|| {
            CommandCompletionCorrelationError::AlreadyConsumed {
                identity: identity.clone(),
            }
        })
    }

    /// Atomically consumes the requested identities in caller order.
    pub fn take_all(
        &mut self,
        expected: &[GraphCommandIdentity],
    ) -> Result<Vec<CommandExecutionCompletion>, CommandCompletionCorrelationError> {
        let indices = self.validate_expected(expected, false)?;
        Ok(indices
            .into_iter()
            .map(|index| {
                self.ordered[index]
                    .take()
                    .expect("validated command completion remains present")
            })
            .collect())
    }

    /// Atomically consumes an exact expected/actual identity bijection in caller order.
    pub fn take_exact(
        &mut self,
        expected: &[GraphCommandIdentity],
    ) -> Result<Vec<CommandExecutionCompletion>, CommandCompletionCorrelationError> {
        let indices = self.validate_expected(expected, true)?;
        Ok(indices
            .into_iter()
            .map(|index| {
                self.ordered[index]
                    .take()
                    .expect("validated command completion remains present")
            })
            .collect())
    }

    fn validate_expected(
        &self,
        expected: &[GraphCommandIdentity],
        exact: bool,
    ) -> Result<Vec<usize>, CommandCompletionCorrelationError> {
        let mut expected_set = HashSet::with_capacity(expected.len());
        let mut indices = Vec::with_capacity(expected.len());
        for identity in expected {
            if !expected_set.insert(identity.clone()) {
                return Err(CommandCompletionCorrelationError::DuplicateIdentity {
                    identity: identity.clone(),
                });
            }
            let Some(index) = self.by_identity.get(identity).copied() else {
                return Err(CommandCompletionCorrelationError::MissingIdentity {
                    identity: identity.clone(),
                });
            };
            if self.ordered[index].is_none() {
                return Err(CommandCompletionCorrelationError::AlreadyConsumed {
                    identity: identity.clone(),
                });
            }
            indices.push(index);
        }
        if exact
            && let Some(completion) = self
                .ordered
                .iter()
                .filter_map(Option::as_ref)
                .find(|completion| !expected_set.contains(completion.identity()))
        {
            return Err(CommandCompletionCorrelationError::ForeignIdentity {
                identity: completion.identity().clone(),
            });
        }
        Ok(indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(identity: &GraphCommandIdentity) -> CommandExecutionCompletion {
        CommandExecutionCompletion::from_error(identity.clone(), GraphRuntimeError::Busy)
    }

    #[test]
    fn ordered_collection_rejects_duplicate_actual_identity() {
        let identity = GraphCommandIdentity::new();
        let error = CommandExecutionCompletions::try_from_ordered(vec![
            failed(&identity),
            failed(&identity),
        ])
        .expect_err("duplicate actual completion identity must be rejected");
        assert!(matches!(
            error,
            CommandCompletionCorrelationError::DuplicateIdentity { .. }
        ));
    }

    #[test]
    fn take_exact_is_atomic_and_returns_expected_identity_order() {
        let first = GraphCommandIdentity::new();
        let second = GraphCommandIdentity::new();
        let missing = GraphCommandIdentity::new();
        let mut completions =
            CommandExecutionCompletions::try_from_ordered(vec![failed(&first), failed(&second)])
                .unwrap();

        let error = completions
            .take_exact(&[first.clone(), missing.clone()])
            .expect_err("missing expected identity must reject the whole take");
        assert!(matches!(
            error,
            CommandCompletionCorrelationError::MissingIdentity { identity }
                if identity == missing
        ));
        assert_eq!(
            completions.len(),
            2,
            "failed validation must consume nothing"
        );

        let taken = completions
            .take_exact(&[second.clone(), first.clone()])
            .unwrap();
        assert_eq!(taken[0].identity(), &second);
        assert_eq!(taken[1].identity(), &first);
        assert!(completions.is_empty());
    }

    #[test]
    fn take_exact_rejects_duplicate_expected_and_foreign_actual_atomically() {
        let first = GraphCommandIdentity::new();
        let second = GraphCommandIdentity::new();
        let mut completions =
            CommandExecutionCompletions::try_from_ordered(vec![failed(&first), failed(&second)])
                .unwrap();

        let duplicate = completions
            .take_exact(&[first.clone(), first.clone()])
            .expect_err("duplicate expected identity must be rejected");
        assert!(matches!(
            duplicate,
            CommandCompletionCorrelationError::DuplicateIdentity { .. }
        ));
        assert_eq!(completions.len(), 2);

        let foreign = completions
            .take_exact(std::slice::from_ref(&first))
            .expect_err("unclaimed actual completion must be reported as foreign");
        assert!(matches!(
            foreign,
            CommandCompletionCorrelationError::ForeignIdentity { identity }
                if identity == second
        ));
        assert_eq!(
            completions.len(),
            2,
            "failed validation must consume nothing"
        );
    }
}
