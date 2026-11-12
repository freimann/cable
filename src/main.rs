//! CABLE-RS terminal front end: a modern, cross-platform reimplementation of
//! the DOS program `CABLE - Compare And Browse with Limited Editing`.

use std::process::ExitCode;

use anyhow::Result;
use crossterm::event::{self, Event};

use cable_rs::app::App;
use cable_rs::{bench, cli, ui};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cable-rs: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let parsed = match cli::parse(std::env::args().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(e) => {
            println!("{}", cli::usage());
            anyhow::bail!("{e}");
        }
    };

    match parsed.action {
        cli::Action::Help => {
            println!("{}", cli::usage());
            return Ok(());
        }
        cli::Action::Version => {
            println!("cable-rs {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        cli::Action::Bench(paths) => return bench::run(&paths),
        cli::Action::Run => {}
    }

    let mut app = App::new(&parsed.paths, parsed.options)?;
    ratatui::run(|terminal| -> Result<()> {
        loop {
            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            if let Event::Key(key) = event::read()? {
                app.handle_key(key)?;
            }
            if app.quit {
                break;
            }
        }
        Ok(())
    })?;
    Ok(())
}
