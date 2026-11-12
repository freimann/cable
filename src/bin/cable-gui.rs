//! CABLE desktop front end: a native window around the same core used by the
//! terminal program.

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
