//! Platform-independent window selection.
//!
//! A [`WindowFilter`] matches discovered windows by stable attributes and,
//! optionally, by position within a deterministic ordering. Selection never
//! relies on identifiers persisted across process executions.

use std::cmp::Ordering;
use std::fmt;

use crate::core::geometry::Rect;
use crate::core::window::{ProcessId, WindowInfo};

/// How the user asked to select a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowSelector {
    /// The focused window of the frontmost application.
    Focused,
    /// A filter over the discovered windows.
    Filter(WindowFilter),
}

/// Criteria used to match windows. Unset criteria are ignored, so the fields
/// combine with a logical AND.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowFilter {
    pub pid: Option<ProcessId>,
    pub application: Option<String>,
    pub title: Option<String>,
    /// Zero-based index within the deterministically ordered matches.
    pub index: Option<usize>,
}

impl WindowFilter {
    /// Returns whether `filter` carries no matching criteria at all.
    pub fn has_criteria(&self) -> bool {
        self.pid.is_some() || self.application.is_some() || self.title.is_some()
    }

    /// Returns whether `window` satisfies every set criterion.
    pub fn matches(&self, window: &WindowInfo) -> bool {
        self.pid.is_none_or(|pid| window.pid == pid)
            && self
                .application
                .as_ref()
                .is_none_or(|app| window.application.eq_ignore_ascii_case(app))
            && self
                .title
                .as_ref()
                .is_none_or(|needle| title_contains(window.title.as_deref(), needle))
    }
}

/// Minimal description of a window used to report ambiguity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowCandidate {
    pub pid: ProcessId,
    pub application: String,
    pub title: Option<String>,
}

impl From<&WindowInfo> for WindowCandidate {
    fn from(window: &WindowInfo) -> Self {
        Self {
            pid: window.pid,
            application: window.application.clone(),
            title: window.title.clone(),
        }
    }
}

/// Reasons a selection could not resolve to exactly one window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    /// No discovered window matched the filter.
    NotFound,
    /// More than one window matched and no index was provided.
    Ambiguous { candidates: Vec<WindowCandidate> },
    /// The provided index was outside the matched set.
    IndexOutOfRange { index: usize, len: usize },
}

impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectionError::NotFound => write!(f, "no window matched the selector"),
            SelectionError::Ambiguous { candidates } => {
                write!(f, "selector matched {} windows", candidates.len())
            }
            SelectionError::IndexOutOfRange { index, len } => {
                write!(
                    f,
                    "index {index} is out of range for {len} matched window(s)"
                )
            }
        }
    }
}

impl std::error::Error for SelectionError {}

/// Resolves the index (into `windows`) of the single window described by `filter`.
///
/// Matches are ordered by `(application, title, x, y, pid, width, height)` so
/// that `--index` is reproducible within an invocation.
pub fn select_index(
    windows: &[WindowInfo],
    filter: &WindowFilter,
) -> Result<usize, SelectionError> {
    let mut matched: Vec<usize> = windows
        .iter()
        .enumerate()
        .filter(|(_, window)| filter.matches(window))
        .map(|(index, _)| index)
        .collect();

    matched.sort_by(|&a, &b| compare_windows(&windows[a], &windows[b]));

    match filter.index {
        Some(index) => matched
            .get(index)
            .copied()
            .ok_or(SelectionError::IndexOutOfRange {
                index,
                len: matched.len(),
            }),
        None => match matched.as_slice() {
            [] => Err(SelectionError::NotFound),
            [only] => Ok(*only),
            _ => Err(SelectionError::Ambiguous {
                candidates: matched
                    .iter()
                    .map(|&index| WindowCandidate::from(&windows[index]))
                    .collect(),
            }),
        },
    }
}

fn title_contains(window_title: Option<&str>, needle: &str) -> bool {
    window_title.is_some_and(|title| title.to_lowercase().contains(&needle.to_lowercase()))
}

fn compare_windows(a: &WindowInfo, b: &WindowInfo) -> Ordering {
    a.application
        .cmp(&b.application)
        .then_with(|| title_key(a).cmp(title_key(b)))
        .then_with(|| {
            compare_coord(
                coord(a.bounds, |rect| rect.origin.x),
                coord(b.bounds, |rect| rect.origin.x),
            )
        })
        .then_with(|| {
            compare_coord(
                coord(a.bounds, |rect| rect.origin.y),
                coord(b.bounds, |rect| rect.origin.y),
            )
        })
        .then_with(|| a.pid.0.cmp(&b.pid.0))
        .then_with(|| {
            compare_coord(
                coord(a.bounds, |rect| rect.size.width),
                coord(b.bounds, |rect| rect.size.width),
            )
        })
        .then_with(|| {
            compare_coord(
                coord(a.bounds, |rect| rect.size.height),
                coord(b.bounds, |rect| rect.size.height),
            )
        })
}

fn title_key(window: &WindowInfo) -> &str {
    window.title.as_deref().unwrap_or("")
}

fn coord(bounds: Option<Rect>, project: impl Fn(&Rect) -> f64) -> f64 {
    bounds.map(|rect| project(&rect)).unwrap_or(f64::INFINITY)
}

fn compare_coord(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(pid: i32, app: &str, title: Option<&str>, bounds: Option<Rect>) -> WindowInfo {
        WindowInfo {
            pid: ProcessId(pid),
            application: app.to_string(),
            title: title.map(str::to_string),
            bounds,
        }
    }

    #[test]
    fn single_match_resolves() {
        let windows = vec![window(1, "A", Some("one"), None)];
        let filter = WindowFilter {
            pid: Some(ProcessId(1)),
            ..Default::default()
        };
        assert_eq!(select_index(&windows, &filter), Ok(0));
    }

    #[test]
    fn no_match_reports_not_found() {
        let windows = vec![window(1, "A", Some("one"), None)];
        let filter = WindowFilter {
            application: Some("B".to_string()),
            ..Default::default()
        };
        assert_eq!(
            select_index(&windows, &filter),
            Err(SelectionError::NotFound)
        );
    }

    #[test]
    fn multiple_matches_report_candidates() {
        let windows = vec![
            window(1, "A", Some("one"), None),
            window(1, "A", Some("two"), None),
        ];
        let filter = WindowFilter {
            pid: Some(ProcessId(1)),
            ..Default::default()
        };
        match select_index(&windows, &filter) {
            Err(SelectionError::Ambiguous { candidates }) => assert_eq!(candidates.len(), 2),
            other => panic!("expected ambiguity, got {other:?}"),
        }
    }

    #[test]
    fn application_filter_is_case_insensitive() {
        let windows = vec![window(1, "Safari", Some("x"), None)];
        let filter = WindowFilter {
            application: Some("safari".to_string()),
            ..Default::default()
        };
        assert_eq!(select_index(&windows, &filter), Ok(0));
    }

    #[test]
    fn title_filter_is_case_insensitive_substring() {
        let windows = vec![window(1, "A", Some("My Document"), None)];
        let filter = WindowFilter {
            title: Some("document".to_string()),
            ..Default::default()
        };
        assert_eq!(select_index(&windows, &filter), Ok(0));
    }

    #[test]
    fn ordering_is_deterministic_and_index_selects_stably() {
        let windows = vec![
            window(2, "B", Some("b"), Some(Rect::new(10.0, 0.0, 100.0, 100.0))),
            window(1, "A", Some("z"), Some(Rect::new(50.0, 0.0, 100.0, 100.0))),
            window(1, "A", Some("a"), Some(Rect::new(50.0, 0.0, 100.0, 100.0))),
        ];
        // Ordered by (app, title): A/a (index 2), A/z (index 1), B/b (index 0).
        let filter = WindowFilter {
            index: Some(0),
            ..Default::default()
        };
        assert_eq!(select_index(&windows, &filter), Ok(2));
        let filter = WindowFilter {
            index: Some(1),
            ..Default::default()
        };
        assert_eq!(select_index(&windows, &filter), Ok(1));
        let filter = WindowFilter {
            index: Some(2),
            ..Default::default()
        };
        assert_eq!(select_index(&windows, &filter), Ok(0));
    }

    #[test]
    fn index_out_of_range_is_reported() {
        let windows = vec![window(1, "A", Some("one"), None)];
        let filter = WindowFilter {
            index: Some(5),
            ..Default::default()
        };
        assert_eq!(
            select_index(&windows, &filter),
            Err(SelectionError::IndexOutOfRange { index: 5, len: 1 })
        );
    }

    #[test]
    fn empty_collection_reports_not_found() {
        let filter = WindowFilter::default();
        assert_eq!(select_index(&[], &filter), Err(SelectionError::NotFound));
    }
}
