use std::any::Any;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

pub(crate) type PanicPayload = Box<dyn Any + Send + 'static>;

thread_local! {
    // A panic payload may itself panic from `Drop`. Retain at most one nested payload per thread;
    // subsequent recursive payloads propagate instead of leaking without bound.
    static LEAKED_RECURSIVE_PANIC_SENTINEL: Cell<bool> = const { Cell::new(false) };
}

#[derive(Default)]
pub(crate) struct PanicCollector {
    first: Option<PanicPayload>,
}

impl PanicCollector {
    pub(crate) fn catch(&mut self, callback: impl FnOnce()) {
        self.record(catch_unwind(AssertUnwindSafe(callback)).err());
    }

    pub(crate) fn record(&mut self, panic: Option<PanicPayload>) {
        if self.first.is_none() {
            self.first = panic;
        } else {
            suppress(panic);
        }
    }

    pub(crate) fn into_payload(mut self) -> Option<PanicPayload> {
        self.first.take()
    }

    pub(crate) fn resume(mut self) {
        resume(self.first.take());
    }
}

impl Drop for PanicCollector {
    fn drop(&mut self) {
        suppress(self.first.take());
    }
}

pub(crate) fn resume(panic: Option<PanicPayload>) {
    if let Some(payload) = panic {
        resume_unwind(payload);
    }
}

pub(crate) fn suppress(panic: Option<PanicPayload>) {
    if let Some(payload) = panic {
        suppress_payload(payload);
    }
}

fn suppress_payload(payload: PanicPayload) -> bool {
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

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    struct RecursiveDropPanic;

    impl Drop for RecursiveDropPanic {
        fn drop(&mut self) {
            panic!("recursive panic payload drop");
        }
    }

    #[test]
    fn repeated_suppressed_payloads_are_released() {
        let drops = Arc::new(AtomicUsize::new(0));
        for _ in 0..1_024 {
            let drops = Arc::clone(&drops);
            let panic = catch_unwind(AssertUnwindSafe(|| panic_any(DropProbe(drops)))).err();
            suppress(panic);
        }
        assert_eq!(drops.load(Ordering::Relaxed), 1_024);
    }

    #[test]
    fn recursive_payload_suppression_is_bounded_to_one_sentinel_per_thread() {
        std::thread::spawn(|| {
            let first = catch_unwind(AssertUnwindSafe(|| panic_any(RecursiveDropPanic))).err();
            assert!(suppress_payload(first.expect("first panic payload")));

            let escalation = catch_unwind(AssertUnwindSafe(|| {
                let second = catch_unwind(AssertUnwindSafe(|| panic_any(RecursiveDropPanic))).err();
                suppress(second);
            }));
            assert!(
                escalation.is_err(),
                "a second recursive payload must propagate instead of leaking again"
            );
        })
        .join()
        .expect("bounded recursive-payload test thread");
    }
}
