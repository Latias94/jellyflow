use std::str::FromStr;

use keyboard_types::Code as KeyCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serialized key code (a `keyboard_types::Code`), stored as a string like `"Space"` or `"KeyA"`.
///
/// This is intentionally aligned with the `KeyboardEvent.code` naming used by XyFlow for
/// `panActivationKeyCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeGraphKeyCode(pub KeyCode);

impl Serialize for NodeGraphKeyCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for NodeGraphKeyCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let code = KeyCode::from_str(&s)
            .map_err(|_| serde::de::Error::custom(format!("unrecognized key code: {s}")))?;
        Ok(Self(code))
    }
}

/// Delete key binding for removing the current selection (XyFlow `deleteKeyCode`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphDeleteKey {
    /// Delete is disabled.
    None,
    /// Use Backspace (XyFlow default).
    #[default]
    Backspace,
    /// Use Delete.
    Delete,
    /// Accept either Backspace or Delete.
    BackspaceOrDelete,
}

impl NodeGraphDeleteKey {
    pub fn matches(self, key: KeyCode) -> bool {
        match self {
            Self::None => false,
            Self::Backspace => key == KeyCode::Backspace,
            Self::Delete => key == KeyCode::Delete,
            Self::BackspaceOrDelete => matches!(key, KeyCode::Backspace | KeyCode::Delete),
        }
    }
}
