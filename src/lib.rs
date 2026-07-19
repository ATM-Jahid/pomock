//! Core application and terminal boundaries for `pomock`.
//!
//! State changes flow from physical input through [`input`] into semantic
//! [`app::Action`] values. [`app::App`] owns terminal-independent transitions
//! and reports boundary work through [`app::AppOutcome`]. The [`ui`] module may
//! read application state, while [`persistence`] stores opaque task snapshots.
//! Terminal IO and outcome handling stay in the binary composition root.
//!
//! This is a pre-1.0 internal API used by the `pomock` binary. It is organized
//! as an extension seam for future workspace crates, not yet promised as a
//! stable third-party library interface.

pub mod app;
pub mod config;
mod display;
pub mod input;
pub mod notification;
pub mod persistence;
mod settings;
pub mod sound;
mod tasks;
mod timer;
pub mod ui;
mod ui_layout;

pub use timer::SessionKind;
