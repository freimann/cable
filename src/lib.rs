//! CABLE-RS: a modern, cross-platform reimplementation of the DOS program
//! `CABLE - Compare And Browse with Limited Editing`.
//!
//! The crate is split into a reusable core (`app`, `buffer`, `compare`, ...)
//! plus two front ends: the original terminal UI in [`ui`] and the desktop GUI
//! in [`gui`], enabled with the `gui` feature.

pub mod app;
pub mod bench;
pub mod buffer;
pub mod cli;
pub mod codec;
pub mod compare;
pub mod dir;
pub mod disasm;
#[cfg(feature = "gui")]
pub mod gui;
pub mod help;
pub mod ui;
