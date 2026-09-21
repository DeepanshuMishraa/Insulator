//! Customizable keyboard shortcuts.
//!
//! The app's shortcuts are declared in `crate::lib` with portable
//! `secondary-*` chords. This module is the registry that makes a subset of
//! them user-configurable: each entry has a stable id, a default chord, and
//! the key context it is bound in. Overrides persist in
//! `AppSettings::keybindings` as `id -> chord` (`""` means unbound) and are
//! applied on top of the defaults with `gpui::NoAction` disabling the default
//! chord plus a fresh binding for the replacement.

use std::collections::HashMap;

/// A single rebindable shortcut.
#[derive(Clone, Copy, Debug)]
pub struct KeybindingDef {
    /// Stable id persisted in settings.
    pub id: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// Group shown in the settings page.
    pub category: &'static str,
    /// Portable default chord (`secondary-*`), or `None` for unbound.
    pub default: Option<&'static str>,
    /// Key context the binding lives in (`None` = global).
    pub context: Option<&'static str>,
}

/// Categories in display order.
pub const CATEGORIES: &[&str] = &["Chat", "Tabs", "Panels", "Review", "Find", "General"];

/// All rebindable shortcuts.
pub const ALL: &[KeybindingDef] = &[
    KeybindingDef {
        id: "new_chat",
        label: "New chat",
        category: "Chat",
        default: Some("secondary-n"),
        context: None,
    },
    KeybindingDef {
        id: "new_project",
        label: "New project",
        category: "Chat",
        default: Some("secondary-o"),
        context: None,
    },
    KeybindingDef {
        id: "new_tab",
        label: "New tab",
        category: "Tabs",
        default: Some("secondary-t"),
        context: None,
    },
    KeybindingDef {
        id: "focus_composer",
        label: "Focus composer",
        category: "Chat",
        default: Some("secondary-l"),
        context: None,
    },
    KeybindingDef {
        id: "next_tab",
        label: "Next tab",
        category: "Tabs",
        default: Some("ctrl-tab"),
        context: None,
    },
    KeybindingDef {
        id: "prev_tab",
        label: "Previous tab",
        category: "Tabs",
        default: Some("ctrl-shift-tab"),
        context: None,
    },
    KeybindingDef {
        id: "close_tab",
        label: "Close tab / window",
        category: "Tabs",
        default: Some("secondary-w"),
        context: None,
    },
    KeybindingDef {
        id: "close_active_tab",
        label: "Close active tab",
        category: "Tabs",
        default: Some("secondary-shift-w"),
        context: None,
    },
    KeybindingDef {
        id: "next_main_tab",
        label: "Next main tab",
        category: "Tabs",
        default: None,
        context: None,
    },
    KeybindingDef {
        id: "prev_main_tab",
        label: "Previous main tab",
        category: "Tabs",
        default: None,
        context: None,
    },
    KeybindingDef {
        id: "toggle_sidebar",
        label: "Toggle left panel",
        category: "Panels",
        default: Some("secondary-b"),
        context: None,
    },
    KeybindingDef {
        id: "toggle_right_panel",
        label: "Toggle right panel",
        category: "Panels",
        default: Some("secondary-shift-b"),
        context: None,
    },
    KeybindingDef {
        id: "command_palette",
        label: "Command palette",
        category: "Panels",
        default: Some("secondary-k"),
        context: None,
    },
    KeybindingDef {
        id: "model_picker",
        label: "Model picker",
        category: "Panels",
        default: Some("secondary-/"),
        context: None,
    },
    KeybindingDef {
        id: "usage_panel",
        label: "Usage panel",
        category: "Panels",
        default: Some("secondary-u"),
        context: None,
    },
    KeybindingDef {
        id: "open_review",
        label: "Open Review tab",
        category: "Review",
        default: Some("secondary-shift-e"),
        context: None,
    },
    KeybindingDef {
        id: "pull_requests",
        label: "Toggle pull requests",
        category: "Review",
        default: Some("secondary-shift-p"),
        context: None,
    },
    KeybindingDef {
        id: "open_find",
        label: "Find in file",
        category: "Find",
        default: Some("secondary-f"),
        context: Some("Insulator"),
    },
    KeybindingDef {
        id: "find_next",
        label: "Find next",
        category: "Find",
        default: Some("secondary-g"),
        context: Some("Insulator"),
    },
    KeybindingDef {
        id: "find_prev",
        label: "Find previous",
        category: "Find",
        default: Some("secondary-shift-g"),
        context: Some("Insulator"),
    },
    KeybindingDef {
        id: "save_file",
        label: "Save file",
        category: "General",
        default: Some("secondary-s"),
        context: None,
    },
    KeybindingDef {
        id: "open_settings",
        label: "Open settings",
        category: "General",
        default: Some("secondary-,"),
        context: None,
    },
    KeybindingDef {
        id: "navigate_back",
        label: "Navigate back",
        category: "General",
        default: Some("secondary-["),
        context: Some("Insulator"),
    },
    KeybindingDef {
        id: "navigate_forward",
        label: "Navigate forward",
        category: "General",
        default: None,
        context: Some("Insulator"),
    },
];

/// Look up a definition by id.
pub fn def(id: &str) -> Option<&'static KeybindingDef> {
    ALL.iter().find(|def| def.id == id)
}

/// The chord currently in effect: override when present, else the default.
/// Empty string means unbound.
pub fn effective(keybindings: &HashMap<String, String>, id: &str) -> String {
    if let Some(custom) = keybindings.get(id) {
        return custom.clone();
    }
    def(id)
        .and_then(|def| def.default)
        .unwrap_or_default()
        .to_owned()
}

/// Whether the id currently differs from its default.
pub fn is_customized(keybindings: &HashMap<String, String>, id: &str) -> bool {
    match keybindings.get(id) {
        Some(custom) => *custom != def(id).and_then(|def| def.default).unwrap_or_default(),
        None => false,
    }
}

/// Ids whose effective chord equals `chord` (excluding `except_id`).
/// Used for conflict warnings.
pub fn conflicts(
    keybindings: &HashMap<String, String>,
    chord: &str,
    except_id: &str,
) -> Vec<&'static KeybindingDef> {
    if chord.is_empty() {
        return Vec::new();
    }
    ALL.iter()
        .filter(|def| def.id != except_id && effective(keybindings, def.id) == chord)
        .collect()
}

/// Portable chord from a captured keystroke. `None` when the press should not
/// become a shortcut (plain typing with no modifier).
pub fn chord_from_keystroke(keystroke: &gpui::Keystroke) -> Option<String> {
    let modifiers = &keystroke.modifiers;
    let has_modifier = modifiers.platform
        || modifiers.control
        || modifiers.alt
        || modifiers.function
        || (modifiers.shift && is_named_key(&keystroke.key));
    if !has_modifier {
        return None;
    }
    let mut parts: Vec<&str> = Vec::with_capacity(5);
    if cfg!(target_os = "macos") {
        if modifiers.platform {
            parts.push("secondary");
        }
        if modifiers.control {
            parts.push("ctrl");
        }
    } else {
        if modifiers.control {
            parts.push("secondary");
        }
        if modifiers.platform {
            parts.push("super");
        }
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    if modifiers.function {
        parts.push("fn");
    }
    let mut chord = parts.join("-");
    if !chord.is_empty() {
        chord.push('-');
    }
    chord.push_str(&keystroke.key.to_lowercase());
    gpui::Keystroke::parse(&chord).ok()?;
    Some(chord)
}

/// Whether the key is a named (non-printable) key that may carry Shift.
fn is_named_key(key: &str) -> bool {
    key.len() != 1
}

/// Human-readable shortcut label for the current platform.
pub fn display(chord: &str) -> String {
    if chord.is_empty() {
        return String::new();
    }
    let macos = cfg!(target_os = "macos");
    let mut modifiers: Vec<&str> = Vec::new();
    let mut key: &str = chord;
    for token in chord.split('-') {
        match token.to_lowercase().as_str() {
            "secondary" if macos => modifiers.push("⌘"),
            "secondary" => modifiers.push("Ctrl"),
            "cmd" | "super" | "win" if macos => modifiers.push("⌘"),
            "cmd" | "super" | "win" => modifiers.push("Win"),
            "ctrl" if macos => modifiers.push("⌃"),
            "ctrl" => modifiers.push("Ctrl"),
            "alt" if macos => modifiers.push("⌥"),
            "alt" => modifiers.push("Alt"),
            "shift" if macos => modifiers.push("⇧"),
            "shift" => modifiers.push("Shift"),
            "fn" => modifiers.push("Fn"),
            _ => {
                key = token;
                break;
            }
        }
    }
    // `split('-')` consumes the key as the first non-modifier token; recover
    // the full key when the chord itself ends in a dash (the "-" key).
    if key.is_empty() {
        key = "-";
    }
    let pretty_key = match key.to_lowercase().as_str() {
        "escape" => "Esc".to_owned(),
        "enter" => "⏎".to_owned(),
        "tab" => "Tab".to_owned(),
        "space" => "Space".to_owned(),
        "," | "/" | "[" | "]" => key.to_owned(),
        _ if key.len() == 1 => key.to_uppercase(),
        _ => {
            let mut chars = key.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    };
    if macos {
        format!("{}{}", modifiers.concat(), pretty_key)
    } else if modifiers.is_empty() {
        pretty_key
    } else {
        format!("{}+{}", modifiers.join("+"), pretty_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_falls_back_to_default() {
        let map = HashMap::new();
        assert_eq!(effective(&map, "new_chat"), "secondary-n");
        assert_eq!(effective(&map, "next_main_tab"), "");
    }

    #[test]
    fn display_formats_secondary_chords() {
        assert!(!display("secondary-n").is_empty());
        assert_eq!(display(""), "");
    }

    #[test]
    fn conflicts_detects_duplicates() {
        let mut map = HashMap::new();
        map.insert("new_chat".to_owned(), "secondary-k".to_owned());
        let found = conflicts(&map, "secondary-k", "command_palette");
        assert!(found.iter().any(|def| def.id == "new_chat"));
        assert!(conflicts(&map, "", "new_chat").is_empty());
    }
}
