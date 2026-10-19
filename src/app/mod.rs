//! Application-layer orchestration for layout planning and execution.
//!
//! This layer composes the pure `core` modules with platform interfaces through
//! narrow traits so the pipeline stays testable without macOS.

pub mod desktop;
pub mod event_processor;
pub mod layout_executor;
pub mod layout_planner;
pub mod overlay;
pub mod registry;
pub mod tiling;
pub mod window_eligibility;
