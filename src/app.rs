//! Application state and key handling.

use std::path::{Path, PathBuf};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Color;

use crate::buffer::PagedFile;
use crate::codec;
use crate::compare;
use crate::dir::{DirBrowser, SortKey};
use crate::disasm::Cpu;
use crate::help;

pub const MENU: [(&str, char); 14] = [
    ("Compare", 'C'),
    ("Dir", 'D'),
    ("Edit", 'E'),
    ("Help", 'H'),
    ("Mode", 'M'),
    ("Next", 'N'),
    ("Options", 'O'),
    ("+", '+'),
    ("-", '-'),
    ("=", '='),
    ("Quit", 'Q'),
    ("Search", 'S'),
    ("View", 'V'),
    ("Write", 'W'),
];

/// Keys that toggle the original Scroll Lock state: the key itself, F7 (both
/// keys are missing or unreliable in some terminals), and Ctrl+L / Ctrl+B.
/// Windows terminals can report Ctrl+L either as `Char('l')` with CONTROL or
/// as the bare form-feed character, so both are accepted.
fn is_scroll_lock_toggle(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::ScrollLock | KeyCode::F(7) | KeyCode::Char('\u{c}') => true,
        KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Char('b') | KeyCode::Char('B') => {
            key.modifiers.contains(KeyModifiers::CONTROL)
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Classic,
    Amber,
    Mono,
}

impl Theme {
    pub fn name(self) -> &'static str {
        match self {
            Theme::Classic => "Colour (blue/yellow)",
            Theme::Amber => "Amber",
            Theme::Mono => "Mono",
        }
    }

    pub fn bg(self) -> Color {
        match self {
            Theme::Classic => Color::Blue,
            Theme::Amber | Theme::Mono => Color::Black,
        }
    }

    pub fn fg(self) -> Color {
        match self {
            Theme::Classic => Color::LightYellow,
            Theme::Amber => Color::Rgb(255, 191, 0),
            Theme::Mono => Color::White,
        }
    }

    pub fn dim(self) -> Color {
        match self {
            Theme::Classic => Color::Gray,
            Theme::Amber => Color::Rgb(150, 100, 0),
            Theme::Mono => Color::DarkGray,
        }
    }

    /// Background of the original's selection bars (`0x78` in colour mode,
    /// i.e. light grey) and of the highlighted menu command.
    pub fn bar_bg(self) -> Color {
        match self {
            Theme::Classic => Color::Gray,
            Theme::Amber => Color::Rgb(150, 100, 0),
            Theme::Mono => Color::DarkGray,
        }
    }

    /// Text drawn on a selection bar (`0x78` = dark grey on light grey).
    pub fn bar_fg(self) -> Color {
        match self {
            Theme::Classic => Color::DarkGray,
            Theme::Amber | Theme::Mono => Color::Black,
        }
    }

    pub fn accent(self) -> Color {
        match self {
            Theme::Classic => Color::Cyan,
            Theme::Amber => Color::Rgb(255, 220, 120),
            Theme::Mono => Color::White,
        }
    }

    pub fn header_fg(self) -> Color {
        match self {
            Theme::Classic => Color::White,
            Theme::Amber => Color::Rgb(255, 220, 120),
            Theme::Mono => Color::White,
        }
    }

    pub fn cursor_fg(self) -> Color {
        match self {
            Theme::Classic => Color::Blue,
            Theme::Amber | Theme::Mono => Color::Black,
        }
    }

    pub fn cursor_bg(self) -> Color {
        match self {
            Theme::Classic => Color::White,
            Theme::Amber => Color::Rgb(255, 191, 0),
            Theme::Mono => Color::White,
        }
    }

    pub fn diff_fg(self) -> Color {
        match self {
            Theme::Classic => Color::White,
            Theme::Amber | Theme::Mono => Color::Black,
        }
    }

    pub fn diff_bg(self) -> Color {
        match self {
            Theme::Classic => Color::Red,
            Theme::Amber => Color::Rgb(255, 191, 0),
            Theme::Mono => Color::White,
        }
    }

    pub fn border(self) -> Color {
        match self {
            Theme::Classic => Color::Cyan,
            Theme::Amber => Color::Rgb(255, 191, 0),
            Theme::Mono => Color::White,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub ebcdic: bool,
    pub conceal: bool,
    pub respect_case: bool,
    pub forward: bool,
    pub hex_input: bool,
    /// Scroll Lock: false moves the active window only (`SL:Active`),
    /// true moves both windows together (`SL:Both`).
    pub move_both: bool,
    pub theme: Theme,
    /// Right-hand character view: `None` shows ASCII/EBCDIC, `Some(cpu)`
    /// shows passive disassembly for that instruction set.
    pub disasm: Option<Cpu>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ebcdic: false,
            conceal: true,
            respect_case: true,
            forward: true,
            hex_input: false,
            move_both: false,
            theme: Theme::Classic,
            disasm: None,
        }
    }
}

impl Options {
    pub fn charset(&self) -> &'static str {
        if self.ebcdic { "EBCDIC" } else { "ASCII" }
    }

    pub fn direction(&self) -> &'static str {
        if self.forward { "Forward" } else { "Backward" }
    }

    pub fn case(&self) -> &'static str {
        if self.respect_case {
            "Respect"
        } else {
            "Ignore"
        }
    }

    pub fn show(&self) -> &'static str {
        if self.conceal { "Conceal" } else { "Show" }
    }

    pub fn input(&self) -> &'static str {
        if self.hex_input { "HEX" } else { "Character" }
    }

    pub fn scroll_lock(&self) -> &'static str {
        if self.move_both { "Both" } else { "Active" }
    }

    /// Name of the right-hand character view.  Without a disassembly CPU this
    /// is the active character set, so the status line is unchanged in text
    /// mode.
    pub fn side_view(&self) -> &'static str {
        match self.disasm {
            Some(cpu) => cpu.label(),
            None => self.charset(),
        }
    }

    /// Advance the side view through every CPU and back to characters.
    pub fn cycle_side_view(&mut self) {
        self.disasm = match self.disasm {
            None => Some(Cpu::X86),
            Some(cpu) => cpu.next(),
        };
    }
}

pub struct Pane {
    pub file: Option<PagedFile>,
    pub cursor: u64,
    pub top: u64,
}

impl Pane {
    pub fn empty() -> Self {
        Self {
            file: None,
            cursor: 0,
            top: 0,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file = PagedFile::open(path)?;
        Ok(Self {
            file: Some(file),
            cursor: 0,
            top: 0,
        })
    }

    pub fn len(&self) -> u64 {
        self.file.as_ref().map(PagedFile::len).unwrap_or(0)
    }

    pub fn name(&self) -> String {
        match &self.file {
            Some(f) => {
                let full = f.path().to_string_lossy();
                if full.chars().count() <= 34 {
                    full.into_owned()
                } else {
                    f.path()
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| full.into_owned())
                }
            }
            None => "--- empty ---".to_string(),
        }
    }

    pub fn dirty(&self) -> bool {
        self.file.as_ref().map(PagedFile::is_dirty).unwrap_or(false)
    }

    pub fn cursor_max(&self, allow_append: bool) -> u64 {
        let len = self.len();
        if len == 0 {
            0
        } else if allow_append {
            len
        } else {
            len - 1
        }
    }

    pub fn clamp_cursor(&mut self, allow_append: bool) {
        let max = self.cursor_max(allow_append);
        if self.cursor > max {
            self.cursor = max;
        }
    }

    pub fn ensure_visible(&mut self, rows: usize, bytes_per_row: usize) {
        let rows = rows.max(1) as u64;
        let bpr = bytes_per_row.max(16) as u64;
        self.top -= self.top % bpr;
        let page = rows * bpr;
        let line_start = (self.cursor / bpr) * bpr;
        if line_start < self.top {
            self.top = line_start;
        } else if self.cursor >= self.top + page {
            let last_row = (self.cursor / bpr).saturating_sub(rows - 1);
            self.top = last_row * bpr;
        }
        // Keep the window inside the file when possible.
        let len = self.len();
        if len > 0 && self.top >= len {
            self.top = ((len - 1) / bpr) * bpr;
        }
    }
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Quit,
    Write,
    Load { path: PathBuf, pane: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputAction {
    Add,
    Sub,
    Goto,
    Search,
    View,
}

#[derive(Clone, Copy, Debug)]
pub struct EditState {
    pub nibble: Option<u8>,
}

pub enum Modal {
    Confirm {
        title: String,
        text: String,
        action: ConfirmAction,
    },
    Input {
        title: String,
        prompt: String,
        value: String,
        action: InputAction,
    },
    Options {
        index: usize,
    },
    Modes {
        index: usize,
    },
    Attributes {
        index: usize,
    },
    Help {
        scroll: usize,
    },
    Dir(DirBrowser),
}

#[derive(Clone, Debug)]
pub enum LastAction {
    Search(Vec<u8>),
    Compare,
}

pub struct App {
    pub panes: [Pane; 2],
    pub active: usize,
    pub options: Options,
    pub modal: Option<Modal>,
    pub menu_index: usize,
    pub quit: bool,
    pub message: Option<String>,
    pub rows_per_pane: usize,
    /// Number of bytes on each displayed line.  Always a multiple of 16; the
    /// adaptive layout widens it when the terminal has room for more than one
    /// sixteen-byte group.
    pub bytes_per_row: usize,
    pub compare_mode: bool,
    pub last_action: Option<LastAction>,
    pub edit: Option<EditState>,
}

impl App {
    pub fn new(paths: &[PathBuf], options: Options) -> Result<Self> {
        let mut panes = [Pane::empty(), Pane::empty()];
        for (i, path) in paths.iter().take(2).enumerate() {
            panes[i] = Pane::open(path)
                .map_err(|e| anyhow::anyhow!("Error opening file {}: {e}", path.display()))?;
        }

        let modal = if paths.is_empty() {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            Some(Modal::Dir(DirBrowser::new(cwd, 0)))
        } else {
            None
        };

        Ok(Self {
            panes,
            active: 0,
            options,
            modal,
            menu_index: 3, // the original starts with Help highlighted
            quit: false,
            message: None,
            rows_per_pane: 10,
            bytes_per_row: 16,
            compare_mode: false,
            last_action: None,
            edit: None,
        })
    }

    pub fn any_dirty(&self) -> bool {
        self.panes.iter().any(Pane::dirty)
    }

    pub fn set_message(&mut self, text: impl Into<String>) {
        self.message = Some(text.into());
    }

    /// Adopt the geometry chosen by the renderer and keep both panes inside
    /// their windows.  Called on every draw so a terminal resize is handled
    /// without a keypress.
    pub fn sync_geometry(&mut self, rows: usize, bytes_per_row: usize) {
        self.rows_per_pane = rows.max(1);
        self.bytes_per_row = bytes_per_row.max(16);
        let rows = self.rows_per_pane;
        let bpr = self.bytes_per_row;
        for pane in &mut self.panes {
            if pane.file.is_some() {
                pane.ensure_visible(rows, bpr);
            }
        }
    }

    pub fn status_line(&self) -> String {
        if let Some(msg) = &self.message {
            return msg.clone();
        }
        let mut s = format!(
            "{}  {}  {}  {}  {}  SL:{}",
            self.options.side_view(),
            self.options.direction(),
            self.options.case(),
            self.options.show(),
            self.options.input(),
            self.options.scroll_lock()
        );
        if self.compare_mode {
            s.push_str("  [COMPARE]");
        }
        if self.edit.is_some() {
            s.push_str("  [EDIT]");
        }
        s
    }

    pub fn fkey_line(&self) -> String {
        format!(
            "F1:Help  F2:{}  F3:{}  F4:{}  F5:{}  F6:{}  F8:{}  Tab:Active",
            self.options.case(),
            self.options.direction(),
            self.options.input(),
            self.options.show(),
            self.options.charset(),
            self.options.side_view()
        )
    }

    /// Description shown on the message line for the currently highlighted menu
    /// command.  The original prints it from the table at `CS:0x0D18` whenever
    /// the highlight moves (`CS:0x1D13`); Help's description is the generic
    /// hint, which is why the hint appears on the initial screen (Help is the
    /// default highlight).  `CS:0x0914` embeds the Enter keycap as the bytes
    /// `11 C4 D9`, i.e. `◄─┘` in code page 437.
    pub fn menu_description(&self) -> &'static str {
        match self.menu_index {
            0 => "Compare files starting at current positions",
            1 => "Display directory",
            2 => "Edit (in ASCII) file in active window",
            3 => "Type first letter of command or press ◄─┘ to select hilited command",
            4 => "Switch between Colour and Mono monitors",
            5 => "Repeat Compare or Search",
            6 => "Change case, direction, data entry and show options",
            7 => "Move current position forwards",
            8 => "Move current position backwards",
            9 => "Set current position",
            10 => "Quit CABLE, return to DOS",
            11 => "Search for ASCII string",
            12 => "View file in active window",
            13 => "Write changes to active file back to disk",
            _ => "",
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        // Scroll Lock is absent from most Mac keyboards and crossterm does not
        // report it on Windows, so F7 and Ctrl+L/Ctrl+B are available as
        // equivalents.  Some Windows terminals deliver Ctrl+L as a bare form
        // feed instead of Char('l') with CONTROL set, so accept both forms.
        if is_scroll_lock_toggle(&key) {
            self.toggle_scroll_lock();
            return Ok(());
        }

        if self.modal.is_some() {
            return self.handle_modal_key(key);
        }
        if self.edit.is_some() {
            return self.handle_edit_key(key);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => self.try_quit(),
                KeyCode::Char('s') | KeyCode::Char('S') => self.write_confirm(),
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::F(1) => self.open_help(),
            KeyCode::F(2) => {
                self.options.respect_case = !self.options.respect_case;
                self.set_message(format!("Case: {}", self.options.case()));
            }
            KeyCode::F(3) => {
                self.options.forward = !self.options.forward;
                self.set_message(format!("Direction: {}", self.options.direction()));
            }
            KeyCode::F(4) => {
                self.options.hex_input = !self.options.hex_input;
                self.set_message(format!("Input: {}", self.options.input()));
            }
            KeyCode::F(5) => {
                self.options.conceal = !self.options.conceal;
                self.set_message(format!("Unprintables: {}", self.options.show()));
            }
            KeyCode::F(6) => {
                self.options.ebcdic = !self.options.ebcdic;
                self.set_message(format!("Character set: {}", self.options.charset()));
            }
            KeyCode::F(8) => {
                self.options.cycle_side_view();
                self.set_message(format!("Side view: {}", self.options.side_view()));
            }
            KeyCode::F(9) => {
                self.menu_index = (self.menu_index + MENU.len() - 1) % MENU.len();
                self.message = None;
            }
            KeyCode::F(10) => {
                self.menu_index = (self.menu_index + 1) % MENU.len();
                self.message = None;
            }
            KeyCode::Tab => self.switch_pane(),
            KeyCode::BackTab => self.switch_pane(),
            KeyCode::Enter => {
                let i = self.menu_index;
                self.execute_menu(i);
            }
            KeyCode::Char(' ') => {
                self.menu_index = (self.menu_index + 1) % MENU.len();
                self.message = None;
            }
            KeyCode::Esc => {
                if self.compare_mode {
                    self.compare_mode = false;
                    self.set_message("Compare display off");
                } else {
                    self.message = None;
                }
            }
            KeyCode::Left => self.move_cursor(-1),
            KeyCode::Right => self.move_cursor(1),
            KeyCode::Up => self.move_cursor(-(self.bytes_per_row as i64)),
            KeyCode::Down => self.move_cursor(self.bytes_per_row as i64),
            KeyCode::PageUp => {
                let d = (self.rows_per_pane.max(1) * self.bytes_per_row.max(16)) as i64;
                self.move_cursor(-d);
            }
            KeyCode::PageDown => {
                let d = (self.rows_per_pane.max(1) * self.bytes_per_row.max(16)) as i64;
                self.move_cursor(d);
            }
            KeyCode::Home => self.move_home(),
            KeyCode::End => self.move_end(),
            KeyCode::Char(c) => self.handle_command_char(c),
            _ => {}
        }
        Ok(())
    }

    fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.active)
    }

    fn switch_pane(&mut self) {
        self.active = 1 - self.active;
        self.panes[self.active].clamp_cursor(self.edit.is_some());
        self.panes[self.active].ensure_visible(self.rows_per_pane, self.bytes_per_row);
    }

    fn move_cursor(&mut self, delta: i64) {
        let allow_append = self.edit.is_some();
        let rows = self.rows_per_pane;
        let bpr = self.bytes_per_row;
        let active = self.active;
        Self::nudge_pane(&mut self.panes[active], delta, allow_append, rows, bpr);
        if self.both_panes() {
            let other = 1 - active;
            Self::nudge_pane(&mut self.panes[other], delta, allow_append, rows, bpr);
        }
    }

    /// True when Scroll Lock is on and both windows hold a file.
    fn both_panes(&self) -> bool {
        self.options.move_both && self.panes[0].file.is_some() && self.panes[1].file.is_some()
    }

    fn nudge_pane(p: &mut Pane, delta: i64, allow_append: bool, rows: usize, bpr: usize) {
        if p.file.is_none() {
            return;
        }
        let max = p.cursor_max(allow_append) as i64;
        p.cursor = (p.cursor as i64 + delta).clamp(0, max.max(0)) as u64;
        p.ensure_visible(rows, bpr);
    }

    fn move_home(&mut self) {
        let active = self.active;
        Self::home_pane(&mut self.panes[active]);
        if self.both_panes() {
            let other = 1 - active;
            Self::home_pane(&mut self.panes[other]);
        }
    }

    fn home_pane(p: &mut Pane) {
        if p.file.is_none() {
            return;
        }
        p.cursor = 0;
        p.top = 0;
    }

    fn move_end(&mut self) {
        let allow_append = self.edit.is_some();
        let rows = self.rows_per_pane;
        let bpr = self.bytes_per_row;
        let active = self.active;
        Self::end_pane(&mut self.panes[active], allow_append, rows, bpr);
        if self.both_panes() {
            let other = 1 - active;
            Self::end_pane(&mut self.panes[other], allow_append, rows, bpr);
        }
    }

    fn end_pane(p: &mut Pane, allow_append: bool, rows: usize, bpr: usize) {
        if p.file.is_none() {
            return;
        }
        p.cursor = p.cursor_max(allow_append);
        p.ensure_visible(rows, bpr);
    }

    fn toggle_scroll_lock(&mut self) {
        self.options.move_both = !self.options.move_both;
    }

    fn handle_command_char(&mut self, c: char) {
        let c = c.to_ascii_uppercase();
        match c {
            'C' => self.start_compare(),
            'D' => self.open_dir(),
            'E' => self.start_edit(),
            'H' => self.open_help(),
            'M' => self.open_modes(),
            'N' => self.repeat_last(),
            'O' => self.open_options(),
            '+' => self.open_offset_input(InputAction::Add),
            '-' => self.open_offset_input(InputAction::Sub),
            '=' => self.open_offset_input(InputAction::Goto),
            'Q' => self.try_quit(),
            'S' => self.open_search(),
            'V' => self.open_view(),
            'W' => self.write_confirm(),
            _ => {}
        }
    }

    fn execute_menu(&mut self, index: usize) {
        self.menu_index = index.min(MENU.len() - 1);
        match index {
            0 => self.start_compare(),
            1 => self.open_dir(),
            2 => self.start_edit(),
            3 => self.open_help(),
            4 => self.open_modes(),
            5 => self.repeat_last(),
            6 => self.open_options(),
            7 => self.open_offset_input(InputAction::Add),
            8 => self.open_offset_input(InputAction::Sub),
            9 => self.open_offset_input(InputAction::Goto),
            10 => self.try_quit(),
            11 => self.open_search(),
            12 => self.open_view(),
            13 => self.write_confirm(),
            _ => {}
        }
    }

    fn open_help(&mut self) {
        self.modal = Some(Modal::Help { scroll: 0 });
    }

    fn open_options(&mut self) {
        self.modal = Some(Modal::Options { index: 0 });
    }

    fn open_modes(&mut self) {
        self.modal = Some(Modal::Modes { index: 0 });
    }

    fn open_dir(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        self.modal = Some(Modal::Dir(DirBrowser::new(cwd, self.active)));
    }

    fn open_input(&mut self, title: &str, prompt: &str, action: InputAction, value: &str) {
        self.modal = Some(Modal::Input {
            title: title.to_string(),
            prompt: prompt.to_string(),
            value: value.to_string(),
            action,
        });
    }

    fn open_offset_input(&mut self, action: InputAction) {
        if self.panes[self.active].file.is_none() {
            self.set_message("No file to edit");
            return;
        }
        let (title, prompt) = match action {
            InputAction::Add => (
                "+",
                "+: Enter value to add to current position (if HEX start with H)",
            ),
            InputAction::Sub => (
                "-",
                "-: Enter value to subtract from current position (if HEX start with H)",
            ),
            InputAction::Goto => ("=", "=: Enter new current position (if HEX start with H)"),
            _ => ("", ""),
        };
        self.open_input(title, prompt, action, "");
    }

    fn open_search(&mut self) {
        if self.panes[self.active].file.is_none() {
            self.set_message("No file to search");
            return;
        }
        let (title, prompt) = if self.options.hex_input {
            ("SEARCH", "SEARCH: Enter HEX search string (blanks ignored)")
        } else if self.options.ebcdic {
            ("SEARCH", "SEARCH: Enter EBCDIC search string")
        } else {
            ("SEARCH", "SEARCH: Enter ASCII search string")
        };
        self.open_input(title, prompt, InputAction::Search, "");
    }

    fn open_view(&mut self) {
        self.open_input(
            "VIEW",
            "VIEW: Enter [drive:][path\\]filespec",
            InputAction::View,
            "",
        );
    }

    fn start_edit(&mut self) {
        if self.panes[self.active].file.is_none() {
            self.set_message("No file to edit");
            return;
        }
        if self.options.disasm.is_some() {
            self.set_message("Edit unavailable in disassembly view");
            return;
        }
        self.edit = Some(EditState { nibble: None });
        self.message = Some(
            if self.options.hex_input {
                "EDIT: Enter HEX digits (Esc to quit)"
            } else if self.options.ebcdic {
                "EDIT: Enter EBCDIC characters (Esc to quit)"
            } else {
                "EDIT: Enter ASCII characters (Esc to quit)"
            }
            .to_string(),
        );
    }

    fn handle_edit_key(&mut self, key: KeyEvent) -> Result<()> {
        let rows = self.rows_per_pane;
        let bpr = self.bytes_per_row;
        match key.code {
            KeyCode::Esc => {
                self.edit = None;
                self.message = None;
            }
            KeyCode::ScrollLock | KeyCode::F(7) => self.toggle_scroll_lock(),
            KeyCode::Tab | KeyCode::BackTab => {
                let other = 1 - self.active;
                if self.panes[other].file.is_none() {
                    self.set_message("No file to edit");
                } else {
                    self.active = other;
                    self.panes[self.active].clamp_cursor(true);
                    self.panes[self.active].ensure_visible(rows, bpr);
                    self.message = Some(
                        if self.options.hex_input {
                            "EDIT: Enter HEX digits (Esc to quit)"
                        } else if self.options.ebcdic {
                            "EDIT: Enter EBCDIC characters (Esc to quit)"
                        } else {
                            "EDIT: Enter ASCII characters (Esc to quit)"
                        }
                        .to_string(),
                    );
                }
            }
            KeyCode::Left => self.move_cursor(-1),
            KeyCode::Right => self.move_cursor(1),
            KeyCode::Up => self.move_cursor(-(bpr as i64)),
            KeyCode::Down => self.move_cursor(bpr as i64),
            KeyCode::PageUp => {
                let d = (rows.max(1) * bpr.max(16)) as i64;
                self.move_cursor(-d);
            }
            KeyCode::PageDown => {
                let d = (rows.max(1) * bpr.max(16)) as i64;
                self.move_cursor(d);
            }
            KeyCode::Home => self.move_home(),
            KeyCode::End => self.move_end(),
            KeyCode::Backspace => {
                if let Some(p) = self.active_pane_mut() {
                    p.cursor = p.cursor.saturating_sub(1);
                    p.ensure_visible(rows, bpr);
                }
            }
            KeyCode::Enter => {
                if !self.options.hex_input {
                    self.edit_byte(0x0d);
                }
            }
            KeyCode::Char(c) => {
                if self.options.hex_input {
                    if let Some(v) = c.to_digit(16).map(|v| v as u8) {
                        let Some(state) = self.edit.as_mut() else {
                            return Ok(());
                        };
                        if let Some(hi) = state.nibble.take() {
                            self.edit_byte((hi << 4) | v);
                        } else {
                            state.nibble = Some(v);
                        }
                    }
                } else {
                    match codec::encode_text(&c.to_string(), self.options.ebcdic) {
                        Ok(bytes) if !bytes.is_empty() => self.edit_byte(bytes[0]),
                        Ok(_) => {}
                        Err(e) => self.set_message(e.to_string()),
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn edit_byte(&mut self, value: u8) {
        let idx = self.active;
        let rows = self.rows_per_pane;
        let bpr = self.bytes_per_row;
        if self.panes[idx].file.is_none() {
            self.set_message("No file to edit");
            return;
        }
        let result = {
            let p = &mut self.panes[idx];
            let file = p.file.as_mut().expect("checked above");
            file.write_byte(p.cursor, value)
        };
        match result {
            Ok(()) => {
                let p = &mut self.panes[idx];
                p.cursor = p.cursor.saturating_add(1);
                p.ensure_visible(rows, bpr);
            }
            Err(e) => self.set_message(format!("Error writing file: {e}")),
        }
    }

    fn start_compare(&mut self) {
        if self.panes[0].file.is_none() || self.panes[1].file.is_none() {
            self.set_message("No file to compare");
            return;
        }
        self.compare_mode = true;
        self.last_action = Some(LastAction::Compare);
        self.do_compare(false);
    }

    /// Find the next difference.  `advance` is false for a fresh Compare
    /// command (start at the cursor) and true for Next (step past the
    /// difference already displayed).
    fn do_compare(&mut self, advance: bool) {
        if self.panes[0].file.is_none() || self.panes[1].file.is_none() {
            self.set_message("No file to compare");
            return;
        }
        let cursor = self.panes[self.active].cursor;
        let forward = self.options.forward;
        let start = if !advance {
            cursor
        } else if forward {
            cursor.saturating_add(1)
        } else if cursor == 0 {
            self.set_message("No difference found before end of file");
            return;
        } else {
            cursor - 1
        };
        let active = self.active;

        let result = {
            let (left, right) = self.panes.split_at_mut(1);
            let (a, b) = if active == 0 {
                (left[0].file.as_mut(), right[0].file.as_mut())
            } else {
                (right[0].file.as_mut(), left[0].file.as_mut())
            };
            match (a, b) {
                (Some(a), Some(b)) => compare::find_difference(a, b, start, forward),
                _ => Ok(None),
            }
        };

        match result {
            Ok(Some(offset)) => {
                self.compare_mode = true;
                let rows = self.rows_per_pane;
                let bpr = self.bytes_per_row.max(16) as u64;
                for pane in &mut self.panes {
                    // A length difference is reported at the first byte past
                    // the shorter file, so allow the cursor to sit at `len`.
                    pane.cursor = offset.min(pane.len());
                    if pane.file.is_some() {
                        pane.top = (offset / bpr) * bpr;
                        pane.ensure_visible(rows, bpr as usize);
                    }
                }
                self.message = None;
            }
            Ok(None) => {
                self.set_message("No difference found before end of file");
            }
            Err(e) => self.set_message(format!("Error reading file: {e}")),
        }
    }

    fn repeat_last(&mut self) {
        match self.last_action.clone() {
            Some(LastAction::Search(needle)) => self.perform_search(&needle, true),
            Some(LastAction::Compare) => self.do_compare(true),
            None => self.set_message("No compare/search to repeat"),
        }
    }

    fn perform_search(&mut self, needle: &[u8], advance: bool) {
        if needle.is_empty() {
            self.set_message("Search requires a string");
            return;
        }
        let forward = self.options.forward;
        let respect = self.options.respect_case;
        let ebcdic = self.options.ebcdic;
        let cursor = self.panes[self.active].cursor;
        let start = if !advance {
            cursor
        } else if forward {
            cursor.saturating_add(1)
        } else if cursor == 0 {
            self.set_message("No match found before end of file");
            return;
        } else {
            cursor - 1
        };
        let Some(file) = self.panes[self.active].file.as_mut() else {
            self.set_message("No file to search");
            return;
        };
        match compare::find(file, needle, start, forward, respect, ebcdic) {
            Ok(Some(offset)) => {
                let rows = self.rows_per_pane;
                let bpr = self.bytes_per_row;
                let p = &mut self.panes[self.active];
                p.cursor = offset;
                p.ensure_visible(rows, bpr);
                self.message = None;
            }
            Ok(None) => self.set_message("No match found before end of file"),
            Err(e) => self.set_message(format!("Error reading file: {e}")),
        }
    }

    fn write_confirm(&mut self) {
        let Some(file) = self.panes[self.active].file.as_ref() else {
            self.set_message("No file to edit");
            return;
        };
        if !file.is_dirty() {
            self.set_message("No changes to write");
            return;
        }
        if file.is_read_only() {
            self.set_message("File is read-only - can't write");
            return;
        }
        let path = file.path().display().to_string();
        self.modal = Some(Modal::Confirm {
            title: "WRITE".to_string(),
            text: format!("Do you want to write changes to {path} (Y/N)?"),
            action: ConfirmAction::Write,
        });
    }

    fn do_write(&mut self) {
        let Some(file) = self.panes[self.active].file.as_mut() else {
            self.set_message("No file to edit");
            return;
        };
        match file.save() {
            Ok(len) => self.set_message(format!("{len} bytes written")),
            Err(e) => self.set_message(e.to_string()),
        }
    }

    fn try_quit(&mut self) {
        if self.any_dirty() {
            self.modal = Some(Modal::Confirm {
                title: "QUIT".to_string(),
                text: "Changes lost.  Quit without writing changes? (Y/N)".to_string(),
                action: ConfirmAction::Quit,
            });
        } else {
            self.quit = true;
        }
    }

    fn load_into(&mut self, path: PathBuf, pane: usize) {
        if self.panes[pane].dirty() {
            let name = self.panes[pane].name();
            self.modal = Some(Modal::Confirm {
                title: "REPLACE".to_string(),
                text: format!("Changes to {name} not written.  Replace it? (Y/N)"),
                action: ConfirmAction::Load { path, pane },
            });
            return;
        }
        self.load_into_force(path, pane);
    }

    fn load_into_force(&mut self, path: PathBuf, pane: usize) {
        match Pane::open(&path) {
            Ok(p) => {
                self.panes[pane] = p;
                self.compare_mode = false;
                self.message = None;
            }
            Err(e) => self.set_message(format!("Error opening file: {e}")),
        }
    }

    fn submit_input(&mut self, action: InputAction, value: String) -> Result<()> {
        match action {
            InputAction::Add | InputAction::Sub | InputAction::Goto => {
                let amount = match codec::parse_offset(&value) {
                    Ok(v) => v,
                    Err(e) => {
                        self.set_message(e.to_string());
                        return Ok(());
                    }
                };
                if self.panes[self.active].file.is_none() {
                    self.set_message("No file to edit");
                    return Ok(());
                }
                let rows = self.rows_per_pane;
                let bpr = self.bytes_per_row;
                let p = &mut self.panes[self.active];
                let len = p.len();
                let max = if self.edit.is_some() {
                    len
                } else {
                    len.saturating_sub(1)
                };
                p.cursor = match action {
                    InputAction::Add => p.cursor.saturating_add(amount).min(max),
                    InputAction::Sub => p.cursor.saturating_sub(amount),
                    InputAction::Goto => amount.min(max),
                    _ => p.cursor,
                };
                p.ensure_visible(rows, bpr);
                self.message = None;
            }
            InputAction::Search => {
                if value.trim().is_empty() {
                    self.set_message("Search requires a string");
                    return Ok(());
                }
                let needle = if self.options.hex_input {
                    match codec::parse_hex(&value) {
                        Ok(v) => v,
                        Err(e) => {
                            self.set_message(e.to_string());
                            return Ok(());
                        }
                    }
                } else {
                    match codec::encode_text(&value, self.options.ebcdic) {
                        Ok(v) => v,
                        Err(e) => {
                            self.set_message(e.to_string());
                            return Ok(());
                        }
                    }
                };
                if needle.is_empty() {
                    self.set_message("Search requires a string");
                    return Ok(());
                }
                self.last_action = Some(LastAction::Search(needle.clone()));
                self.perform_search(&needle, false);
            }
            InputAction::View => {
                if value.trim().is_empty() {
                    self.set_message("View requires a filespec");
                    return Ok(());
                }
                let path = PathBuf::from(value.trim());
                let path = if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(&path))
                        .unwrap_or(path)
                };
                if path.is_dir() {
                    self.set_message("Error opening file: is a directory");
                } else {
                    let pane = self.active;
                    self.load_into(path, pane);
                }
            }
        }
        Ok(())
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some(modal) = self.modal.take() else {
            return Ok(());
        };
        match modal {
            Modal::Confirm {
                title,
                text,
                action,
            } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => match action {
                    ConfirmAction::Quit => self.quit = true,
                    ConfirmAction::Write => self.do_write(),
                    ConfirmAction::Load { path, pane } => self.load_into_force(path, pane),
                },
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
                _ => {
                    self.modal = Some(Modal::Confirm {
                        title,
                        text,
                        action,
                    });
                }
            },
            Modal::Input {
                title,
                prompt,
                mut value,
                action,
            } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    self.submit_input(action, value)?;
                }
                KeyCode::Backspace => {
                    value.pop();
                    self.modal = Some(Modal::Input {
                        title,
                        prompt,
                        value,
                        action,
                    });
                }
                KeyCode::Char('u') | KeyCode::Char('U')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    value.clear();
                    self.modal = Some(Modal::Input {
                        title,
                        prompt,
                        value,
                        action,
                    });
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    value.push(c);
                    self.modal = Some(Modal::Input {
                        title,
                        prompt,
                        value,
                        action,
                    });
                }
                _ => {
                    self.modal = Some(Modal::Input {
                        title,
                        prompt,
                        value,
                        action,
                    });
                }
            },
            Modal::Options { mut index } => {
                const N: usize = 11;
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Up => {
                        index = (index + N - 1) % N;
                        self.modal = Some(Modal::Options { index });
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        index = (index + 1) % N;
                        self.modal = Some(Modal::Options { index });
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        self.activate_option(index);
                    }
                    KeyCode::Char(c) => self.option_letter(c.to_ascii_uppercase()),
                    _ => self.modal = Some(Modal::Options { index }),
                }
            }
            Modal::Modes { mut index } => {
                const N: usize = 5;
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Up => {
                        index = (index + N - 1) % N;
                        self.modal = Some(Modal::Modes { index });
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        index = (index + 1) % N;
                        self.modal = Some(Modal::Modes { index });
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => self.activate_mode(index),
                    KeyCode::Char(c) => match c.to_ascii_uppercase() {
                        'A' => self.activate_mode(0),
                        'C' => self.activate_mode(1),
                        'H' => self.activate_mode(2),
                        'M' => self.activate_mode(3),
                        'Q' => self.activate_mode(4),
                        _ => self.modal = Some(Modal::Modes { index }),
                    },
                    _ => self.modal = Some(Modal::Modes { index }),
                }
            }
            Modal::Attributes { mut index } => {
                const N: usize = 3;
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Up => {
                        index = (index + N - 1) % N;
                        self.modal = Some(Modal::Attributes { index });
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        index = (index + 1) % N;
                        self.modal = Some(Modal::Attributes { index });
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        self.options.theme = match index {
                            0 => Theme::Classic,
                            1 => Theme::Amber,
                            _ => Theme::Mono,
                        };
                    }
                    _ => self.modal = Some(Modal::Attributes { index }),
                }
            }
            Modal::Help { mut scroll } => {
                let max = help::LINES.len().saturating_sub(1);
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::F(1) => {}
                    KeyCode::Up => {
                        scroll = scroll.saturating_sub(1);
                        self.modal = Some(Modal::Help { scroll });
                    }
                    KeyCode::Down => {
                        scroll = (scroll + 1).min(max);
                        self.modal = Some(Modal::Help { scroll });
                    }
                    KeyCode::PageUp => {
                        scroll = scroll.saturating_sub(20);
                        self.modal = Some(Modal::Help { scroll });
                    }
                    KeyCode::PageDown => {
                        scroll = (scroll + 20).min(max);
                        self.modal = Some(Modal::Help { scroll });
                    }
                    KeyCode::Home => self.modal = Some(Modal::Help { scroll: 0 }),
                    KeyCode::End => self.modal = Some(Modal::Help { scroll: max }),
                    _ => self.modal = Some(Modal::Help { scroll }),
                }
            }
            Modal::Dir(mut b) => {
                let page = 15usize;
                if b.path_input.is_some() {
                    match key.code {
                        KeyCode::Esc => b.path_input = None,
                        KeyCode::Enter => b.apply_path_input(),
                        KeyCode::Backspace => {
                            if let Some(input) = b.path_input.as_mut() {
                                input.pop();
                            }
                        }
                        KeyCode::Char(c) => {
                            if let Some(input) = b.path_input.as_mut() {
                                input.push(c);
                            }
                        }
                        _ => {}
                    }
                    self.modal = Some(Modal::Dir(b));
                    return Ok(());
                }

                if b.sort_prompt {
                    match key.code {
                        KeyCode::Esc => b.sort_prompt = false,
                        KeyCode::Char(c) => {
                            let key = match c.to_ascii_uppercase() {
                                'N' => Some(SortKey::Name),
                                'E' => Some(SortKey::Ext),
                                'D' => Some(SortKey::Date),
                                'S' => Some(SortKey::Size),
                                _ => None,
                            };
                            if let Some(k) = key {
                                b.set_sort(k);
                            }
                        }
                        _ => {}
                    }
                    self.modal = Some(Modal::Dir(b));
                    return Ok(());
                }

                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Up => {
                        b.move_by(-1);
                        b.ensure_visible(page);
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::Down => {
                        b.move_by(1);
                        b.ensure_visible(page);
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::PageUp => {
                        b.move_by(-(page as isize));
                        b.ensure_visible(page);
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::PageDown => {
                        b.move_by(page as isize);
                        b.ensure_visible(page);
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::Home => {
                        b.home();
                        b.ensure_visible(page);
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::End => {
                        b.end();
                        b.ensure_visible(page);
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::Backspace => {
                        b.parent();
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::Tab => {
                        b.toggle_target();
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        b.sort_prompt = true;
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::Char('/') => {
                        b.begin_path_input();
                        self.modal = Some(Modal::Dir(b));
                    }
                    KeyCode::Enter => {
                        let target = b.target_pane;
                        let view = key.modifiers.contains(KeyModifiers::CONTROL);
                        if view {
                            if let Some(path) = b.current_path() {
                                let pane = 1 - target;
                                self.load_into(path, pane);
                            } else {
                                self.modal = Some(Modal::Dir(b));
                            }
                        } else {
                            match b.activate() {
                                Some(path) => self.load_into(path, target),
                                None => self.modal = Some(Modal::Dir(b)),
                            }
                        }
                    }
                    _ => self.modal = Some(Modal::Dir(b)),
                }
            }
        }
        Ok(())
    }

    fn activate_option(&mut self, index: usize) {
        match index {
            0 => self.options.ebcdic = false,
            1 => self.options.forward = false,
            2 => self.options.conceal = true,
            3 => self.options.ebcdic = true,
            4 => self.options.forward = true,
            5 => self.open_help(),
            6 => self.options.respect_case = false,
            7 => return, // Quit to main menu
            8 => self.options.respect_case = true,
            9 => self.options.conceal = false,
            10 => self.options.hex_input = !self.options.hex_input,
            _ => {}
        }
        if self.modal.is_none() {
            self.modal = Some(Modal::Options { index });
        }
    }

    fn option_letter(&mut self, c: char) {
        match c {
            'A' => self.options.ebcdic = false,
            'E' => self.options.ebcdic = true,
            'B' => self.options.forward = false,
            'F' => self.options.forward = true,
            'C' => self.options.conceal = true,
            'S' => self.options.conceal = false,
            'I' => self.options.respect_case = false,
            'R' => self.options.respect_case = true,
            'X' => self.options.hex_input = !self.options.hex_input,
            'H' => self.open_help(),
            'Q' => return,
            _ => {}
        }
        if self.modal.is_none() {
            self.modal = Some(Modal::Options { index: 0 });
        }
    }

    fn activate_mode(&mut self, index: usize) {
        match index {
            0 => self.modal = Some(Modal::Attributes { index: 0 }),
            1 => {
                self.options.theme = Theme::Classic;
                self.modal = Some(Modal::Modes { index: 1 });
            }
            2 => self.open_help(),
            3 => {
                self.options.theme = Theme::Mono;
                self.modal = Some(Modal::Modes { index: 3 });
            }
            4 => self.modal = None, // Quit closes the Mode menu
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = dir.join(format!("cable-rs-{pid}-{nanos}-{name}"));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn scrolling_uses_wide_lines() {
        let path = temp_file("wide.bin", &vec![0u8; 4096]);
        let mut pane = Pane::open(&path).unwrap();

        // Move far enough down that only the last eight 32-byte lines fit.
        pane.cursor = 1000;
        pane.ensure_visible(8, 32);
        assert_eq!(pane.top, 768);

        // Still inside the page: no scroll.
        pane.cursor = 900;
        pane.ensure_visible(8, 32);
        assert_eq!(pane.top, 768);

        // Move above the window: jump to the line containing the cursor.
        pane.cursor = 100;
        pane.ensure_visible(8, 32);
        assert_eq!(pane.top, 96);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn scroll_lock_accepts_keyboard_aliases() {
        let plain = KeyEvent::new;
        assert!(is_scroll_lock_toggle(&plain(
            KeyCode::ScrollLock,
            KeyModifiers::empty()
        )));
        assert!(is_scroll_lock_toggle(&plain(
            KeyCode::F(7),
            KeyModifiers::empty()
        )));
        // Some Windows terminals deliver Ctrl+L as a bare form feed.
        assert!(is_scroll_lock_toggle(&plain(
            KeyCode::Char('\u{c}'),
            KeyModifiers::empty()
        )));
        assert!(is_scroll_lock_toggle(&plain(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL
        )));
        assert!(is_scroll_lock_toggle(&plain(
            KeyCode::Char('B'),
            KeyModifiers::CONTROL
        )));
        // A plain `l` must still reach the normal character commands.
        assert!(!is_scroll_lock_toggle(&plain(
            KeyCode::Char('l'),
            KeyModifiers::empty()
        )));
    }

    #[test]
    fn side_view_cycles_back_to_characters() {
        let mut options = Options::default();
        assert_eq!(options.side_view(), "ASCII");
        for expected in ["x86", "x64", "ARM", "Thumb", "ARM64"] {
            options.cycle_side_view();
            assert_eq!(options.side_view(), expected);
        }
        options.cycle_side_view();
        assert!(options.disasm.is_none());
        assert_eq!(options.side_view(), "ASCII");
    }

    #[test]
    fn disassembly_view_blocks_editing() {
        let path = temp_file("disasm.bin", &[0u8; 64]);
        let mut options = Options::default();
        options.disasm = Some(crate::disasm::Cpu::X86);
        let mut app = App::new(&[path.clone()], options).unwrap();
        app.start_edit();
        assert!(app.edit.is_none());
        assert!(app.message.as_deref().unwrap_or("").contains("disassembly"));
        fs::remove_file(path).unwrap();
    }
}
