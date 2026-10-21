//! Persists the terminal's zoomed font size across launches.
//!
//! The file once held the full-window terminal layout; older copies still
//! carry those fields, which serde skips.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(super) struct SavedTerminalZoom {
    /// Zoomed terminal font size; absent means the default.
    #[serde(default)]
    pub(super) font_size: Option<f32>,
}

fn terminal_zoom_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Insulator")
        .join("terminal-layout.json")
}

impl SavedTerminalZoom {
    /// Read at startup. A missing or unreadable file means the default size.
    pub(super) fn load() -> Self {
        std::fs::read(terminal_zoom_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(super) fn current() -> Self {
        Self {
            font_size: Some(crate::terminal::terminal_font_size()),
        }
    }

    /// Blocking file write. Callers run it on a background thread.
    pub(super) fn write(&self) {
        let path = terminal_zoom_path();
        let Some(parent) = path.parent() else { return };
        let _ = std::fs::create_dir_all(parent);
        let Ok(bytes) = serde_json::to_vec(self) else {
            return;
        };
        let _ = std::fs::write(path, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_layout_files_still_yield_the_font_size() {
        let json = r#"{"mode":"terminal","active":1,"tabs":[{"cwd":"/tmp"}],"font_size":15.0}"#;
        let zoom: SavedTerminalZoom = serde_json::from_str(json).expect("old layout parses");
        assert_eq!(zoom.font_size, Some(15.0));
    }
}
