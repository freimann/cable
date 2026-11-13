//! CABLE desktop front end: a native window around the same core used by the
//! terminal program.

// A release build opens only its own window; the console is for the terminal
// front end.  The attribute is ignored on non-Windows targets.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    match cable_rs::gui::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cable-gui: {e:#}");
            ExitCode::FAILURE
        }
    }
}
