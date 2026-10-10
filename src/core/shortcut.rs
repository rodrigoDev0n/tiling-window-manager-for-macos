//! Platform-independent shortcut reference model (SPEC-007).
//!
//! The shortcut list is informational data consumed by the overlay renderer.
//! No shortcuts described here are executed by this module; execution is out
//! of scope for SPEC-007.

/// A single display-ready keyboard shortcut entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// Human-readable key combination, e.g. "⌘⌥M".
    pub keys: String,
    /// Short description of the action.
    pub description: String,
}

/// A titled group of shortcuts, used to organize the reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub title: String,
    pub shortcuts: Vec<Shortcut>,
}

/// Returns the default shortcut reference set.
///
/// The set is informational and reflects the intended roadmap. Only the
/// overlay toggle (`⌘⌥/`) is functional in SPEC-007.
pub fn default_shortcuts() -> Vec<ShortcutGroup> {
    vec![
        ShortcutGroup {
            title: "Overlay".to_string(),
            shortcuts: vec![Shortcut {
                keys: "⌘⌥/".to_string(),
                description: "Show or hide this overlay".to_string(),
            }],
        },
        ShortcutGroup {
            title: "Layouts".to_string(),
            shortcuts: vec![
                Shortcut {
                    keys: "⌘⌥M".to_string(),
                    description: "Monocle layout".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥S".to_string(),
                    description: "Master/Stack layout".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥C".to_string(),
                    description: "Columns layout".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥R".to_string(),
                    description: "Rows layout".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥B".to_string(),
                    description: "BSP layout".to_string(),
                },
            ],
        },
        ShortcutGroup {
            title: "Focus and Windows".to_string(),
            shortcuts: vec![
                Shortcut {
                    keys: "⌘⌥←/→/↑/↓".to_string(),
                    description: "Move focus".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥⇧←/→".to_string(),
                    description: "Move window".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥Return".to_string(),
                    description: "Swap with master".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥F".to_string(),
                    description: "Toggle floating".to_string(),
                },
                Shortcut {
                    keys: "⌘⌥Q".to_string(),
                    description: "Quit".to_string(),
                },
            ],
        },
    ]
}

/// Renders the groups as monospaced-friendly plain text, one row per shortcut.
///
/// Key combinations are padded to a shared column so the rows align when
/// rendered with a monospaced font.
pub fn render_plain(groups: &[ShortcutGroup]) -> String {
    let max_keys = groups
        .iter()
        .flat_map(|group| group.shortcuts.iter())
        .map(|shortcut| shortcut.keys.chars().count())
        .max()
        .unwrap_or(0);

    let mut lines: Vec<String> = Vec::new();
    for group in groups {
        if !group.shortcuts.is_empty() {
            lines.push(format!("{}:", group.title));
            for shortcut in &group.shortcuts {
                let padding = " ".repeat(max_keys.saturating_sub(shortcut.keys.chars().count()));
                lines.push(format!(
                    "{}  {}  {}",
                    shortcut.keys, padding, shortcut.description
                ));
            }
            lines.push(String::new());
        }
    }
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_set_is_non_empty_and_grouped() {
        let groups = default_shortcuts();
        assert!(!groups.is_empty());
        assert!(groups.iter().all(|group| !group.shortcuts.is_empty()));
    }

    #[test]
    fn default_set_contains_the_overlay_toggle() {
        let groups = default_shortcuts();
        let has_toggle = groups
            .iter()
            .flat_map(|g| &g.shortcuts)
            .any(|s| s.keys == "⌘⌥/");
        assert!(has_toggle);
    }

    #[test]
    fn render_pads_keys_to_a_shared_column() {
        let groups = default_shortcuts();
        let text = render_plain(&groups);
        let max_keys = groups
            .iter()
            .flat_map(|group| &group.shortcuts)
            .map(|shortcut| shortcut.keys.chars().count())
            .max()
            .unwrap_or(0);

        for group in &groups {
            for shortcut in &group.shortcuts {
                let padding = " ".repeat(max_keys - shortcut.keys.chars().count());
                let expected = format!("{}  {}  {}", shortcut.keys, padding, shortcut.description);
                assert!(
                    text.lines().any(|line| line == expected),
                    "missing padded row: {expected:?}"
                );
            }
        }
    }

    #[test]
    fn render_strips_trailing_blank_line() {
        let groups = vec![ShortcutGroup {
            title: "Only".to_string(),
            shortcuts: vec![Shortcut {
                keys: "⌘A".to_string(),
                description: "Test".to_string(),
            }],
        }];
        let text = render_plain(&groups);
        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn empty_groups_render_empty() {
        assert_eq!(render_plain(&[]), String::new());
    }
}
