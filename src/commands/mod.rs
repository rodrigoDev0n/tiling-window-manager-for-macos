//! Command parsing and dispatch.
//!
//! Commands only decide *what* to do; the actual native work is delegated to
//! the `platform` layer.

use crate::core::geometry::Rect;
use crate::core::layout::{self, Shortcut};
use crate::core::selector::{self, SelectionError, WindowFilter, WindowSelector};
use crate::core::window::{ProcessId, WindowInfo};
use crate::error::AppError;
use crate::platform::macos::accessibility;
use crate::platform::macos::{control, displays, windows};

/// Executes the command described by `args` (excluding the program name).
pub fn run(args: &[String]) -> Result<(), AppError> {
    match args.first().map(String::as_str) {
        Some("permissions") => permissions(),
        Some("windows") => list_windows(),
        Some("move") => move_window(&args[1..]),
        Some("focus") => focus_window(&args[1..]),
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
    println!("  move          Move/resize a window to an absolute or predefined geometry.");
    println!("  focus         Focus (raise) a window.");
    println!("  -h, --help    Show this help message.");
    println!();
    println!("Selectors (default: --focused):");
    println!("  --focused             The focused window of the frontmost application.");
    println!("  --pid <PID>           Restrict to a process.");
    println!("  --app <NAME>          Restrict to an application name.");
    println!("  --title <SUBSTRING>   Restrict to a title substring.");
    println!("  --index <N>           Pick the N-th match in a deterministic order.");
    println!();
    println!("move geometry:");
    println!("  --x <f64> --y <f64> --width <f64> --height <f64>");
    println!("  --to <left|right|maximize|center>");
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

/// A geometry request supplied on the command line.
#[derive(Debug)]
enum Geometry {
    Absolute(Rect),
    Shortcut(Shortcut),
}

/// Parsed command-line selection and geometry.
#[derive(Debug)]
struct ParsedArgs {
    selector: WindowSelector,
    geometry: Option<Geometry>,
}

fn move_window(args: &[String]) -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    let parsed = parse_args(args)?;
    let geometry = parsed.geometry.ok_or_else(|| {
        AppError::InvalidArgument(
            "move requires --x/--y/--width/--height or --to <left|right|maximize|center>"
                .to_string(),
        )
    })?;

    let (info, handle) = resolve_managed(&parsed.selector)?;

    let target = match geometry {
        Geometry::Absolute(rect) => rect,
        Geometry::Shortcut(shortcut) => {
            let current = info.bounds.ok_or(AppError::UnsupportedWindow)?;
            let work_area = displays::work_area_for_window(current).ok_or_else(|| {
                AppError::ControlFailed("could not determine the screen work area".to_string())
            })?;
            layout::shortcut_rect(shortcut, work_area, current).ok_or_else(|| {
                AppError::ControlFailed("could not compute the target geometry".to_string())
            })?
        }
    };

    let applied = control::set_bounds(&handle, target).map_err(map_control_error)?;

    println!(
        "Moved {} (PID {}): x={:.0} y={:.0} width={:.0} height={:.0}",
        display_title(&info),
        info.pid.0,
        applied.origin.x,
        applied.origin.y,
        applied.size.width,
        applied.size.height
    );
    Ok(())
}

fn focus_window(args: &[String]) -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    let parsed = parse_args(args)?;
    if parsed.geometry.is_some() {
        return Err(AppError::InvalidArgument(
            "focus does not accept geometry arguments".to_string(),
        ));
    }

    let (info, handle) = resolve_managed(&parsed.selector)?;
    control::focus_window(&handle, info.pid).map_err(map_control_error)?;

    println!("Focused {} (PID {})", display_title(&info), info.pid.0);
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, AppError> {
    let mut focused = false;
    let mut filter = WindowFilter::default();
    let mut x = None;
    let mut y = None;
    let mut width = None;
    let mut height = None;
    let mut shortcut = None;

    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--focused" => {
                focused = true;
                index += 1;
            }
            "--pid" => {
                let value = flag_value(args, index, flag)?;
                let pid = value.parse::<i32>().map_err(|_| {
                    AppError::InvalidArgument(format!("invalid --pid value: {value}"))
                })?;
                filter.pid = Some(ProcessId(pid));
                index += 2;
            }
            "--app" => {
                filter.application = Some(flag_value(args, index, flag)?.to_string());
                index += 2;
            }
            "--title" => {
                filter.title = Some(flag_value(args, index, flag)?.to_string());
                index += 2;
            }
            "--index" => {
                let value = flag_value(args, index, flag)?;
                let parsed = value.parse::<usize>().map_err(|_| {
                    AppError::InvalidArgument(format!("invalid --index value: {value}"))
                })?;
                filter.index = Some(parsed);
                index += 2;
            }
            "--x" => {
                x = Some(flag_f64(args, index, flag)?);
                index += 2;
            }
            "--y" => {
                y = Some(flag_f64(args, index, flag)?);
                index += 2;
            }
            "--width" => {
                width = Some(flag_f64(args, index, flag)?);
                index += 2;
            }
            "--height" => {
                height = Some(flag_f64(args, index, flag)?);
                index += 2;
            }
            "--to" => {
                let value = flag_value(args, index, flag)?;
                shortcut = Some(Shortcut::parse(value).ok_or_else(|| {
                    AppError::InvalidArgument(format!(
                        "invalid --to value: {value} (expected left, right, maximize or center)"
                    ))
                })?);
                index += 2;
            }
            other => {
                return Err(AppError::InvalidArgument(format!(
                    "unexpected argument: {other}"
                )));
            }
        }
    }

    if focused && (filter.has_criteria() || filter.index.is_some()) {
        return Err(AppError::InvalidArgument(
            "--focused cannot be combined with other selectors".to_string(),
        ));
    }

    let selector = if focused {
        WindowSelector::Focused
    } else {
        WindowSelector::Filter(filter)
    };

    let geometry = build_geometry(x, y, width, height, shortcut)?;
    Ok(ParsedArgs { selector, geometry })
}

fn build_geometry(
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    shortcut: Option<Shortcut>,
) -> Result<Option<Geometry>, AppError> {
    if let Some(shortcut) = shortcut {
        if x.is_some() || y.is_some() || width.is_some() || height.is_some() {
            return Err(AppError::InvalidArgument(
                "--to cannot be combined with --x/--y/--width/--height".to_string(),
            ));
        }
        return Ok(Some(Geometry::Shortcut(shortcut)));
    }

    match (x, y, width, height) {
        (None, None, None, None) => Ok(None),
        (Some(x), Some(y), Some(width), Some(height)) => {
            let rect = Rect::new(x, y, width, height);
            if rect.is_valid() {
                Ok(Some(Geometry::Absolute(rect)))
            } else {
                Err(AppError::InvalidArgument(
                    "width and height must be positive".to_string(),
                ))
            }
        }
        _ => Err(AppError::InvalidArgument(
            "provide all of --x, --y, --width and --height".to_string(),
        )),
    }
}

fn flag_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, AppError> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| AppError::InvalidArgument(format!("{flag} requires a value")))
}

fn flag_f64(args: &[String], index: usize, flag: &str) -> Result<f64, AppError> {
    let value = flag_value(args, index, flag)?;
    let parsed = value
        .parse::<f64>()
        .map_err(|_| AppError::InvalidArgument(format!("invalid {flag} value: {value}")))?;
    if !parsed.is_finite() {
        return Err(AppError::InvalidArgument(format!("{flag} must be finite")));
    }
    Ok(parsed)
}

fn resolve_managed(
    selector: &WindowSelector,
) -> Result<(WindowInfo, control::WindowHandle), AppError> {
    match selector {
        WindowSelector::Focused => {
            let window = control::focused_window().ok_or(AppError::NoFocusedWindow)?;
            Ok((window.info, window.handle))
        }
        WindowSelector::Filter(filter) => {
            let managed = control::discover_managed_windows();
            let infos: Vec<WindowInfo> = managed.iter().map(|window| window.info.clone()).collect();
            let index = selector::select_index(&infos, filter).map_err(map_selection_error)?;
            let window = managed
                .into_iter()
                .nth(index)
                .ok_or(AppError::WindowNotFound)?;
            Ok((window.info, window.handle))
        }
    }
}

fn map_selection_error(error: SelectionError) -> AppError {
    match error {
        SelectionError::NotFound => AppError::WindowNotFound,
        SelectionError::Ambiguous { candidates } => AppError::AmbiguousSelection(candidates),
        SelectionError::IndexOutOfRange { index, len } => AppError::InvalidArgument(format!(
            "index {index} is out of range for {len} matched window(s)"
        )),
    }
}

fn map_control_error(error: control::ControlError) -> AppError {
    if error.is_unsupported() {
        AppError::UnsupportedWindow
    } else {
        AppError::ControlFailed(error.to_string())
    }
}

fn display_title(info: &WindowInfo) -> String {
    info.title
        .clone()
        .unwrap_or_else(|| info.application.clone())
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

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn parses_absolute_geometry() {
        let parsed = parse_args(&args(&[
            "--pid", "42", "--x", "1", "--y", "2", "--width", "3", "--height", "4",
        ]))
        .expect("parse");
        match parsed.geometry {
            Some(Geometry::Absolute(rect)) => {
                assert_eq!(rect, Rect::new(1.0, 2.0, 3.0, 4.0));
            }
            _ => panic!("expected absolute geometry"),
        }
        match parsed.selector {
            WindowSelector::Filter(filter) => assert_eq!(filter.pid, Some(ProcessId(42))),
            _ => panic!("expected filter selector"),
        }
    }

    #[test]
    fn parses_shortcut() {
        let parsed = parse_args(&args(&["--to", "left"])).expect("parse");
        assert!(matches!(
            parsed.geometry,
            Some(Geometry::Shortcut(Shortcut::Left))
        ));
    }

    #[test]
    fn rejects_partial_geometry() {
        let error = parse_args(&args(&["--x", "1", "--y", "2"])).expect_err("should fail");
        assert!(matches!(error, AppError::InvalidArgument(_)));
    }

    #[test]
    fn rejects_shortcut_with_absolute() {
        let error = parse_args(&args(&[
            "--to", "left", "--x", "1", "--y", "2", "--width", "3", "--height", "4",
        ]))
        .expect_err("should fail");
        assert!(matches!(error, AppError::InvalidArgument(_)));
    }

    #[test]
    fn rejects_focused_with_filter() {
        let error = parse_args(&args(&["--focused", "--pid", "1"])).expect_err("should fail");
        assert!(matches!(error, AppError::InvalidArgument(_)));
    }

    #[test]
    fn rejects_non_positive_dimensions() {
        let error = parse_args(&args(&[
            "--x", "1", "--y", "2", "--width", "0", "--height", "4",
        ]))
        .expect_err("should fail");
        assert!(matches!(error, AppError::InvalidArgument(_)));
    }
}
