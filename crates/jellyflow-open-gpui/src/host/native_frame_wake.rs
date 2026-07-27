//! Adapter-local bridge for a fallback native frame request.
//!
//! Open GPUI 0.2 exposes `Window::request_animation_frame` only during rendering. A pointer event
//! that begins a continuous interaction happens before the host has a later render in which to make
//! that request. On macOS, this bridge captures the native view while the window is available, then
//! queues one AppKit display on the main dispatch queue after the application update and current
//! display callback have both returned. The interaction owner schedules it only as a delayed
//! fallback: every semantic tick still enters exclusively through `Window::on_next_frame`, and the
//! normal Open GPUI display path cancels the fallback first.

#[cfg(target_os = "macos")]
use std::ptr::NonNull;

#[cfg(target_os = "macos")]
use open_gpui::Window;

/// A live native view captured from a current Open GPUI window update.
///
/// It is intentionally consumed immediately after the update returns, before the foreground task
/// yields again. That keeps the raw handle within the window's UI-thread lifetime while avoiding a
/// synchronous platform callback under Open GPUI's application borrow.
#[cfg(target_os = "macos")]
pub(super) struct PlatformFrameWake(NonNull<objc::runtime::Object>);

#[cfg(target_os = "macos")]
#[allow(unsafe_code, unexpected_cfgs)]
pub(super) fn prepare(window: &mut Window) -> Option<PlatformFrameWake> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    window.refresh();
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    NonNull::new(handle.ns_view.as_ptr().cast()).map(PlatformFrameWake)
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, unexpected_cfgs)]
impl PlatformFrameWake {
    pub(super) fn request(self) {
        use dispatch2::DispatchQueue;
        use objc::runtime::{Object, YES};
        use objc::{msg_send, sel, sel_impl};

        let view = self.0.as_ptr().cast::<Object>();
        let view_address = view as usize;
        // Keep the native view alive until the queued main-thread display request has run. The
        // queue boundary prevents AppKit from reentering Open GPUI before its current callback has
        // restored the platform frame callback.
        unsafe {
            let _: *mut Object = msg_send![view, retain];
        }
        DispatchQueue::main().exec_async(move || {
            let view = view_address as *mut Object;
            unsafe {
                let _: () = msg_send![view, setNeedsDisplay: YES];
                let _: () = msg_send![view, displayIfNeeded];
                let _: () = msg_send![view, release];
            }
        });
    }
}
