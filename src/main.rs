//! CABLE-RS: a modern, cross-platform reimplementation of the DOS program
//! `CABLE - Compare And Browse with Limited Editing`.

mod app;
mod buffer;
mod codec;
mod compare;
mod dir;
mod help;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use crossterm::event::{self, Event};

use app::{App, Options, Theme};

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
    let mut options = Options::default();
    let mut paths: Vec<PathBuf> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                usage();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("cable-rs {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--mono" => options.theme = Theme::Mono,
            "--colour" | "--color" => options.theme = Theme::Classic,
            "--amber" => options.theme = Theme::Amber,
            "--ebcdic" => options.ebcdic = true,
            "--ascii" => options.ebcdic = false,
            "--show" => options.conceal = false,
            "--conceal" => options.conceal = true,
            "--ignore-case" => options.respect_case = false,
            "--respect-case" => options.respect_case = true,
            "--hex" => options.hex_input = true,
            "--character" => options.hex_input = false,
            "--backward" => options.forward = false,
            "--forward" => options.forward = true,
            "--both" => options.move_both = true,
            "--active" => options.move_both = false,
            s if s.starts_with('-') => {
                usage();
                bail!("unknown option {s}");
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    if paths.len() > 2 {
        usage();
        bail!("at most two files can be opened");
    }

    let mut app = App::new(&paths, options)?;
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

fn usage() {
    const BODY: &str = r#" - Compare And Browse with Limited Editing

Usage: cable-rs [OPTIONS] [FILE1 [FILE2]]

With no files, a directory browser is opened.

Options:
  --mono            start in mono
  --colour, --color start in colour (default)
  --amber           start in amber
  --ascii           treat data as ASCII (default)
  --ebcdic          treat data as EBCDIC (CP037)
  --conceal         show unprintables as '.' (default)
  --show            show unprintables as control pictures
  --respect-case    case-sensitive search/compare (default)
  --ignore-case     case-insensitive search/compare
  --hex             HEX search/edit input
  --character       character search/edit input (default)
  --forward         compare/search forwards (default)
  --backward        compare/search backwards
  --active          scroll only the active window (default)
  --both            scroll both windows together
  -h, --help        show this help
  -V, --version     show version"#;
    println!("cable-rs {}{BODY}", env!("CARGO_PKG_VERSION"));
}
