use std::any::Any;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

pub(crate) type OpenGpuiPanicPayload = Box<dyn Any + Send + 'static>;

thread_local! {
    // An adversarial panic payload may panic again from `Drop`. Rust cannot both discard an
    // infinitely recursive payload and keep unwinding recoverable, so retain at most one sentinel
    // per thread. A second recursive payload is escalated instead of leaking linearly.
    static LEAKED_RECURSIVE_PANIC_SENTINEL: Cell<bool> = const { Cell::new(false) };
}

/// Bounded owner for callback panics collected while a host finishes an atomic transition.
#[derive(Default)]
pub(crate) struct OpenGpuiPanicAccumulator {
    first: Option<OpenGpuiPanicPayload>,
    leaked_recursive_sentinel: bool,
}

impl OpenGpuiPanicAccumulator {
    pub(crate) fn capture(&mut self, callback: impl FnOnce()) {
        self.push(catch_unwind(AssertUnwindSafe(callback)).err());
    }

    pub(crate) fn push(&mut self, panic: Option<OpenGpuiPanicPayload>) {
        let Some(panic) = panic else {
            return;
        };
        if self.first.is_none() {
            self.first = Some(panic);
        } else {
            self.leaked_recursive_sentinel |= suppress_payload(panic);
        }
    }

    pub(crate) fn merge(&mut self, mut other: Self) {
        self.leaked_recursive_sentinel |= other.leaked_recursive_sentinel;
        self.push(other.first.take());
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.first.is_none()
    }

    pub(crate) fn leaked_recursive_sentinel(&self) -> bool {
        self.leaked_recursive_sentinel
    }

    pub(crate) fn resume(mut self) {
        let panic = self.first.take();
        drop(self);
        if let Some(panic) = panic {
            resume_unwind(panic);
        }
    }

    pub(crate) fn suppress(mut self) -> bool {
        let leaked =
            self.first.take().is_some_and(suppress_payload) || self.leaked_recursive_sentinel;
        self.leaked_recursive_sentinel = false;
        leaked
    }
}

impl Drop for OpenGpuiPanicAccumulator {
    fn drop(&mut self) {
        if let Some(panic) = self.first.take() {
            self.leaked_recursive_sentinel |= suppress_payload(panic);
        }
    }
}

fn suppress_payload(payload: OpenGpuiPanicPayload) -> bool {
    let nested = catch_unwind(AssertUnwindSafe(|| drop(payload))).err();
    let Some(nested) = nested else {
        return false;
    };

    let retain_sentinel = LEAKED_RECURSIVE_PANIC_SENTINEL.with(|sentinel| !sentinel.replace(true));
    if retain_sentinel {
        std::mem::forget(nested);
        true
    } else {
        resume_unwind(nested);
    }
}

#[cfg(test)]
mod tests {
    use std::panic::panic_any;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct DropProbe(Arc<AtomicUsize>);

    struct RecursiveDropPanic;

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl Drop for RecursiveDropPanic {
        fn drop(&mut self) {
            panic!("recursive panic payload drop");
        }
    }

    #[test]
    fn repeated_suppressed_payloads_are_dropped_instead_of_forgotten() {
        let drops = Arc::new(AtomicUsize::new(0));
        for _ in 0..1_024 {
            let drops = Arc::clone(&drops);
            let panic = catch_unwind(AssertUnwindSafe(|| panic_any(DropProbe(drops)))).err();
            let mut accumulator = OpenGpuiPanicAccumulator::default();
            accumulator.push(panic);
            assert!(!accumulator.suppress());
        }
        assert_eq!(drops.load(Ordering::Relaxed), 1_024);
    }

    #[test]
    fn recursive_payload_suppression_is_bounded_to_one_sentinel_per_thread() {
        let first = catch_unwind(AssertUnwindSafe(|| panic_any(RecursiveDropPanic))).err();
        let mut accumulator = OpenGpuiPanicAccumulator::default();
        accumulator.push(first);
        assert!(accumulator.suppress());

        let escalation = catch_unwind(AssertUnwindSafe(|| {
            let second = catch_unwind(AssertUnwindSafe(|| panic_any(RecursiveDropPanic))).err();
            let mut accumulator = OpenGpuiPanicAccumulator::default();
            accumulator.push(second);
            accumulator.suppress();
        }));
        assert!(
            escalation.is_err(),
            "a second recursive payload must escalate instead of leaking again"
        );
    }
}
