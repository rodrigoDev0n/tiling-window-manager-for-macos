//! Command parsing and dispatch.
//!
//! Commands only decide *what* to do; the actual native work is delegated to
//! the `platform` layer.

use crate::core::window::WindowInfo;
use crate::error::AppError;
use crate::platform::macos::{accessibility, windows};

/// Executes the command described by `args` (excluding the program name).
pub fn run(args: &[String]) -> Result<(), AppError> {
    match args.first().map(String::as_str) {
        Some("permissions") => permissions(),
        Some("windows") => list_windows(),
        Some("-h" | "--help") | None => {
            print_usage();
            Ok(())
        }
        Some(unknown) => Err(AppError::UnknownCommand(unknown.to_string())),
    }
}

fn print_usage() {
    println!("Usage: tiling_window <command>");
    println!();
    println!("Commands:");
    println!("  permissions   Report Accessibility authorization status.");
    println!("  windows       List accessible windows of running applications.");
    println!("  -h, --help    Show this help message.");
}

fn permissions() -> Result<(), AppError> {
    if accessibility::is_trusted() {
        println!("Accessibility permission: granted");
    } else {
        println!("Accessibility permission: NOT granted");
        println!();
        println!("{}", accessibility::PERMISSION_INSTRUCTIONS);
    }
    Ok(())
}

fn list_windows() -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    let discovered = windows::discover_windows();
    if discovered.is_empty() {
        println!("No accessible windows found.");
        return Ok(());
    }

    print!("{}", render_table(&discovered));
    Ok(())
}

fn render_table(windows: &[WindowInfo]) -> String {
    let headers = ["PID", "APPLICATION", "TITLE", "X", "Y", "WIDTH", "HEIGHT"];

    let rows: Vec<[String; 7]> = windows
        .iter()
        .map(|window| {
            let (x, y, width, height) = match window.bounds {
                Some(bounds) => (
                    format!("{:.0}", bounds.origin.x),
                    format!("{:.0}", bounds.origin.y),
                    format!("{:.0}", bounds.size.width),
                    format!("{:.0}", bounds.size.height),
                ),
                None => (
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                ),
            };
            [
                window.pid.0.to_string(),
                window.application.clone(),
                window
                    .title
                    .clone()
                    .unwrap_or_else(|| "(untitled)".to_string()),
                x,
                y,
                width,
                height,
            ]
        })
        .collect();

    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    let mut output = String::new();
    output.push_str(&format_row(&headers.map(String::from), &widths));
    output.push_str(&format_separator(&widths));
    for row in &rows {
        output.push_str(&format_row(row, &widths));
    }
    output
}

fn format_row(cells: &[String; 7], widths: &[usize]) -> String {
    let mut line = String::new();
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            line.push_str("  ");
        }
        line.push_str(cell);
        let padding = widths[index].saturating_sub(cell.chars().count());
        line.push_str(&" ".repeat(padding));
    }
    let mut line = line.trim_end().to_string();
    line.push('\n');
    line
}

fn format_separator(widths: &[usize]) -> String {
    let segments: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
    format!("{}\n", segments.join("  "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geometry::Rect;
    use crate::core::window::ProcessId;

    fn sample(bounds: Option<Rect>, title: Option<&str>) -> WindowInfo {
        WindowInfo {
            pid: ProcessId(1234),
            application: "Example".to_string(),
            title: title.map(str::to_string),
            bounds,
        }
    }

    #[test]
    fn table_contains_headers_and_window_data() {
        let windows = vec![sample(
            Some(Rect::new(0.0, 25.0, 1440.0, 800.0)),
            Some("Editor"),
        )];
        let table = render_table(&windows);

        assert!(table.contains("PID"));
        assert!(table.contains("APPLICATION"));
        assert!(table.contains("Example"));
        assert!(table.contains("Editor"));
        assert!(table.contains("1440"));
        assert!(table.contains("800"));
    }

    #[test]
    fn missing_geometry_renders_placeholders() {
        let windows = vec![sample(None, None)];
        let table = render_table(&windows);
        assert!(table.contains("(untitled)"));
        assert!(table.contains('-'));
    }

    #[test]
    fn empty_window_list_renders_only_headers() {
        let table = render_table(&[]);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("PID"));
    }
}
