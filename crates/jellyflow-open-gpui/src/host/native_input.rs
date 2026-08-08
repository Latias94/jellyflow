use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use open_gpui::{Keystroke, WindowId};

use super::focus_navigation::OpenGpuiGraphFocusTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiGraphKeyOwner {
    Root,
    Exact(OpenGpuiGraphFocusTarget),
}

impl OpenGpuiGraphKeyOwner {
    pub(crate) const fn target(self) -> Option<OpenGpuiGraphFocusTarget> {
        match self {
            Self::Root => None,
            Self::Exact(target) => Some(target),
        }
    }

    pub(crate) const fn is_root(self) -> bool {
        matches!(self, Self::Root)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiNativeKeyOwner {
    Ime,
    Overlay,
    NativeControl,
    Graph(OpenGpuiGraphKeyOwner),
    None,
}

#[derive(Clone, Debug)]
struct FrozenKeyDown {
    keystroke: Keystroke,
    owner: OpenGpuiNativeKeyOwner,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PairedGraphKey {
    Enter,
    Space,
}

#[derive(Default)]
struct NativeInputState {
    terminal: bool,
    frozen_key_down: Option<FrozenKeyDown>,
    native_tab_fallback: Option<Keystroke>,
    pressed_graph_keys: BTreeMap<PairedGraphKey, OpenGpuiGraphKeyOwner>,
}

#[derive(Clone, Default)]
pub(crate) struct OpenGpuiNativeInputHandle {
    state: Rc<RefCell<NativeInputState>>,
}

pub(super) struct NativeInputOwner {
    handle: OpenGpuiNativeInputHandle,
    window: Option<WindowId>,
}

impl NativeInputOwner {
    pub(super) fn new() -> Self {
        Self {
            handle: OpenGpuiNativeInputHandle::default(),
            window: None,
        }
    }

    pub(super) fn bind_window(&mut self, window: WindowId) {
        match self.window {
            Some(current) => assert_eq!(
                current, window,
                "one native input owner cannot be rebound to another window",
            ),
            None => self.window = Some(window),
        }
    }

    pub(super) fn owns_window(&self, window: WindowId) -> bool {
        self.window == Some(window) && !self.handle.state.borrow().terminal
    }

    pub(super) fn handle(&self) -> OpenGpuiNativeInputHandle {
        self.handle.clone()
    }

    pub(super) fn freeze_key_down(
        &self,
        keystroke: Keystroke,
        owner: OpenGpuiNativeKeyOwner,
    ) -> bool {
        let should_intercept = should_intercept_action(owner, &keystroke);
        let mut state = self.handle.state.borrow_mut();
        if state.terminal {
            return false;
        }
        state.native_tab_fallback = None;
        if let Some(key) = paired_graph_key(&keystroke) {
            state.pressed_graph_keys.remove(&key);
        }
        state.frozen_key_down = Some(FrozenKeyDown { keystroke, owner });
        should_intercept
    }

    pub(super) fn terminate(&mut self) {
        self.window = None;
        self.handle.terminate();
    }
}

impl OpenGpuiNativeInputHandle {
    pub(crate) fn take_key_down(&self, keystroke: &Keystroke) -> OpenGpuiNativeKeyOwner {
        let mut state = self.state.borrow_mut();
        let Some(frozen) = state.frozen_key_down.take() else {
            return OpenGpuiNativeKeyOwner::None;
        };
        if frozen.keystroke != *keystroke {
            return OpenGpuiNativeKeyOwner::None;
        }
        if let OpenGpuiNativeKeyOwner::Graph(owner) = frozen.owner
            && pairs_on_key_up(keystroke)
            && let Some(key) = paired_graph_key(keystroke)
        {
            state.pressed_graph_keys.insert(key, owner);
        }
        frozen.owner
    }

    pub(crate) fn take_key_up(&self, keystroke: &Keystroke) -> OpenGpuiNativeKeyOwner {
        let owner = paired_graph_key(keystroke)
            .and_then(|key| self.state.borrow_mut().pressed_graph_keys.remove(&key));
        owner.map_or(OpenGpuiNativeKeyOwner::None, OpenGpuiNativeKeyOwner::Graph)
    }

    pub(crate) fn defer_native_tab_fallback(&self, keystroke: &Keystroke) {
        self.state.borrow_mut().native_tab_fallback = Some(keystroke.clone());
    }

    pub(crate) fn take_native_tab_fallback(&self, keystroke: &Keystroke) -> bool {
        self.state
            .borrow_mut()
            .native_tab_fallback
            .take()
            .is_some_and(|pending| pending == *keystroke)
    }

    pub(crate) fn terminate(&self) {
        let mut state = self.state.borrow_mut();
        state.terminal = true;
        state.frozen_key_down = None;
        state.native_tab_fallback = None;
        state.pressed_graph_keys.clear();
    }
}

fn should_intercept_action(owner: OpenGpuiNativeKeyOwner, keystroke: &Keystroke) -> bool {
    match owner {
        OpenGpuiNativeKeyOwner::Graph(_) => graph_owned_key(keystroke),
        OpenGpuiNativeKeyOwner::NativeControl => false,
        OpenGpuiNativeKeyOwner::Ime
        | OpenGpuiNativeKeyOwner::Overlay
        | OpenGpuiNativeKeyOwner::None => false,
    }
}

fn graph_owned_key(keystroke: &Keystroke) -> bool {
    if has_non_shift_modifier(keystroke) {
        return false;
    }
    matches!(
        keystroke.key.to_ascii_lowercase().as_str(),
        "escape"
            | "delete"
            | "backspace"
            | "left"
            | "right"
            | "up"
            | "down"
            | "tab"
            | "space"
            | "enter"
            | "f"
    )
}

fn has_non_shift_modifier(keystroke: &Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    modifiers.control || modifiers.alt || modifiers.platform || modifiers.function
}

fn pairs_on_key_up(keystroke: &Keystroke) -> bool {
    !keystroke.modifiers.modified() && paired_graph_key(keystroke).is_some()
}

fn paired_graph_key(keystroke: &Keystroke) -> Option<PairedGraphKey> {
    if keystroke.key.eq_ignore_ascii_case("enter") {
        Some(PairedGraphKey::Enter)
    } else if keystroke.key.eq_ignore_ascii_case("space") {
        Some(PairedGraphKey::Space)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use open_gpui::Modifiers;

    use super::*;

    fn key(name: &str) -> Keystroke {
        Keystroke {
            key: name.to_owned(),
            ..Keystroke::default()
        }
    }

    #[test]
    fn key_down_consumes_the_owner_frozen_before_native_dispatch() {
        let owner = NativeInputOwner::new();
        let escape = key("escape");

        assert!(!owner.freeze_key_down(escape.clone(), OpenGpuiNativeKeyOwner::NativeControl));
        assert_eq!(
            owner.handle().take_key_down(&escape),
            OpenGpuiNativeKeyOwner::NativeControl,
        );
    }

    #[test]
    fn native_control_tab_remains_available_to_the_control() {
        let owner = NativeInputOwner::new();
        let tab = key("tab");

        assert!(!owner.freeze_key_down(tab.clone(), OpenGpuiNativeKeyOwner::NativeControl));
        assert_eq!(
            owner.handle().take_key_down(&tab),
            OpenGpuiNativeKeyOwner::NativeControl,
        );
    }

    #[test]
    fn native_tab_fallback_is_exact_and_single_use() {
        let owner = NativeInputOwner::new();
        let tab = key("tab");
        owner.handle().defer_native_tab_fallback(&tab);

        assert!(!owner.handle().take_native_tab_fallback(&key("escape")));
        assert!(!owner.handle().take_native_tab_fallback(&tab));

        owner.handle().defer_native_tab_fallback(&tab);
        assert!(owner.handle().take_native_tab_fallback(&tab));
        assert!(!owner.handle().take_native_tab_fallback(&tab));
    }

    #[test]
    fn a_different_raw_key_cannot_consume_stale_frozen_authority() {
        let owner = NativeInputOwner::new();
        assert!(owner.freeze_key_down(
            key("delete"),
            OpenGpuiNativeKeyOwner::Graph(OpenGpuiGraphKeyOwner::Root),
        ));

        assert_eq!(
            owner.handle().take_key_down(&key("escape")),
            OpenGpuiNativeKeyOwner::None,
        );
        assert_eq!(
            owner.handle().take_key_down(&key("delete")),
            OpenGpuiNativeKeyOwner::None,
        );
    }

    #[test]
    fn space_key_up_returns_the_exact_key_down_owner() {
        let owner = NativeInputOwner::new();
        let space = key("space");
        let graph = OpenGpuiNativeKeyOwner::Graph(OpenGpuiGraphKeyOwner::Root);
        assert!(owner.freeze_key_down(space.clone(), graph));
        assert_eq!(owner.handle().take_key_down(&space), graph);

        assert_eq!(owner.handle().take_key_up(&space), graph);
        assert_eq!(
            owner.handle().take_key_up(&space),
            OpenGpuiNativeKeyOwner::None,
        );
    }

    #[test]
    fn a_new_native_key_down_revokes_a_lost_graph_key_up() {
        let owner = NativeInputOwner::new();
        let enter = key("enter");
        let graph = OpenGpuiNativeKeyOwner::Graph(OpenGpuiGraphKeyOwner::Root);
        assert!(owner.freeze_key_down(enter.clone(), graph));
        assert_eq!(owner.handle().take_key_down(&enter), graph);

        assert!(!owner.freeze_key_down(enter.clone(), OpenGpuiNativeKeyOwner::NativeControl,));
        assert_eq!(
            owner.handle().take_key_down(&enter),
            OpenGpuiNativeKeyOwner::NativeControl,
        );
        assert_eq!(
            owner.handle().take_key_up(&enter),
            OpenGpuiNativeKeyOwner::None,
        );
    }

    #[test]
    fn modified_native_shortcuts_remain_owned_by_open_gpui() {
        let owner = NativeInputOwner::new();
        for name in ["c", "v", "a", "z"] {
            let keystroke = Keystroke {
                modifiers: Modifiers {
                    platform: true,
                    ..Modifiers::default()
                },
                key: name.to_owned(),
                key_char: None,
            };
            assert!(!owner.freeze_key_down(
                keystroke,
                OpenGpuiNativeKeyOwner::Graph(OpenGpuiGraphKeyOwner::Root),
            ));
        }
    }
}
