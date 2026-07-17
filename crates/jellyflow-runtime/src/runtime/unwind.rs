use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

pub(crate) type PanicPayload = Box<dyn Any + Send + 'static>;

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
        std::mem::forget(payload);
    }
}
