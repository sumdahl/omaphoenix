//! OmaPhoenix engine: detects what makes an Omarchy machine yours, captures
//! the selected items into a git-backed snapshot store, and restores them.
//! No UI: front ends pass a log function and render the results.

pub mod app;
pub mod category;
pub mod doctor;
pub mod gate;
pub mod migrate;
pub mod schedule;
pub mod store;
pub mod sys;
pub mod undo;

pub use app::{App, AppError, RestoreOptions, StageReport};
