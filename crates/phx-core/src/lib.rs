//! OmaPhoenix engine: detect, capture and restore an Omarchy setup through a git-backed store.

pub mod app;
pub mod category;
pub mod doctor;
pub mod fuzzy;
pub mod gate;
pub mod migrate;
pub mod schedule;
pub mod store;
pub mod sys;
pub mod undo;

pub use app::{App, AppError, RestoreOptions, StageReport};
