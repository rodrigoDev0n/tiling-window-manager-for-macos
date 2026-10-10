//! Application-layer orchestration for layout planning and execution.
//!
//! This layer composes the pure `core` modules with platform interfaces through
//! narrow traits so the pipeline stays testable without macOS.

pub mod event_processor;
pub mod layout_executor;
pub mod layout_planner;
pub mod registry;
pub mod window_eligibility;
