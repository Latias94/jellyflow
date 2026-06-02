use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::callbacks::install_callbacks;

use super::super::scenario::{
    ConformanceCallbackTraceRecorder, ConformanceTraceConfig, ConformanceTraceEvent,
};

pub(super) fn install_trace_recorders(
    store: &mut NodeGraphStore,
    config: ConformanceTraceConfig,
    trace: Rc<RefCell<Vec<ConformanceTraceEvent>>>,
) {
    if config.record_store_events || config.record_gesture_events {
        let store_trace = trace.clone();
        let token = store.subscribe(move |event| {
            if config.record_store_events {
                store_trace
                    .borrow_mut()
                    .push(ConformanceTraceEvent::from_store_event(event));
            }
        });

        if config.record_gesture_events {
            let gesture_trace = trace.clone();
            store.subscribe_gesture_with_token(token, move |event| {
                gesture_trace
                    .borrow_mut()
                    .push(ConformanceTraceEvent::Gesture(event));
            });
        }
    }

    if config.record_xyflow_callbacks {
        let _ = install_callbacks(&mut *store, ConformanceCallbackTraceRecorder::new(trace));
    }
}
