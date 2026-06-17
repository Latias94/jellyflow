use eframe::egui::{Context, Key, Modifiers};
use jellyflow::runtime::runtime::drag::NodeNudgeDirection;

use crate::bridge::JellyflowEguiBridge;
use crate::state::JellyflowEguiState;

pub fn handle_global_shortcuts(
    ctx: &Context,
    bridge: &mut JellyflowEguiBridge,
    state: &mut JellyflowEguiState,
) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }

    let mut undo = false;
    let mut redo = false;
    let mut delete = false;
    let mut nudge = None;
    ctx.input(|input| {
        undo = command(input.modifiers, input.key_pressed(Key::Z)) && !input.modifiers.shift;
        redo = command(input.modifiers, input.key_pressed(Key::Z)) && input.modifiers.shift;
        delete = input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace);
        if !input.modifiers.command && !input.modifiers.ctrl && !input.modifiers.alt {
            nudge = nudge_direction(input).map(|direction| (direction, input.modifiers.shift));
        }
    });

    if undo {
        match bridge.undo() {
            Ok(Some(_)) => state.set_status("Undo"),
            Ok(None) => state.set_status("Nothing to undo"),
            Err(err) => state.set_status(err.to_string()),
        }
    }
    if redo {
        match bridge.redo() {
            Ok(Some(_)) => state.set_status("Redo"),
            Ok(None) => state.set_status("Nothing to redo"),
            Err(err) => state.set_status(err.to_string()),
        }
    }
    if delete {
        match bridge.delete_selection() {
            Ok(Some(_)) => state.set_status("Deleted selection"),
            Ok(None) => state.set_status("Nothing selected"),
            Err(err) => state.set_status(err),
        }
    }
    if let Some((direction, fast)) = nudge {
        match bridge.nudge_selection(direction, fast) {
            Ok(Some(_)) => state.set_status("Nudged selection"),
            Ok(None) => state.set_status("Nothing to nudge"),
            Err(err) => state.set_status(err.to_string()),
        }
    }
}

fn command(modifiers: Modifiers, key_pressed: bool) -> bool {
    key_pressed && (modifiers.command || modifiers.ctrl)
}

fn nudge_direction(input: &eframe::egui::InputState) -> Option<NodeNudgeDirection> {
    if input.key_pressed(Key::ArrowUp) {
        Some(NodeNudgeDirection::Up)
    } else if input.key_pressed(Key::ArrowDown) {
        Some(NodeNudgeDirection::Down)
    } else if input.key_pressed(Key::ArrowLeft) {
        Some(NodeNudgeDirection::Left)
    } else if input.key_pressed(Key::ArrowRight) {
        Some(NodeNudgeDirection::Right)
    } else {
        None
    }
}
