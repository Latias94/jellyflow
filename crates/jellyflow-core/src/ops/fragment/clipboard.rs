use serde_json as json;

use super::model::GraphFragment;

/// Clipboard header for `GraphFragment` payloads.
pub const GRAPH_FRAGMENT_CLIPBOARD_PREFIX: &str = "jellyflow.fragment.v1\n";

impl GraphFragment {
    /// Serializes this fragment to a clipboard-friendly text payload.
    pub fn to_clipboard_text(&self) -> Result<String, json::Error> {
        let json = json::to_string(self)?;
        Ok(format!("{GRAPH_FRAGMENT_CLIPBOARD_PREFIX}{json}"))
    }

    /// Parses a fragment from clipboard text.
    ///
    /// Accepts both:
    /// - the canonical `jellyflow.fragment.v1` header format,
    /// - raw JSON (useful for debugging and external tooling).
    pub fn from_clipboard_text(text: &str) -> Result<Self, json::Error> {
        let payload = text
            .strip_prefix(GRAPH_FRAGMENT_CLIPBOARD_PREFIX)
            .unwrap_or(text);
        json::from_str(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::GRAPH_FRAGMENT_CLIPBOARD_PREFIX;
    use crate::ops::fragment::GraphFragment;

    #[test]
    fn clipboard_text_roundtrips_with_prefix() {
        let fragment = GraphFragment::default();
        let text = fragment.to_clipboard_text().expect("serialize");
        assert!(text.starts_with(GRAPH_FRAGMENT_CLIPBOARD_PREFIX));
        let parsed = GraphFragment::from_clipboard_text(&text).expect("parse");
        assert_eq!(parsed.version, fragment.version);
        assert_eq!(parsed.nodes.len(), 0);
    }

    #[test]
    fn clipboard_text_accepts_raw_json() {
        let fragment = GraphFragment::default();
        let json = serde_json::to_string(&fragment).expect("serialize");
        let parsed = GraphFragment::from_clipboard_text(&json).expect("parse");
        assert_eq!(parsed.version, fragment.version);
    }
}
