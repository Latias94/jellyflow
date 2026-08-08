use std::cell::Cell;

use jellyflow_core::core::{EdgeKind, Graph, PortId};
use jellyflow_core::types::{TypeCompatibility, TypeCompatibilityResult, TypeDesc};

use crate::profile::{GraphProfile, PortTypeImpactContext, PortTypeRevalidation};
use crate::rules::Diagnostic;
use crate::runtime::commands::TopologyEdit;

use super::GraphRuntimeError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProfileEvaluationError {
    RuntimeUnavailable,
    ReentrantQuery,
}

impl From<ProfileEvaluationError> for GraphRuntimeError {
    fn from(error: ProfileEvaluationError) -> Self {
        match error {
            ProfileEvaluationError::RuntimeUnavailable => GraphRuntimeError::RuntimeUnavailable,
            ProfileEvaluationError::ReentrantQuery => GraphRuntimeError::ReentrantProfileQuery,
        }
    }
}

#[derive(Default)]
pub(super) struct ProfileEvaluationGate {
    hook_active: Cell<bool>,
    failure: Cell<Option<ProfileEvaluationError>>,
}

impl ProfileEvaluationGate {
    pub(super) fn ensure_query_entry(
        &self,
        alive: &Cell<bool>,
    ) -> Result<(), ProfileEvaluationError> {
        if !alive.get() {
            self.record_failure(ProfileEvaluationError::RuntimeUnavailable);
            return Err(ProfileEvaluationError::RuntimeUnavailable);
        }
        if self.hook_active.get() {
            self.record_failure(ProfileEvaluationError::ReentrantQuery);
            return Err(ProfileEvaluationError::ReentrantQuery);
        }
        Ok(())
    }

    pub(super) fn evaluate<T>(
        &self,
        profile: &dyn GraphProfile,
        alive: &Cell<bool>,
        operation: impl FnOnce(&dyn GraphProfile) -> T,
    ) -> Result<T, ProfileEvaluationError> {
        let evaluator = self.begin(profile, alive)?;
        let value = operation(&evaluator);
        evaluator.finish()?;
        Ok(value)
    }

    fn begin<'a>(
        &'a self,
        profile: &'a dyn GraphProfile,
        alive: &'a Cell<bool>,
    ) -> Result<ProfileEvaluator<'a>, ProfileEvaluationError> {
        self.ensure_query_entry(alive)?;
        self.failure.set(None);
        Ok(ProfileEvaluator {
            profile,
            alive,
            gate: self,
        })
    }

    fn record_failure(&self, failure: ProfileEvaluationError) {
        if failure == ProfileEvaluationError::RuntimeUnavailable || self.failure.get().is_none() {
            self.failure.set(Some(failure));
        }
    }

    fn enter_hook<'a>(&'a self, alive: &'a Cell<bool>) -> Option<ProfileHookGuard<'a>> {
        if !alive.get() {
            self.record_failure(ProfileEvaluationError::RuntimeUnavailable);
            return None;
        }
        if self.failure.get().is_some() {
            return None;
        }
        if self.hook_active.replace(true) {
            self.record_failure(ProfileEvaluationError::ReentrantQuery);
            return None;
        }
        Some(ProfileHookGuard { gate: self })
    }
}

struct ProfileHookGuard<'a> {
    gate: &'a ProfileEvaluationGate,
}

impl Drop for ProfileHookGuard<'_> {
    fn drop(&mut self) {
        self.gate.hook_active.set(false);
    }
}

#[must_use = "profile evaluation must be checked after the guarded operation"]
struct ProfileEvaluator<'a> {
    profile: &'a dyn GraphProfile,
    alive: &'a Cell<bool>,
    gate: &'a ProfileEvaluationGate,
}

impl ProfileEvaluator<'_> {
    fn record_runtime_state(&self) {
        if !self.alive.get() {
            self.gate
                .record_failure(ProfileEvaluationError::RuntimeUnavailable);
        }
    }

    fn finish(&self) -> Result<(), ProfileEvaluationError> {
        self.record_runtime_state();
        match self.gate.failure.get() {
            Some(failure) => Err(failure),
            None => Ok(()),
        }
    }

    fn evaluate<T>(
        &self,
        fallback: impl FnOnce() -> T,
        hook: impl FnOnce(&dyn GraphProfile) -> T,
    ) -> T {
        let Some(guard) = self.gate.enter_hook(self.alive) else {
            return fallback();
        };
        let value = hook(self.profile);
        drop(guard);
        self.record_runtime_state();
        value
    }

    fn compatibility_policy(&self) -> Option<&dyn TypeCompatibility> {
        let guard = self.gate.enter_hook(self.alive)?;
        let compatibility = self.profile.type_compatibility();
        drop(guard);
        self.finish().ok()?;
        Some(compatibility)
    }
}

impl GraphProfile for ProfileEvaluator<'_> {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        self.evaluate(|| None, |profile| profile.type_of_port(graph, port))
    }

    fn type_compatibility(&self) -> &dyn TypeCompatibility {
        self
    }

    fn port_types_affected_by_mutation(
        &self,
        context: &mut PortTypeImpactContext<'_>,
    ) -> PortTypeRevalidation {
        self.evaluate(
            || PortTypeRevalidation::All,
            |profile| profile.port_types_affected_by_mutation(context),
        )
    }

    fn dependent_port_types(
        &self,
        context: &mut PortTypeImpactContext<'_>,
        changed_port: PortId,
    ) -> PortTypeRevalidation {
        self.evaluate(
            || PortTypeRevalidation::All,
            |profile| profile.dependent_port_types(context, changed_port),
        )
    }

    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic> {
        self.evaluate(Vec::new, |profile| profile.validate_graph(graph))
    }

    fn allow_cycles(&self, edge_kind: EdgeKind) -> bool {
        self.evaluate(|| false, |profile| profile.allow_cycles(edge_kind))
    }

    fn concretize_bound(&self) -> usize {
        self.evaluate(|| 0, |profile| profile.concretize_bound())
    }

    fn concretize(&self, graph: &Graph) -> TopologyEdit {
        self.evaluate(TopologyEdit::new, |profile| profile.concretize(graph))
    }
}

impl TypeCompatibility for ProfileEvaluator<'_> {
    fn compatible(&self, from: &TypeDesc, to: &TypeDesc) -> TypeCompatibilityResult {
        let unavailable = || TypeCompatibilityResult::Incompatible {
            reason: "profile evaluation is unavailable".to_owned(),
        };
        let Some(compatibility) = self.compatibility_policy() else {
            return unavailable();
        };
        let Some(guard) = self.gate.enter_hook(self.alive) else {
            return unavailable();
        };
        let result = compatibility.compatible(from, to);
        drop(guard);
        if self.finish().is_err() {
            return unavailable();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    use jellyflow_core::core::GraphId;

    use super::*;

    #[derive(Clone, Copy)]
    enum ProbeAction {
        None,
        Reenter,
        ReenterThenRetire,
        Panic,
    }

    struct ProbeProfile {
        gate: Rc<ProfileEvaluationGate>,
        alive: Rc<Cell<bool>>,
        action: ProbeAction,
        nested_error: Rc<Cell<Option<ProfileEvaluationError>>>,
        calls: Rc<Cell<usize>>,
    }

    impl GraphProfile for ProbeProfile {
        fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
            None
        }

        fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
            self.calls.set(self.calls.get().saturating_add(1));
            match self.action {
                ProbeAction::None => {}
                ProbeAction::Reenter => {
                    self.nested_error
                        .set(self.gate.ensure_query_entry(&self.alive).err());
                }
                ProbeAction::ReenterThenRetire => {
                    self.nested_error
                        .set(self.gate.ensure_query_entry(&self.alive).err());
                    self.alive.set(false);
                }
                ProbeAction::Panic => panic!("profile hook panic"),
            }
            Vec::new()
        }
    }

    fn profile(
        gate: Rc<ProfileEvaluationGate>,
        alive: Rc<Cell<bool>>,
        action: ProbeAction,
    ) -> (
        ProbeProfile,
        Rc<Cell<Option<ProfileEvaluationError>>>,
        Rc<Cell<usize>>,
    ) {
        let nested_error = Rc::new(Cell::new(None));
        let calls = Rc::new(Cell::new(0));
        (
            ProbeProfile {
                gate,
                alive,
                action,
                nested_error: nested_error.clone(),
                calls: calls.clone(),
            },
            nested_error,
            calls,
        )
    }

    #[test]
    fn reentry_is_recorded_and_stops_the_evaluation() {
        let gate = Rc::new(ProfileEvaluationGate::default());
        let alive = Rc::new(Cell::new(true));
        let (profile, nested_error, calls) =
            profile(gate.clone(), alive.clone(), ProbeAction::Reenter);

        let error = gate
            .evaluate(&profile, &alive, |profile| {
                profile.validate_graph(&Graph::new(GraphId::from_u128(1)))
            })
            .expect_err("nested profile query must reject the outer evaluation");

        assert_eq!(error, ProfileEvaluationError::ReentrantQuery);
        assert_eq!(
            nested_error.get(),
            Some(ProfileEvaluationError::ReentrantQuery)
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn runtime_unavailable_overrides_reentry_from_the_same_hook() {
        let gate = Rc::new(ProfileEvaluationGate::default());
        let alive = Rc::new(Cell::new(true));
        let (profile, nested_error, _) =
            profile(gate.clone(), alive.clone(), ProbeAction::ReenterThenRetire);

        let error = gate
            .evaluate(&profile, &alive, |profile| {
                profile.validate_graph(&Graph::new(GraphId::from_u128(2)))
            })
            .expect_err("runtime retirement must terminate profile evaluation");

        assert_eq!(
            nested_error.get(),
            Some(ProfileEvaluationError::ReentrantQuery)
        );
        assert_eq!(error, ProfileEvaluationError::RuntimeUnavailable);
    }

    #[test]
    fn panicking_hook_releases_the_guard_for_the_next_evaluation() {
        let gate = Rc::new(ProfileEvaluationGate::default());
        let alive = Rc::new(Cell::new(true));
        let (panicking, _, _) = profile(gate.clone(), alive.clone(), ProbeAction::Panic);

        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _ = gate.evaluate(&panicking, &alive, |profile| {
                    profile.validate_graph(&Graph::new(GraphId::from_u128(3)))
                });
            }))
            .is_err()
        );

        let (healthy, _, calls) = profile(gate.clone(), alive.clone(), ProbeAction::None);
        assert!(
            gate.evaluate(&healthy, &alive, |profile| {
                profile.validate_graph(&Graph::new(GraphId::from_u128(4)))
            })
            .is_ok()
        );
        assert_eq!(calls.get(), 1);
    }
}
