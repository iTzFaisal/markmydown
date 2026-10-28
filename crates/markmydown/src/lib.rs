//! markmydown — a fast, native, offline-first Markdown workbench.
//!
//! The crate is split into domain modules so that file safety, parsing,
//! rendering, export, and UI code can be tested independently.

pub mod app;
pub mod config;
pub mod editor;
pub mod export;
pub mod preview;
pub mod theme;
pub mod ui;
pub mod workspace;
