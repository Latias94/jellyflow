use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::conformance::{ConformanceCallbackEvent, ConformanceCallbackTraceSink};

use super::events::HarnessEvent;

#[derive(Clone)]
pub(super) struct HarnessCallbackTraceSink {
    events: Rc<RefCell<Vec<HarnessEvent>>>,
}

impl HarnessCallbackTraceSink {
    pub(super) fn new(events: Rc<RefCell<Vec<HarnessEvent>>>) -> Self {
        Self { events }
    }
}

impl ConformanceCallbackTraceSink for HarnessCallbackTraceSink {
    fn push_callback(&self, event: ConformanceCallbackEvent) {
        self.events.borrow_mut().push(HarnessEvent::Callback(event));
    }
}
