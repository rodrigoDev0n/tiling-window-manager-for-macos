//! Platform-independent shortcut reference model (SPEC-007, SPEC-008).
//!
//! This module is the single source of truth for the shortcut set: the
//! [`Binding`] table drives both the overlay reference (`default_shortcuts`)
//! and the registered global hotkeys, so the displayed list and the executed
//! actions never diverge. The module contains no AppKit or Carbon types.

use crate::core::action::{Action, Direction};
use crate::core::layout::LayoutType;

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

/// A logical key, independent of a specific keyboard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character key (for example `'m'` or `'-'`).
    Char(char),
    /// Arrow keys.
    Up,
    Down,
    Left,
    Right,
}

/// The non-character modifier set of a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub command: bool,
    pub option: bool,
    pub shift: bool,
}

impl Modifiers {
    /// `⌘⌥`.
    pub const fn command_option() -> Self {
        Self {
            command: true,
            option: true,
            shift: false,
        }
    }

    /// `⌘⌥⇧`.
    pub const fn command_option_shift() -> Self {
        Self {
            command: true,
            option: true,
            shift: true,
        }
    }
}

/// One bindable shortcut: the logical key, modifiers, executed action, and the
/// description shown in the overlay reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub modifiers: Modifiers,
    pub action: Action,
    pub description: String,
}

/// The default, functional shortcut set (SPEC-008 FR-008).
///
/// Only actions this build can execute are listed; roadmap-only entries (BSP,
/// floating, swap-with-master, quit) are intentionally absent so the overlay
/// never advertises a shortcut that does nothing.
pub fn default_bindings() -> Vec<Binding> {
    let cmd_opt = Modifiers::command_option();
    let cmd_opt_shift = Modifiers::command_option_shift();
    vec![
        Binding {
            key: Key::Char('-'),
            modifiers: cmd_opt,
            action: Action::ToggleOverlay,
            description: "Show or hide this overlay".to_string(),
        },
        Binding {
            key: Key::Char('m'),
            modifiers: cmd_opt,
            action: Action::SetLayout(LayoutType::Monocle),
            description: "Monocle layout".to_string(),
        },
        Binding {
            key: Key::Char('s'),
            modifiers: cmd_opt,
            action: Action::SetLayout(LayoutType::MasterStack),
            description: "Master/Stack layout".to_string(),
        },
        Binding {
            key: Key::Char('c'),
            modifiers: cmd_opt,
            action: Action::SetLayout(LayoutType::Columns),
            description: "Columns layout".to_string(),
        },
        Binding {
            key: Key::Char('r'),
            modifiers: cmd_opt,
            action: Action::SetLayout(LayoutType::Rows),
            description: "Rows layout".to_string(),
        },
        Binding {
            key: Key::Char('q'),
            modifiers: cmd_opt,
            action: Action::SetLayout(LayoutType::Quad),
            description: "Quad layout (2x2)".to_string(),
        },
        Binding {
            key: Key::Left,
            modifiers: cmd_opt,
            action: Action::Focus(Direction::Left),
            description: "Move focus left".to_string(),
        },
        Binding {
            key: Key::Right,
            modifiers: cmd_opt,
            action: Action::Focus(Direction::Right),
            description: "Move focus right".to_string(),
        },
        Binding {
            key: Key::Up,
            modifiers: cmd_opt,
            action: Action::Focus(Direction::Up),
            description: "Move focus up".to_string(),
        },
        Binding {
            key: Key::Down,
            modifiers: cmd_opt,
            action: Action::Focus(Direction::Down),
            description: "Move focus down".to_string(),
        },
        Binding {
            key: Key::Left,
            modifiers: cmd_opt_shift,
            action: Action::MoveWindow(Direction::Left),
            description: "Move window left".to_string(),
        },
        Binding {
            key: Key::Right,
            modifiers: cmd_opt_shift,
            action: Action::MoveWindow(Direction::Right),
            description: "Move window right".to_string(),
        },
    ]
}

/// Renders the key combination of `binding` as a display string, for example
/// `⌘⌥M` or `⌘⌥⇧←`.
pub fn format_binding(binding: &Binding) -> String {
    let mut keys = String::new();
    if binding.modifiers.command {
        keys.push('⌘');
    }
    if binding.modifiers.option {
        keys.push('⌥');
    }
    if binding.modifiers.shift {
        keys.push('⇧');
    }
    keys.push_str(&format_key(binding.key));
    keys
}

fn format_key(key: Key) -> String {
    match key {
        Key::Char(character) => character.to_ascii_uppercase().to_string(),
        Key::Up => "↑".to_string(),
        Key::Down => "↓".to_string(),
        Key::Left => "←".to_string(),
        Key::Right => "→".to_string(),
    }
}

/// Returns the overlay shortcut reference grouped by purpose, derived from
/// [`default_bindings`].
pub fn default_shortcuts() -> Vec<ShortcutGroup> {
    let bindings = default_bindings();
    vec![
        shortcut_group(
            "Overlay",
            bindings
                .iter()
                .filter(|b| b.action == Action::ToggleOverlay),
        ),
        shortcut_group(
            "Layouts",
            bindings
                .iter()
                .filter(|b| matches!(b.action, Action::SetLayout(_))),
        ),
        shortcut_group(
            "Move and fullscreen",
            bindings
                .iter()
                .filter(|b| matches!(b.action, Action::ToggleMoveMode | Action::ToggleFullscreen)),
        ),
        shortcut_group(
            "Focus and Windows",
            bindings
                .iter()
                .filter(|b| matches!(b.action, Action::Focus(_) | Action::MoveWindow(_))),
        ),
    ]
}

fn shortcut_group<'a>(title: &str, bindings: impl Iterator<Item = &'a Binding>) -> ShortcutGroup {
    ShortcutGroup {
        title: title.to_string(),
        shortcuts: bindings
            .map(|binding| Shortcut {
                keys: format_binding(binding),
                description: binding.description.clone(),
            })
            .collect(),
    }
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
    fn default_bindings_cover_the_action_families() {
        let bindings = default_bindings();
        assert_eq!(
            bindings
                .iter()
                .filter(|b| b.action == Action::ToggleOverlay)
                .count(),
            1
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|b| matches!(b.action, Action::SetLayout(_)))
                .count(),
            5
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|b| matches!(b.action, Action::Focus(_)))
                .count(),
            4
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|b| matches!(b.action, Action::MoveWindow(_)))
                .count(),
            2
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|b| b.action == Action::ToggleMoveMode)
                .count(),
            1
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|b| b.action == Action::ToggleFullscreen)
                .count(),
            1
        );
    }

    #[test]
    fn default_bindings_have_no_duplicate_key_combinations() {
        let bindings = default_bindings();
        let mut combos: Vec<(&Key, Modifiers)> =
            bindings.iter().map(|b| (&b.key, b.modifiers)).collect();
        combos.sort_by(|a, b| format!("{:?}{:?}", a.0, a.1).cmp(&format!("{:?}{:?}", b.0, b.1)));
        for pair in combos.windows(2) {
            assert_ne!(pair[0], pair[1], "duplicate key combination");
        }
    }

    #[test]
    fn overlay_entries_are_derived_from_bindings() {
        let bindings = default_bindings();
        let overlay_entries = default_shortcuts()
            .into_iter()
            .flat_map(|group| group.shortcuts);

        let expected: std::collections::HashSet<(String, String)> = bindings
            .iter()
            .map(|b| (format_binding(b), b.description.clone()))
            .collect();
        let actual: std::collections::HashSet<(String, String)> =
            overlay_entries.map(|s| (s.keys, s.description)).collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn format_binding_renders_modifier_glyphs() {
        let binding = Binding {
            key: Key::Char('m'),
            modifiers: Modifiers::command_option(),
            action: Action::SetLayout(LayoutType::Monocle),
            description: String::new(),
        };
        assert_eq!(format_binding(&binding), "⌘⌥M");

        let shift = Binding {
            key: Key::Left,
            modifiers: Modifiers::command_option_shift(),
            action: Action::MoveWindow(Direction::Left),
            description: String::new(),
        };
        assert_eq!(format_binding(&shift), "⌘⌥⇧←");
    }

    #[test]
    fn default_set_contains_the_overlay_toggle() {
        let groups = default_shortcuts();
        let has_toggle = groups
            .iter()
            .flat_map(|g| &g.shortcuts)
            .any(|s| s.keys == "⌘⌥-");
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
