//! Shared command-line parsing for the terminal and desktop front ends.

use std::path::PathBuf;

use crate::app::{Options, Theme};
use crate::disasm;

#[derive(Clone, Debug)]
pub enum Action {
    Run,
    Help,
    Version,
    Bench(Vec<PathBuf>),
}

#[derive(Clone, Debug)]
pub struct Parsed {
    pub options: Options,
    pub paths: Vec<PathBuf>,
    pub action: Action,
    /// True when a theme flag selected the palette, rather than the default.
    pub theme_explicit: bool,
}

/// Parse the arguments after the program name.  The returned error is a
/// one-line diagnostic; callers print [`usage`] first, as the original does.
pub fn parse(args: Vec<String>) -> Result<Parsed, String> {
    let mut options = Options::default();
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut theme_explicit = false;

    if args.first().map(String::as_str) == Some("--bench") {
        let bench_paths: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
        return Ok(Parsed {
            options,
            paths: Vec::new(),
            action: Action::Bench(bench_paths),
            theme_explicit,
        });
    }

    for arg in &args {
        match arg.as_str() {
            "-h" | "--help" => {
                return Ok(Parsed {
                    options,
                    paths,
                    action: Action::Help,
                    theme_explicit,
                });
            }
            "-V" | "--version" => {
                return Ok(Parsed {
                    options,
                    paths,
                    action: Action::Version,
                    theme_explicit,
                });
            }
            "--mono" => {
                options.theme = Theme::Mono;
                theme_explicit = true;
            }
            "--colour" | "--color" => {
                options.theme = Theme::Classic;
                theme_explicit = true;
            }
            "--amber" => {
                options.theme = Theme::Amber;
                theme_explicit = true;
            }
            "--dark" | "--modern-dark" => {
                options.theme = Theme::ModernDark;
                theme_explicit = true;
            }
            "--light" | "--modern-light" => {
                options.theme = Theme::ModernLight;
                theme_explicit = true;
            }
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
            "--x86" => options.disasm = Some(disasm::Cpu::X86),
            "--x64" | "--amd64" => options.disasm = Some(disasm::Cpu::X64),
            "--arm" => options.disasm = Some(disasm::Cpu::Arm),
            "--thumb" => options.disasm = Some(disasm::Cpu::Thumb),
            "--arm64" | "--aarch64" => options.disasm = Some(disasm::Cpu::Arm64),
            "--text" | "--ascii-view" => options.disasm = None,
            s if s.starts_with("--disasm=") => {
                let name = &s["--disasm=".len()..];
                match disasm::Cpu::parse(name) {
                    Some(cpu) => options.disasm = Some(cpu),
                    None => return Err(format!("unknown disassembly target {name}")),
                }
            }
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    if paths.len() > 2 {
        return Err("at most two files can be opened".to_string());
    }

    Ok(Parsed {
        options,
        paths,
        action: Action::Run,
        theme_explicit,
    })
}

pub fn usage() -> String {
    const BODY: &str = r#" - Compare And Browse with Limited Editing

Usage: cable-rs [OPTIONS] [FILE1 [FILE2]]

With no files, a directory browser is opened.

Options:
  --mono            start in mono
  --colour, --color start in colour (default)
  --amber           start in amber
  --dark            start in the modern dark palette
  --light           start in the modern light palette
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
  --text            show ASCII/EBCDIC in the right-hand column (default)
  --x86, --x64      disassemble the right-hand column as x86 / x86-64
  --arm, --thumb    disassemble as ARMv7 / Thumb
  --arm64           disassemble as AArch64
  --disasm=CPU      same as the flags above (CPU: x86, x64, arm, thumb, arm64)
  --bench           run the non-interactive I/O benchmark
  -h, --help        show this help
  -V, --version     show version"#;
    format!("cable-rs {}{BODY}", env!("CARGO_PKG_VERSION"))
}
