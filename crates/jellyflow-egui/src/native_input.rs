use egui::{Context, Event, Id, Key, Modifiers, Popup, Response};
use jellyflow_runtime::interaction::{CanvasPressTarget, NodeNudgeDirection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EguiGraphKeyAction {
    Cancel,
    DeleteSelection,
    Nudge {
        direction: NodeNudgeDirection,
        accelerated: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiFocusedResponseOwner {
    Graph,
    Native,
}

#[derive(Debug, Clone, Copy)]
struct EguiFrozenGraphKey {
    key: Key,
    modifiers: Modifiers,
    action: EguiGraphKeyAction,
}

/// Event-time keyboard ownership for one immediate-mode frame.
///
/// The focused response is classified only when the application renders it. An unobserved focused
/// widget remains native-owned, so disappearing or third-party controls cannot leak shortcuts into
/// the graph.
#[derive(Debug)]
pub(super) struct EguiNativeInputFrame {
    ime_owned: bool,
    popup_owned: bool,
    text_edit_owned: bool,
    focused_at_dispatch: Option<Id>,
    focused_response_owner: Option<EguiFocusedResponseOwner>,
    graph_capture_owned: bool,
    key: Option<EguiFrozenGraphKey>,
}

impl EguiNativeInputFrame {
    pub(super) fn capture(ctx: &Context, graph_capture_owned: bool) -> Self {
        let (ime_owned, key) = ctx.input(|input| {
            let ime_owned = input
                .events
                .iter()
                .any(|event| matches!(event, Event::Ime(_)));
            let key = input.events.iter().find_map(frozen_graph_key);
            (ime_owned, key)
        });
        Self {
            ime_owned,
            popup_owned: Popup::is_any_open(ctx),
            text_edit_owned: ctx.text_edit_focused(),
            focused_at_dispatch: ctx.memory(|memory| memory.focused()),
            focused_response_owner: None,
            graph_capture_owned,
            key,
        }
    }

    pub(super) fn observe_response(&mut self, response: &Response, target: &CanvasPressTarget) {
        if self.focused_at_dispatch != Some(response.id) {
            return;
        }
        let owner = if matches!(target, CanvasPressTarget::NativeControl) {
            EguiFocusedResponseOwner::Native
        } else {
            EguiFocusedResponseOwner::Graph
        };
        self.focused_response_owner = Some(match (self.focused_response_owner, owner) {
            (Some(EguiFocusedResponseOwner::Native), _) | (_, EguiFocusedResponseOwner::Native) => {
                EguiFocusedResponseOwner::Native
            }
            _ => EguiFocusedResponseOwner::Graph,
        });
    }

    pub(super) fn take_graph_action(&mut self, ctx: &Context) -> Option<EguiGraphKeyAction> {
        if !self.graph_owns_key() {
            return None;
        }
        let key = self.key.take()?;
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(
                    event,
                    Event::Key {
                        key: event_key,
                        pressed: true,
                        modifiers,
                        ..
                    } if *event_key == key.key && *modifiers == key.modifiers
                )
            });
        });
        Some(key.action)
    }

    fn graph_owns_key(&self) -> bool {
        if self.key.is_none() || self.ime_owned || self.popup_owned || self.text_edit_owned {
            return false;
        }
        match (self.focused_at_dispatch, self.focused_response_owner) {
            (_, Some(EguiFocusedResponseOwner::Native)) => false,
            (_, Some(EguiFocusedResponseOwner::Graph)) => true,
            (Some(_), None) => false,
            (None, None) => self.graph_capture_owned,
        }
    }
}

fn frozen_graph_key(event: &Event) -> Option<EguiFrozenGraphKey> {
    let Event::Key {
        key,
        pressed: true,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    let no_command_modifier = !modifiers.command && !modifiers.ctrl && !modifiers.alt;
    let action = match key {
        Key::Escape if no_command_modifier => EguiGraphKeyAction::Cancel,
        Key::Delete | Key::Backspace if no_command_modifier => EguiGraphKeyAction::DeleteSelection,
        Key::ArrowLeft if no_command_modifier => EguiGraphKeyAction::Nudge {
            direction: NodeNudgeDirection::Left,
            accelerated: modifiers.shift,
        },
        Key::ArrowRight if no_command_modifier => EguiGraphKeyAction::Nudge {
            direction: NodeNudgeDirection::Right,
            accelerated: modifiers.shift,
        },
        Key::ArrowUp if no_command_modifier => EguiGraphKeyAction::Nudge {
            direction: NodeNudgeDirection::Up,
            accelerated: modifiers.shift,
        },
        Key::ArrowDown if no_command_modifier => EguiGraphKeyAction::Nudge {
            direction: NodeNudgeDirection::Down,
            accelerated: modifiers.shift,
        },
        _ => return None,
    };
    Some(EguiFrozenGraphKey {
        key: *key,
        modifiers: *modifiers,
        action,
    })
}
