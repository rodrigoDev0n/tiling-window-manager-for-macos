//! Command parsing and dispatch.
//!
//! Commands only decide *what* to do; the actual native work is delegated to
//! the `platform` layer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::app;
use crate::core::action::{Action, Direction};
use crate::core::display::{self, Display, DisplayId};
use crate::core::display_assignment;
use crate::core::event::{AppEvent, DomainEvent, WindowEvent, WindowKey};
use crate::core::geometry::Rect;
use crate::core::layout::{self, LayoutConfig, LayoutType, Shortcut};
use crate::core::layout_plan::{ExecutionReport, PlanError, WindowRef};
use crate::core::navigation;
use crate::core::selector::{self, SelectionError, WindowFilter, WindowSelector, compare_windows};
use crate::core::shortcut;
use crate::core::window::{ProcessId, WindowId, WindowInfo};
use crate::error::AppError;
use crate::platform::macos::accessibility;
use crate::platform::macos::{control, displays, hotkey, observers, overlay, windows};

/// Executes the command described by `args` (excluding the program name).
pub fn run(args: &[String]) -> Result<(), AppError> {
    match args.first().map(String::as_str) {
        Some("permissions") => permissions(),
        Some("windows") => list_windows(),
        Some("move") => move_window(&args[1..]),
        Some("focus") => focus_window(&args[1..]),
        Some("displays") => list_displays(),
        Some("display-info") => display_info(args.get(1).map(String::as_str)),
        Some("layout") => layout_preview(&args[1..]),
        Some("apply") => apply_layout(&args[1..]),
        Some("watch") => watch(),
        Some("overlay") => show_overlay(),
        Some("run") => run_manager(&args[1..]),
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
    println!("  permissions     Report Accessibility authorization status.");
    println!("  windows         List accessible windows of running applications.");
    println!("  move            Move/resize a window to an absolute or predefined geometry.");
    println!("  focus           Focus (raise) a window.");
    println!("  displays        List active displays and their geometry.");
    println!("  display-info    Show detailed geometry for one display.");
    println!("  layout          Preview a tiling layout without changing windows.");
    println!("  apply           Apply a tiling layout to windows on a display.");
    println!("  watch           Monitor window events until interrupted (Ctrl+C).");
    println!("  overlay         Show a translucent shortcut overlay (toggle: Cmd+Opt+/).");
    println!("  run [--no-auto] Foreground window manager: overlay, shortcuts, automatic tiling.");
    println!("  -h, --help      Show this help message.");
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
    println!();
    println!("display-info argument:");
    println!("  <display-id>    CGDirectDisplayID in decimal, or 0x-prefixed hex.");
    println!();
    println!("layout options:");
    println!("  --layout <masterstack|columns|rows|monocle>");
    println!("  --display <id>    Only windows on this display (default: main).");
    println!("  --outer-gap <f64>  --inner-gap <f64>");
    println!("  --ratio <f64>      --master-count <usize>");
    println!();
    println!("apply usage:");
    println!("  apply <master-stack|columns|rows|monocle> [--display <id>] [--dry-run]");
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

fn list_displays() -> Result<(), AppError> {
    let active = displays::snapshot();
    if active.is_empty() {
        println!("No active displays found.");
        return Ok(());
    }

    print!("{}", render_displays_table(&active));
    if let Some(main) = display::main_display(&active) {
        println!();
        println!(
            "Main display: {} ({})",
            main.id,
            main.name.as_deref().unwrap_or("(unknown)")
        );
    }
    Ok(())
}

fn display_info(id: Option<&str>) -> Result<(), AppError> {
    let raw = id.ok_or_else(|| {
        AppError::InvalidArgument("display-info requires a display id".to_string())
    })?;
    let parsed =
        DisplayId::parse(raw).map_err(|error| AppError::InvalidArgument(error.to_string()))?;

    let active = displays::snapshot();
    let display = display::find(&active, parsed).ok_or(AppError::DisplayNotFound(parsed.0))?;
    print!("{}", render_display_detail(display));
    Ok(())
}

/// Command-line overrides for a layout preview.
#[derive(Debug, Default)]
struct LayoutOptions {
    layout: Option<LayoutType>,
    display: Option<String>,
    outer_gap: Option<f64>,
    inner_gap: Option<f64>,
    ratio: Option<f64>,
    master_count: Option<usize>,
}

fn layout_preview(args: &[String]) -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    let options = parse_layout_args(args)?;

    let active = displays::snapshot();
    let display = match options.display.as_deref() {
        Some(raw) => {
            let id = DisplayId::parse(raw)
                .map_err(|error| AppError::InvalidArgument(error.to_string()))?;
            display::find(&active, id).ok_or(AppError::DisplayNotFound(id.0))?
        }
        None => display::main_display(&active).ok_or(AppError::DisplayNotFound(0))?,
    };

    let defaults = LayoutConfig::default();
    let config = LayoutConfig {
        layout: options.layout.unwrap_or(defaults.layout),
        outer_gap: options.outer_gap.unwrap_or(defaults.outer_gap),
        inner_gap: options.inner_gap.unwrap_or(defaults.inner_gap),
        master_ratio: options.ratio.unwrap_or(defaults.master_ratio),
        master_count: options.master_count.unwrap_or(defaults.master_count),
    };

    let managed = control::discover_managed_windows();
    let mut window_ids = Vec::with_capacity(managed.len());
    for (index, window) in managed.iter().enumerate() {
        if let Some(bounds) = window.info.bounds {
            let centre_x = bounds.origin.x + bounds.size.width / 2.0;
            let centre_y = bounds.origin.y + bounds.size.height / 2.0;
            if contains_point(display.usable, centre_x, centre_y) {
                window_ids.push(WindowId(index as u64));
            }
        }
    }

    let result =
        layout::calculate(&window_ids, display.usable, config).map_err(map_layout_error)?;

    if result.windows.is_empty() {
        println!("No accessible windows on display {}.", display.id);
        return Ok(());
    }

    let headers = ["#", "X", "Y", "WIDTH", "HEIGHT"];
    let rows: Vec<Vec<String>> = result
        .windows
        .iter()
        .map(|placement| {
            vec![
                placement.window.0.to_string(),
                format!("{:.0}", placement.rect.origin.x),
                format!("{:.0}", placement.rect.origin.y),
                format!("{:.0}", placement.rect.size.width),
                format!("{:.0}", placement.rect.size.height),
            ]
        })
        .collect();
    print!("{}", render_grid(&headers, &rows));
    println!();
    println!(
        "Layout: {} on display {} ({} window(s)). Read-only preview: no windows changed.",
        result.layout,
        display.id,
        result.windows.len()
    );
    Ok(())
}

fn parse_layout_args(args: &[String]) -> Result<LayoutOptions, AppError> {
    let mut options = LayoutOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--layout" => {
                let value = flag_value(args, index, "--layout")?;
                options.layout = Some(LayoutType::parse(value).ok_or_else(|| {
                    AppError::InvalidArgument(format!(
                        "invalid --layout value: {value} (expected masterstack, columns, rows or monocle)"
                    ))
                })?);
                index += 2;
            }
            "--display" => {
                options.display = Some(flag_value(args, index, "--display")?.to_string());
                index += 2;
            }
            "--outer-gap" => {
                options.outer_gap = Some(flag_f64(args, index, "--outer-gap")?);
                index += 2;
            }
            "--inner-gap" => {
                options.inner_gap = Some(flag_f64(args, index, "--inner-gap")?);
                index += 2;
            }
            "--ratio" => {
                options.ratio = Some(flag_f64(args, index, "--ratio")?);
                index += 2;
            }
            "--master-count" => {
                let value = flag_value(args, index, "--master-count")?;
                options.master_count = Some(value.parse::<usize>().map_err(|_| {
                    AppError::InvalidArgument(format!("invalid --master-count value: {value}"))
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
    Ok(options)
}

fn contains_point(rect: Rect, x: f64, y: f64) -> bool {
    x >= rect.origin.x
        && x < rect.origin.x + rect.size.width
        && y >= rect.origin.y
        && y < rect.origin.y + rect.size.height
}

fn map_layout_error(error: layout::LayoutError) -> AppError {
    AppError::InvalidArgument(error.to_string())
}

/// Native backend that moves/resizes real windows through `control::set_bounds`.
struct NativeBackend;

impl app::layout_executor::LayoutBackend<control::LayoutWindow> for NativeBackend {
    fn set_bounds(&self, window: &control::LayoutWindow, rect: Rect) -> Result<Rect, String> {
        control::set_bounds(&window.handle, rect).map_err(|error| error.to_string())
    }
}

fn apply_layout(args: &[String]) -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    let options = parse_apply_args(args)?;

    let active = displays::snapshot();
    let selected = match options.display.as_deref() {
        Some(raw) => {
            let id = DisplayId::parse(raw)
                .map_err(|error| AppError::InvalidArgument(error.to_string()))?;
            display::find(&active, id).ok_or(AppError::DisplayNotFound(id.0))?
        }
        None => display::main_display(&active).ok_or(AppError::DisplayNotFound(0))?,
    };

    let config = LayoutConfig {
        layout: options.layout,
        ..LayoutConfig::default()
    };

    let windows = control::discover_layout_windows();
    let plan = app::layout_planner::build_plan(&active, windows, selected.id, config)
        .map_err(map_plan_error)?;

    if options.dry_run {
        let report = app::layout_executor::execute(
            plan,
            &NativeBackend,
            app::layout_executor::ExecutionMode::DryRun,
        );
        print!("{}", render_execution_report(&report));
        println!("Read-only preview: no windows changed.");
        return Ok(());
    }

    let report = app::layout_executor::execute(
        plan,
        &NativeBackend,
        app::layout_executor::ExecutionMode::Apply,
    );
    print!("{}", render_execution_report(&report));
    Ok(())
}

/// Parsed options for the `apply` command.
#[derive(Debug)]
struct ApplyOptions {
    layout: LayoutType,
    display: Option<String>,
    dry_run: bool,
}

fn parse_apply_args(args: &[String]) -> Result<ApplyOptions, AppError> {
    let layout_name = args.first().ok_or_else(|| {
        AppError::InvalidArgument(
            "apply requires a layout name (master-stack, columns, rows or monocle)".to_string(),
        )
    })?;
    let layout = LayoutType::parse(layout_name).ok_or_else(|| {
        AppError::InvalidArgument(format!(
            "invalid apply layout: {layout_name} (expected master-stack, columns, rows or monocle)"
        ))
    })?;

    let mut display = None;
    let mut dry_run = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--display" => {
                display = Some(flag_value(args, index, "--display")?.to_string());
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            other => {
                return Err(AppError::InvalidArgument(format!(
                    "unexpected argument: {other}"
                )));
            }
        }
    }

    Ok(ApplyOptions {
        layout,
        display,
        dry_run,
    })
}

fn map_plan_error(error: PlanError) -> AppError {
    match error {
        PlanError::DisplayNotFound(id) => AppError::DisplayNotFound(id.0),
        PlanError::Layout(error) => AppError::InvalidArgument(error.to_string()),
    }
}
fn render_execution_report(report: &ExecutionReport) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "Layout: {} | Display: {}\n",
        report.layout, report.display_id
    ));
    output.push_str(&format!(
        "Discovered: {} | Eligible: {} | Applied: {} | Constrained: {} | Failed: {} | Skipped: {}",
        report.discovered,
        report.eligible,
        report.applied,
        report.constrained,
        report.failed,
        report.skipped
    ));
    if report.planned > 0 {
        output.push_str(&format!(" | Planned: {}", report.planned));
    }
    output.push('\n');

    for result in &report.results {
        output.push_str(&format!(
            "  {} [{}] ({}): {} -> {}\n",
            result.application,
            result.title.as_deref().unwrap_or("untitled"),
            result.state,
            format_rect(result.original),
            format_rect(result.target),
        ));
    }

    if !report.exclusions.is_empty() {
        output.push_str("Excluded:\n");
        for exclusion in &report.exclusions {
            output.push_str(&format!(
                "  {} ({}): {}\n",
                exclusion.title.as_deref().unwrap_or("untitled"),
                exclusion.application,
                exclusion.reason
            ));
        }
    }
    output
}

fn format_rect(rect: Rect) -> String {
    format!(
        "x={:.0} y={:.0} w={:.0} h={:.0}",
        rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
    )
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

    let rows: Vec<Vec<String>> = windows
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
            vec![
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

    render_grid(&headers, &rows)
}

fn render_displays_table(displays: &[Display]) -> String {
    let headers = [
        "ID", "MAIN", "NAME", "SCALE", "X", "Y", "WIDTH", "HEIGHT", "USABLE-X", "USABLE-Y",
        "USABLE-W", "USABLE-H",
    ];

    let rows: Vec<Vec<String>> = displays
        .iter()
        .map(|display| {
            vec![
                display.id.to_string(),
                yes_no(display.main),
                display
                    .name
                    .clone()
                    .unwrap_or_else(|| "(unknown)".to_string()),
                format!("{:.1}", display.scale),
                format!("{:.0}", display.bounds.origin.x),
                format!("{:.0}", display.bounds.origin.y),
                format!("{:.0}", display.bounds.size.width),
                format!("{:.0}", display.bounds.size.height),
                format!("{:.0}", display.usable.origin.x),
                format!("{:.0}", display.usable.origin.y),
                format!("{:.0}", display.usable.size.width),
                format!("{:.0}", display.usable.size.height),
            ]
        })
        .collect();

    render_grid(&headers, &rows)
}

fn render_display_detail(display: &Display) -> String {
    format!(
        "ID: {id}\n\
         Name: {name}\n\
         Main: {main}\n\
         Scale: {scale:.1}\n\
         Valid: {valid}\n\
         Bounds (AX points): x={bx:.0} y={by:.0} width={bw:.0} height={bh:.0}\n\
         Usable (AX points): x={ux:.0} y={uy:.0} width={uw:.0} height={uh:.0}\n",
        id = display.id,
        name = display.name.as_deref().unwrap_or("(unknown)"),
        main = if display.main { "yes" } else { "no" },
        scale = display.scale,
        valid = if display.is_consistent() { "yes" } else { "no" },
        bx = display.bounds.origin.x,
        by = display.bounds.origin.y,
        bw = display.bounds.size.width,
        bh = display.bounds.size.height,
        ux = display.usable.origin.x,
        uy = display.usable.origin.y,
        uw = display.usable.size.width,
        uh = display.usable.size.height,
    )
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn render_grid(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers
        .iter()
        .map(|header| header.chars().count())
        .collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    let mut output = String::new();
    let header_cells: Vec<String> = headers.iter().map(|header| header.to_string()).collect();
    output.push_str(&format_row(&header_cells, &widths));
    output.push_str(&format_separator(&widths));
    for row in rows {
        output.push_str(&format_row(row, &widths));
    }
    output
}

fn format_row(cells: &[String], widths: &[usize]) -> String {
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

/// Set by the SIGINT handler; cleared on Ctrl+C to stop a foreground loop.
///
/// Shared by the `watch` and `overlay` commands, which both hold the foreground
/// until interrupted.
static FOREGROUND_RUNNING: AtomicBool = AtomicBool::new(true);

/// Runs the foreground event monitor until interrupted with Ctrl+C.
fn watch() -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    FOREGROUND_RUNNING.store(true, Ordering::SeqCst);
    install_interrupt_handler();

    // The manager must live at a stable address for the duration of the run:
    // native callbacks are handed a pointer to it.
    let mut manager = Box::new(observers::ObserverManager::new());
    manager.subscribe_workspace();
    manager.sync_applications();

    let mut processor = app::event_processor::EventProcessor::default();
    let interval = processor.coalesce_interval().as_secs_f64();

    println!("Watching for window events. Press Ctrl+C to stop.");

    // Register initial observers and seed the registry from a first snapshot.
    watch_pump(&mut manager, &mut processor);

    while FOREGROUND_RUNNING.load(Ordering::SeqCst) {
        observers::run_loop_step(interval);
        watch_pump(&mut manager, &mut processor);
    }

    for event in processor.flush(Instant::now()) {
        report_event(&event);
    }
    report_processor_diagnostics(processor.diagnostics(), processor.registry().len());
    report_observer_diagnostics(manager.diagnostics());
    manager.shutdown();
    println!("Stopped.");
    Ok(())
}

/// Drains queued native events, updates observers, and reports transitions.
fn watch_pump(
    manager: &mut observers::ObserverManager,
    processor: &mut app::event_processor::EventProcessor,
) {
    let now = Instant::now();
    let mut reconcile = false;

    for event in manager.drain_events() {
        match &event {
            DomainEvent::App {
                event: AppEvent::Launched { pid, name },
                ..
            } => {
                if let Err(error) = manager.register_application(*pid, name) {
                    eprintln!(
                        "observer registration failed for {name} (pid {}): {error}",
                        pid.0
                    );
                }
                reconcile = true;
            }
            DomainEvent::App {
                event: AppEvent::Terminated { pid },
                ..
            } => {
                manager.remove_application(*pid);
                reconcile = true;
            }
            _ => {}
        }

        for transition in processor.process(event, now) {
            report_event(&transition);
        }
    }

    for transition in processor.flush(now) {
        report_event(&transition);
    }

    if reconcile {
        for transition in processor.reconcile(manager.snapshot(), now) {
            report_event(&transition);
        }
    }
}

/// Prints a normalized event in the documented `watch` format.
fn report_event(event: &DomainEvent) {
    let clock = format_clock();
    match event {
        DomainEvent::App { event, .. } => match event {
            AppEvent::Launched { pid, name } => {
                println!("[{clock}] APPLICATION_LAUNCHED pid={} name={name}", pid.0);
            }
            AppEvent::Terminated { pid } => {
                println!("[{clock}] APPLICATION_TERMINATED pid={}", pid.0);
            }
            AppEvent::Activated { pid } => {
                println!("[{clock}] APPLICATION_ACTIVATED pid={}", pid.0);
            }
            AppEvent::Deactivated { pid } => {
                println!("[{clock}] APPLICATION_DEACTIVATED pid={}", pid.0);
            }
        },
        DomainEvent::Window { event, .. } => match event {
            WindowEvent::Created { pid, wid, bounds } => {
                let suffix = bounds
                    .map(format_rect)
                    .unwrap_or_else(|| "geometry=unknown".to_string());
                println!(
                    "[{clock}] WINDOW_CREATED pid={} window={} {suffix}",
                    pid.0, wid.0
                );
            }
            WindowEvent::Destroyed { pid, wid } => {
                println!("[{clock}] WINDOW_DESTROYED pid={} window={}", pid.0, wid.0);
            }
            WindowEvent::Moved { pid, wid, bounds } => {
                println!(
                    "[{clock}] WINDOW_MOVED pid={} window={} x={:.0} y={:.0}",
                    pid.0, wid.0, bounds.origin.x, bounds.origin.y
                );
            }
            WindowEvent::Resized { pid, wid, bounds } => {
                println!(
                    "[{clock}] WINDOW_RESIZED pid={} window={} width={:.0} height={:.0}",
                    pid.0, wid.0, bounds.size.width, bounds.size.height
                );
            }
            WindowEvent::Focused { pid, wid } => {
                println!("[{clock}] WINDOW_FOCUSED pid={} window={}", pid.0, wid.0);
            }
            WindowEvent::Minimized { pid, wid } => {
                println!("[{clock}] WINDOW_MINIMIZED pid={} window={}", pid.0, wid.0);
            }
            WindowEvent::Restored { pid, wid } => {
                println!("[{clock}] WINDOW_RESTORED pid={} window={}", pid.0, wid.0);
            }
        },
    }
}

/// Reports observer setup diagnostics that are useful to distinguish expected
/// platform limitations from unexpected failures.
fn report_observer_diagnostics(diagnostics: observers::ObserverDiagnostics) {
    if diagnostics.observer_creation_failures > 0
        || diagnostics.registration_failures > 0
        || diagnostics.unknown_elements > 0
        || diagnostics.overflow_events > 0
    {
        eprintln!(
            "diagnostics: observer_failures={} registration_failures={} unknown_elements={} unsupported_notifications={} overflow_events={}",
            diagnostics.observer_creation_failures,
            diagnostics.registration_failures,
            diagnostics.unknown_elements,
            diagnostics.unsupported_notifications,
            diagnostics.overflow_events,
        );
    }
}

/// Reports how the processor behaved, including registry size and duplicate
/// suppression.
fn report_processor_diagnostics(diagnostics: app::event_processor::Diagnostics, tracked: usize) {
    eprintln!(
        "processor: tracked={tracked} processed={} duplicates={} coalesced={} unknown={} reconciliations={}",
        diagnostics.events_processed,
        diagnostics.duplicates,
        diagnostics.coalesced,
        diagnostics.unknown_window_refs,
        diagnostics.reconciliations,
    );
}

/// Formats the current wall-clock time as `HH:MM:SS` (UTC).
fn format_clock() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
        % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

/// Runs the foreground shortcut overlay until interrupted with Ctrl+C.
fn show_overlay() -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    FOREGROUND_RUNNING.store(true, Ordering::SeqCst);
    install_interrupt_handler();

    overlay::prepare_application().map_err(map_overlay_error)?;
    let window = create_overlay_window()?;
    let mut visibility = app::overlay::OverlayState::new(true);
    window.show();

    let toggle_bindings: Vec<_> = shortcut::default_bindings()
        .into_iter()
        .filter(|binding| binding.action == Action::ToggleOverlay)
        .collect();
    let hotkeys = match hotkey::HotkeySet::register(&toggle_bindings) {
        Ok(set) => {
            for failure in set.failures() {
                eprintln!("warning: hotkey unavailable ({failure})");
            }
            println!("Overlay shown. Press Cmd+Opt+/ to toggle, Ctrl+C to quit.");
            Some(set)
        }
        Err(error) => {
            eprintln!(
                "warning: global hotkey unavailable ({error}); \
                 the overlay cannot be toggled with the keyboard"
            );
            println!("Overlay shown. Ctrl+C to quit.");
            None
        }
    };

    while FOREGROUND_RUNNING.load(Ordering::SeqCst) {
        overlay::pump_events(0.1);
        if let Some(set) = &hotkeys {
            for action in set.take_pending() {
                if action == Action::ToggleOverlay {
                    toggle_overlay(&window, &mut visibility);
                }
            }
        }
    }

    drop(hotkeys);
    drop(window);
    println!("Stopped.");
    Ok(())
}

/// Runs the foreground window manager (SPEC-008/SPEC-009).
///
/// Without flags the manager keeps every active display tiled automatically
/// (SPEC-009). Passing `--no-auto` preserves the exact SPEC-008 behavior: the
/// overlay plus functional shortcuts, with no automatic layout.
fn run_manager(args: &[String]) -> Result<(), AppError> {
    if !accessibility::is_trusted() {
        return Err(AppError::PermissionDenied);
    }

    let auto = !args.iter().any(|arg| arg == "--no-auto");

    FOREGROUND_RUNNING.store(true, Ordering::SeqCst);
    install_interrupt_handler();

    overlay::prepare_application().map_err(map_overlay_error)?;
    let window = create_overlay_window()?;
    let mut visibility = app::overlay::OverlayState::new(true);
    window.show();

    let bindings = shortcut::default_bindings();
    let hotkeys = match hotkey::HotkeySet::register(&bindings) {
        Ok(set) => {
            for failure in set.failures() {
                eprintln!("warning: hotkey unavailable ({failure})");
            }
            Some(set)
        }
        Err(error) => {
            eprintln!("warning: global hotkeys unavailable ({error}); shortcuts inactive");
            None
        }
    };

    if auto {
        println!("Window manager active (automatic tiling). Overlay shown. Ctrl+C to quit.");
        run_auto_manager(&window, &mut visibility, hotkeys.as_ref());
    } else {
        println!("Window manager active. Overlay shown. Ctrl+C to quit.");
        run_manual_manager(&window, &mut visibility, hotkeys.as_ref());
    }

    drop(hotkeys);
    drop(window);
    println!("Stopped.");
    Ok(())
}

/// Runs the SPEC-008 foreground loop: overlay plus shortcuts, no automatic
/// layout.
fn run_manual_manager(
    window: &overlay::OverlayWindow,
    visibility: &mut app::overlay::OverlayState,
    hotkeys: Option<&hotkey::HotkeySet>,
) {
    while FOREGROUND_RUNNING.load(Ordering::SeqCst) {
        overlay::pump_events(0.1);
        if let Some(set) = hotkeys {
            for action in set.take_pending() {
                execute_action(action, window, visibility);
            }
        }
    }
}

/// Runs the SPEC-009 foreground loop: observers, event normalization, and
/// automatic per-display re-tiling, alongside the SPEC-008 overlay/shortcuts.
fn run_auto_manager(
    window: &overlay::OverlayWindow,
    visibility: &mut app::overlay::OverlayState,
    hotkeys: Option<&hotkey::HotkeySet>,
) {
    let mut auto = AutoManager::new();
    let interval = auto.coalesce_interval().as_secs_f64();

    // SPEC-006 startup order: register initial observers and seed the registry,
    // then seed the tiling order and tile every active display once (FR-011).
    auto.pump();
    auto.seed_order();
    auto.retile_all();

    while FOREGROUND_RUNNING.load(Ordering::SeqCst) {
        observers::run_loop_step(interval);
        overlay::pump_events(0.05);
        if let Some(set) = hotkeys {
            for action in set.take_pending() {
                execute_action_auto(action, window, visibility, &mut auto);
            }
        }
        auto.pump();
        auto.retile_due();
    }

    auto.shutdown();
    report_diagnostics_auto(&auto);
}

/// Owns the SPEC-006 observer manager and the SPEC-009 tiling controller so the
/// automatic-tiling loop can drive them without exposing native types.
struct AutoManager {
    observers: Box<observers::ObserverManager>,
    processor: app::event_processor::EventProcessor,
    tiling: app::tiling::TilingState,
}

impl AutoManager {
    fn new() -> Self {
        let mut observers = Box::new(observers::ObserverManager::new());
        observers.subscribe_workspace();
        observers.sync_applications();
        Self {
            observers,
            processor: app::event_processor::EventProcessor::default(),
            tiling: app::tiling::TilingState::new(),
        }
    }

    fn coalesce_interval(&self) -> Duration {
        self.processor.coalesce_interval()
    }

    fn shutdown(&mut self) {
        self.observers.shutdown();
        report_observer_diagnostics(self.observers.diagnostics());
        report_processor_diagnostics(
            self.processor.diagnostics(),
            self.processor.registry().len(),
        );
    }

    /// Drains native events, keeps observers/registry consistent, and feeds the
    /// effective transitions to the tiling controller.
    fn pump(&mut self) {
        let now = Instant::now();
        let active = displays::snapshot();
        let mut reconcile = false;

        for event in self.observers.drain_events() {
            match &event {
                DomainEvent::App {
                    event: AppEvent::Launched { pid, name },
                    ..
                } => {
                    if let Err(error) = self.observers.register_application(*pid, name) {
                        eprintln!(
                            "observer registration failed for {name} (pid {}): {error}",
                            pid.0
                        );
                    }
                    reconcile = true;
                }
                DomainEvent::App {
                    event: AppEvent::Terminated { pid },
                    ..
                } => {
                    self.observers.remove_application(*pid);
                    reconcile = true;
                }
                _ => {}
            }

            for transition in self.processor.process(event, now) {
                self.tiling.observe(&transition, &active);
            }
        }

        for transition in self.processor.flush(now) {
            self.tiling.observe(&transition, &active);
        }

        if reconcile {
            let snapshot = self.observers.snapshot();
            for transition in self.processor.reconcile(snapshot, now) {
                self.tiling.observe(&transition, &active);
            }
        }
    }

    /// Seeds the per-display tiling order from the current registry snapshot.
    fn seed_order(&mut self) {
        let active = displays::snapshot();
        let entries: Vec<(WindowKey, WindowInfo)> = self
            .processor
            .registry()
            .iter()
            .filter(|record| !record.minimized)
            .map(|record| {
                (
                    record.key,
                    WindowInfo {
                        pid: record.key.pid,
                        application: record.application.clone(),
                        title: record.title.clone(),
                        bounds: record.bounds,
                    },
                )
            })
            .collect();
        self.tiling.seed(&active, entries);
    }

    /// Re-tiles every display the controller reports as ready.
    fn retile_due(&mut self) {
        for display in self.tiling.due(Instant::now()) {
            self.retile_display(display);
        }
    }

    /// Re-tiles every active display once (initial tiling, FR-011).
    fn retile_all(&mut self) {
        for display in displays::snapshot() {
            self.retile_display(display.id);
        }
    }

    /// Re-tiles a single display using the persistent order and layout (FR-009).
    fn retile_display(&mut self, display: DisplayId) {
        let active = displays::snapshot();
        let config = self.tiling.layout_for(display);
        let order = self.tiling.order_for(display).to_vec();
        let now = Instant::now();

        if order.is_empty() {
            eprintln!("automatic tiling: display {display}: no eligible windows");
            self.tiling.record_applied(display, &[], now);
            return;
        }

        let windows = {
            let observers = &mut self.observers;
            control::discover_tiling_windows(|pid, element| {
                observers.resolve_identity(pid, element)
            })
        };

        let plan = match app::layout_planner::build_plan_ordered(
            &active, windows, display, config, &order,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!("automatic tiling: display {display}: {error}");
                self.tiling.record_applied(display, &[], now);
                return;
            }
        };

        if plan.entries.is_empty() {
            eprintln!("automatic tiling: display {display}: no eligible windows");
        }

        let keys: Vec<(WindowId, WindowKey)> = plan
            .entries
            .iter()
            .map(|entry| (entry.window_id, entry.window.key()))
            .collect();

        let report = app::layout_executor::execute(
            plan,
            &NativeBackend,
            app::layout_executor::ExecutionMode::Apply,
        );

        let applied = applied_geometry(&report, &keys);
        self.tiling.record_applied(display, &applied, now);

        if report.failed > 0 || report.constrained > 0 {
            eprintln!(
                "automatic tiling: display {display}: applied {}, constrained {}, failed {}, skipped {}",
                report.applied, report.constrained, report.failed, report.skipped
            );
        }
    }
}

/// Extracts the geometry macOS actually applied per window, keyed by its stable
/// identity (SPEC-009 FR-006). Windows that failed are omitted so they are
/// retried on the next change.
fn applied_geometry(
    report: &ExecutionReport,
    keys: &[(WindowId, WindowKey)],
) -> Vec<(WindowKey, Rect)> {
    let mut applied = Vec::with_capacity(report.results.len());
    for result in &report.results {
        let Some(key) = keys
            .iter()
            .find(|(id, _)| *id == result.window_id)
            .map(|(_, key)| *key)
        else {
            continue;
        };
        match &result.state {
            crate::core::layout_plan::WindowResultState::Applied(actual)
            | crate::core::layout_plan::WindowResultState::Constrained(actual) => {
                applied.push((key, *actual));
            }
            crate::core::layout_plan::WindowResultState::AlreadyCorrect => {
                applied.push((key, result.target));
            }
            crate::core::layout_plan::WindowResultState::Failed(_)
            | crate::core::layout_plan::WindowResultState::Planned => {}
        }
    }
    applied
}

/// Dispatches one hotkey action under automatic tiling (SPEC-009 FR-013/014/015).
fn execute_action_auto(
    action: Action,
    overlay_window: &overlay::OverlayWindow,
    visibility: &mut app::overlay::OverlayState,
    auto: &mut AutoManager,
) {
    match action {
        Action::ToggleOverlay => toggle_overlay(overlay_window, visibility),
        Action::SetLayout(layout) => auto_set_layout(layout, auto),
        Action::Focus(direction) => auto_navigate(direction, false, auto),
        Action::MoveWindow(direction) => auto_navigate(direction, true, auto),
    }
}

/// Sets the focused display's layout and re-tiles it immediately (FR-013).
fn auto_set_layout(layout: LayoutType, auto: &mut AutoManager) {
    let Some(display) = focused_display() else {
        eprintln!("layout: no focused window");
        return;
    };
    auto.tiling.set_layout(display, layout);
    auto.retile_display(display);
}

/// Focuses or reorders the focused window's directional neighbour using the
/// persistent tiling order (FR-014, FR-015).
fn auto_navigate(direction: Direction, move_window: bool, auto: &mut AutoManager) {
    let Some(focused) = control::focused_window() else {
        eprintln!("navigation: no focused window");
        return;
    };
    let Some(bounds) = focused.info.bounds else {
        eprintln!("navigation: focused window has no geometry");
        return;
    };
    let active = displays::snapshot();
    let Some(target) = display_assignment::assign(&active, bounds) else {
        eprintln!("navigation: focused window is not on an active display");
        return;
    };

    let order = auto.tiling.order_for(target).to_vec();
    let windows = {
        let observers = &mut auto.observers;
        control::discover_tiling_windows(|pid, element| observers.resolve_identity(pid, element))
    };

    let mut eligible: Vec<(control::LayoutWindow, Rect)> = Vec::new();
    for window in windows {
        if let Ok(rect) = app::window_eligibility::assess(&window, &active, target) {
            eligible.push((window, rect));
        }
    }
    if eligible.len() < 2 {
        return;
    }

    let position = |key: WindowKey| order.iter().position(|candidate| *candidate == key);
    eligible.sort_by(|a, b| match (position(a.0.key()), position(b.0.key())) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => compare_windows(a.0.info(), b.0.info()),
    });

    let rects: Vec<Rect> = eligible.iter().map(|(_, rect)| *rect).collect();
    let ordered: Vec<control::LayoutWindow> =
        eligible.into_iter().map(|(window, _)| window).collect();

    let Some(current) = control::focused_index(&ordered) else {
        eprintln!("navigation: focused window not found among eligible windows");
        return;
    };
    let Some(neighbor) = navigation::directional_neighbor(&rects, current, direction) else {
        return;
    };

    if move_window {
        let current_key = ordered[current].key();
        let neighbor_key = ordered[neighbor].key();
        if auto.tiling.swap_in_order(target, current_key, neighbor_key) {
            auto.retile_display(target);
        }
    } else {
        focus_neighbor(&ordered, neighbor);
    }
}

/// Returns the active display containing the focused window, if any.
fn focused_display() -> Option<DisplayId> {
    let focused = control::focused_window()?;
    let bounds = focused.info.bounds?;
    let active = displays::snapshot();
    display_assignment::assign(&active, bounds)
}

/// Reports automatic-tiling and controller diagnostics (FR-018).
fn report_diagnostics_auto(auto: &AutoManager) {
    let diagnostics = auto.tiling.diagnostics();
    eprintln!(
        "tiling: retiles={} win_created={} win_destroyed={} suppressed_geometry={}",
        diagnostics.retiles,
        diagnostics.windows_created,
        diagnostics.windows_destroyed,
        diagnostics.suppressed_geometry_events,
    );
}

fn create_overlay_window() -> Result<overlay::OverlayWindow, AppError> {
    let content = shortcut::render_plain(&shortcut::default_shortcuts());
    let size = app::overlay::overlay_size(&content);

    let active = displays::snapshot();
    let frame = control::focused_window()
        .and_then(|window| window.info.bounds)
        .and_then(displays::work_area_for_window)
        .or_else(|| display::main_display(&active).map(|display| display.usable))
        .ok_or_else(|| AppError::OverlayFailed("no active display is available".to_string()))?;

    overlay::OverlayWindow::create(frame, size, &content).map_err(map_overlay_error)
}

fn toggle_overlay(window: &overlay::OverlayWindow, visibility: &mut app::overlay::OverlayState) {
    visibility.toggle();
    if visibility.is_visible() {
        window.show();
    } else {
        window.hide();
    }
}

/// Dispatches one hotkey action (SPEC-008).
fn execute_action(
    action: Action,
    overlay_window: &overlay::OverlayWindow,
    visibility: &mut app::overlay::OverlayState,
) {
    match action {
        Action::ToggleOverlay => toggle_overlay(overlay_window, visibility),
        Action::SetLayout(layout) => apply_layout_action(layout),
        Action::Focus(direction) => navigate(direction, false),
        Action::MoveWindow(direction) => navigate(direction, true),
    }
}

/// Applies `layout` to the display containing the focused window.
fn apply_layout_action(layout: LayoutType) {
    let Some(focused) = control::focused_window() else {
        eprintln!("layout: no focused window");
        return;
    };
    let Some(bounds) = focused.info.bounds else {
        eprintln!("layout: focused window has no geometry");
        return;
    };
    let active = displays::snapshot();
    let Some(target) = display_assignment::assign(&active, bounds) else {
        eprintln!("layout: focused window is not on an active display");
        return;
    };

    let config = LayoutConfig {
        layout,
        ..LayoutConfig::default()
    };
    let windows = control::discover_layout_windows();
    let plan = match app::layout_planner::build_plan(&active, windows, target, config) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("layout: {error}");
            return;
        }
    };

    let report = app::layout_executor::execute(
        plan,
        &NativeBackend,
        app::layout_executor::ExecutionMode::Apply,
    );
    if report.failed > 0 || report.constrained > 0 {
        eprintln!(
            "layout ({layout}): applied {}, constrained {}, failed {}, skipped {}",
            report.applied, report.constrained, report.failed, report.skipped
        );
    }
}

/// Moves focus (`move_window == false`) or moves the focused window
/// (`move_window == true`) to the directional neighbour on the same display.
fn navigate(direction: Direction, move_window: bool) {
    let Some(focused) = control::focused_window() else {
        eprintln!("navigation: no focused window");
        return;
    };
    let Some(bounds) = focused.info.bounds else {
        eprintln!("navigation: focused window has no geometry");
        return;
    };
    let active = displays::snapshot();
    let Some(target) = display_assignment::assign(&active, bounds) else {
        eprintln!("navigation: focused window is not on an active display");
        return;
    };

    let mut eligible: Vec<(control::LayoutWindow, Rect)> = Vec::new();
    for window in control::discover_layout_windows() {
        if let Ok(rect) = app::window_eligibility::assess(&window, &active, target) {
            eligible.push((window, rect));
        }
    }
    if eligible.len() < 2 {
        return;
    }

    eligible.sort_by(|a, b| compare_windows(a.0.info(), b.0.info()));
    let rects: Vec<Rect> = eligible.iter().map(|(_, rect)| *rect).collect();
    let ordered: Vec<control::LayoutWindow> =
        eligible.into_iter().map(|(window, _)| window).collect();

    let Some(current) = control::focused_index(&ordered) else {
        eprintln!("navigation: focused window not found among eligible windows");
        return;
    };
    let Some(neighbor) = navigation::directional_neighbor(&rects, current, direction) else {
        return;
    };

    if move_window {
        move_to_neighbor(&ordered, &rects, current, neighbor);
    } else {
        focus_neighbor(&ordered, neighbor);
    }
}

fn focus_neighbor(ordered: &[control::LayoutWindow], neighbor: usize) {
    let window = &ordered[neighbor];
    if let Err(error) = control::focus_window(&window.handle, window.info.pid) {
        eprintln!("focus: {error}");
    }
    if let Err(error) = control::activate_application(window.info.pid) {
        eprintln!("focus: {error}");
    }
}

fn move_to_neighbor(
    ordered: &[control::LayoutWindow],
    rects: &[Rect],
    current: usize,
    neighbor: usize,
) {
    let current_rect = rects[current];
    let neighbor_rect = rects[neighbor];
    if let Err(error) = control::set_bounds(&ordered[current].handle, neighbor_rect) {
        eprintln!("move window: {error}");
    }
    if let Err(error) = control::set_bounds(&ordered[neighbor].handle, current_rect) {
        eprintln!("move window: {error}");
    }
}

fn map_overlay_error(error: overlay::OverlayError) -> AppError {
    AppError::OverlayFailed(error.to_string())
}

extern "C" fn handle_sigint(_signal: libc::c_int) {
    // Storing an atomic flag is safe inside a signal handler.
    FOREGROUND_RUNNING.store(false, Ordering::SeqCst);
}

fn install_interrupt_handler() {
    // SAFETY: `handle_sigint` has the expected signal handler signature.
    unsafe {
        libc::signal(libc::SIGINT, handle_sigint as libc::sighandler_t);
    }
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

    fn sample_display(id: u32, main: bool) -> Display {
        Display {
            id: DisplayId(id),
            name: Some("Built-in Retina Display".to_string()),
            main,
            scale: 2.0,
            bounds: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 875.0),
        }
    }

    #[test]
    fn displays_table_contains_expected_fields() {
        let table = render_displays_table(&[sample_display(1, true)]);
        assert!(table.contains("ID"));
        assert!(table.contains("USABLE-H"));
        assert!(table.contains("1"));
        assert!(table.contains("Built-in Retina Display"));
        assert!(table.contains("yes"));
        assert!(table.contains("1440"));
        assert!(table.contains("875"));
    }

    #[test]
    fn display_detail_contains_geometry() {
        let detail = render_display_detail(&sample_display(1, true));
        assert!(detail.contains("ID: 1"));
        assert!(detail.contains("Main: yes"));
        assert!(detail.contains("Scale: 2.0"));
        assert!(detail.contains("Valid: yes"));
        assert!(detail.contains("height=875"));
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

    #[test]
    fn apply_requires_a_layout_name() {
        let error = parse_apply_args(&args(&[])).expect_err("should fail");
        assert!(matches!(error, AppError::InvalidArgument(_)));
    }

    #[test]
    fn apply_parses_layout_with_optional_flags() {
        let options =
            parse_apply_args(&args(&["columns", "--display", "42", "--dry-run"])).expect("parse");
        assert_eq!(options.layout, LayoutType::Columns);
        assert_eq!(options.display.as_deref(), Some("42"));
        assert!(options.dry_run);
    }

    #[test]
    fn apply_rejects_unknown_layout_and_flags() {
        assert!(matches!(
            parse_apply_args(&args(&["bsp"])),
            Err(AppError::InvalidArgument(_))
        ));
        assert!(matches!(
            parse_apply_args(&args(&["rows", "--bogus"])),
            Err(AppError::InvalidArgument(_))
        ));
    }

    #[test]
    fn execution_report_renders_counts_and_states() {
        use crate::core::display::DisplayId;
        use crate::core::layout_plan::{WindowResult, WindowResultState};

        let report = ExecutionReport {
            display_id: DisplayId(1),
            layout: LayoutType::Columns,
            discovered: 3,
            eligible: 1,
            applied: 1,
            constrained: 0,
            failed: 0,
            skipped: 0,
            planned: 0,
            results: vec![WindowResult {
                window_id: WindowId(0),
                application: "Terminal".to_string(),
                title: None,
                original: Rect::new(5.0, 5.0, 100.0, 100.0),
                target: Rect::new(0.0, 25.0, 1440.0, 875.0),
                state: WindowResultState::Applied(Rect::new(0.0, 25.0, 1440.0, 875.0)),
            }],
            exclusions: Vec::new(),
        };

        let rendered = render_execution_report(&report);
        assert!(rendered.contains("Applied: 1"));
        assert!(rendered.contains("Terminal"));
        assert!(rendered.contains("applied"));
        assert!(rendered.contains("1440"));
    }

    #[test]
    fn applied_geometry_records_actual_and_skips_failures() {
        use crate::core::event::RuntimeWindowId;
        use crate::core::layout_plan::{WindowResult, WindowResultState};

        let key_a = WindowKey::new(ProcessId(1), RuntimeWindowId::new(10));
        let key_b = WindowKey::new(ProcessId(1), RuntimeWindowId::new(11));
        let key_c = WindowKey::new(ProcessId(1), RuntimeWindowId::new(12));
        let target = Rect::new(0.0, 25.0, 1440.0, 875.0);

        let report = ExecutionReport {
            display_id: DisplayId(1),
            layout: LayoutType::MasterStack,
            discovered: 3,
            eligible: 3,
            applied: 1,
            constrained: 1,
            failed: 1,
            skipped: 0,
            planned: 0,
            results: vec![
                WindowResult {
                    window_id: WindowId(0),
                    application: "A".to_string(),
                    title: None,
                    original: Rect::new(5.0, 5.0, 100.0, 100.0),
                    target,
                    state: WindowResultState::Applied(target),
                },
                WindowResult {
                    window_id: WindowId(1),
                    application: "B".to_string(),
                    title: None,
                    original: Rect::new(5.0, 5.0, 100.0, 100.0),
                    target,
                    state: WindowResultState::Constrained(Rect::new(0.0, 25.0, 1400.0, 800.0)),
                },
                WindowResult {
                    window_id: WindowId(2),
                    application: "C".to_string(),
                    title: None,
                    original: Rect::new(5.0, 5.0, 100.0, 100.0),
                    target,
                    state: WindowResultState::Failed("boom".to_string()),
                },
            ],
            exclusions: Vec::new(),
        };

        let keys = vec![
            (WindowId(0), key_a),
            (WindowId(1), key_b),
            (WindowId(2), key_c),
        ];
        let applied = applied_geometry(&report, &keys);

        assert_eq!(
            applied,
            vec![
                (key_a, target),
                (key_b, Rect::new(0.0, 25.0, 1400.0, 800.0)),
            ]
        );
    }
}
