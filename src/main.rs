mod app;
mod commands;
mod config;
mod core;
mod error;
mod layouts;
mod platform;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match commands::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            if matches!(err, error::AppError::PermissionDenied) {
                eprintln!();
                eprintln!(
                    "{}",
                    platform::macos::accessibility::PERMISSION_INSTRUCTIONS
                );
            }
            ExitCode::FAILURE
        }
    }
}
