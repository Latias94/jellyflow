use serde::{Deserialize, Deserializer, Serialize};

/// Behavior for selecting edges during marquee (box) selection.
///
/// XyFlow selects edges connected to the selected nodes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphBoxSelectEdges {
    /// Do not select edges from a marquee selection.
    None,
    /// Select edges connected to any selected node (XyFlow default).
    #[default]
    Connected,
    /// Select edges only when both endpoints are within the marquee-selected node set.
    BothEndpoints,
}

impl<'de> Deserialize<'de> for NodeGraphBoxSelectEdges {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = NodeGraphBoxSelectEdges;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a bool or one of: none, connected, both_endpoints")
            }

            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(if v {
                    NodeGraphBoxSelectEdges::Connected
                } else {
                    NodeGraphBoxSelectEdges::None
                })
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v {
                    "none" => Ok(NodeGraphBoxSelectEdges::None),
                    "connected" => Ok(NodeGraphBoxSelectEdges::Connected),
                    "both_endpoints" => Ok(NodeGraphBoxSelectEdges::BothEndpoints),
                    other => Err(E::custom(format!(
                        "unrecognized box select edges mode: {other}"
                    ))),
                }
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Behavior for selecting nodes during marquee (box) selection.
///
/// This matches XyFlow's `selectionMode`:
/// - `partial`: select nodes when they intersect the marquee (Jellyflow default).
/// - `full`: select nodes only when their rect is fully contained in the marquee.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphSelectionMode {
    /// Select nodes when partially intersecting the marquee.
    #[default]
    Partial,
    /// Select nodes only when fully contained by the marquee.
    Full,
}

pub(super) fn default_box_select_edges() -> NodeGraphBoxSelectEdges {
    NodeGraphBoxSelectEdges::Connected
}
