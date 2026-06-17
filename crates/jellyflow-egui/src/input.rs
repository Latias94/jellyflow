use eframe::egui::{Context, Key, Modifiers};

use crate::bridge::JellyflowEguiBridge;
use crate::state::JellyflowEguiState;

pub fn handle_global_shortcuts(
    ctx: &Context,
    bridge: &mut JellyflowEguiBridge,
    state: &mut JellyflowEguiState,
) {
    let mut undo = false;
    let mut redo = false;
    let mut delete = false;
    ctx.input(|input| {
        undo = command(input.modifiers, input.key_pressed(Key::Z)) && !input.modifiers.shift;
        redo = command(input.modifiers, input.key_pressed(Key::Z)) && input.modifiers.shift;
        delete = input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace);
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
}

fn command(modifiers: Modifiers, key_pressed: bool) -> bool {
    key_pressed && (modifiers.command || modifiers.ctrl)
}
