use jellyflow_runtime::geometry::ViewPoint;
use open_gpui::{
    KeyDownEvent, KeyUpEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PlatformInput, point, px,
};

fn native_point(view_point: ViewPoint) -> open_gpui::Point<open_gpui::Pixels> {
    point(px(view_point.x), px(view_point.y))
}

pub(super) fn mouse_down(button: MouseButton, position: ViewPoint) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent {
        button,
        position: native_point(position),
        ..MouseDownEvent::default()
    })
}

pub(super) fn mouse_move(button: MouseButton, position: ViewPoint) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position: native_point(position),
        pressed_button: Some(button),
        ..MouseMoveEvent::default()
    })
}

pub(super) fn mouse_up(button: MouseButton, position: ViewPoint) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent {
        button,
        position: native_point(position),
        ..MouseUpEvent::default()
    })
}

pub(super) fn key_down(key: &str) -> Result<PlatformInput, &'static str> {
    Ok(PlatformInput::KeyDown(KeyDownEvent {
        keystroke: Keystroke::parse(key).map_err(|_| "input_keystroke_invalid")?,
        is_held: false,
        prefer_character_input: false,
    }))
}

pub(super) fn key_up(key: &str) -> Result<PlatformInput, &'static str> {
    Ok(PlatformInput::KeyUp(KeyUpEvent {
        keystroke: Keystroke::parse(key).map_err(|_| "input_keystroke_invalid")?,
    }))
}
