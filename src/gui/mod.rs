//! Desktop GUI front end for CABLE, built on egui/eframe.
//!
//! The terminal UI remains available in `cable-rs`; this module reuses the
//! same core state machine and adds a native window, menus, toolbar, mouse
//! navigation and modern dialogs.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use eframe::egui;
use egui::{Align2, Color32, FontId, Key, Rect, Sense, Stroke, StrokeKind, TextFormat, pos2, vec2};
use ratatui::style::Color as TuiColor;

use crate::app::{App as CoreApp, InputAction, Options, Theme};
use crate::cli;
use crate::codec;
use crate::disasm::{self, Cpu};
use crate::help;

const BASE_FONT_SIZE: f32 = 14.0;
/// Desktop order: the modern palettes first, the original ones after.
const THEMES: [Theme; 5] = [
    Theme::ModernDark,
    Theme::ModernLight,
    Theme::Classic,
    Theme::Amber,
    Theme::Mono,
];
const PANE_GAP: f32 = 10.0;
/// The original shows a 64-byte preview of each file in the band between the
/// two windows, starting sixteen bytes before that window's cursor.
const PREVIEW_LEN: usize = 64;
const PREVIEW_BEFORE: usize = 16;

/// Cell geometry shared by rendering and hit testing.
#[derive(Clone, Copy)]
struct Grid {
    rows: usize,
    bpr: usize,
    char_w: f32,
    row_h: f32,
    header_h: f32,
}
/// Compact key legend for the status bar; the full list is under Help.
const FKEY_HINT: &str =
    "F1 Help · F2 Case · F3 Dir · F4 Input · F5 Show · F6 Char · F8 Side · Tab Pane";
const PANE_PAD: f32 = 10.0;

/// Run the desktop application.
pub fn run() -> Result<()> {
    let parsed = cli::parse(std::env::args().skip(1).collect()).map_err(anyhow::Error::msg)?;
    match parsed.action {
        cli::Action::Help => {
            println!("{}", cli::usage());
            return Ok(());
        }
        cli::Action::Version => {
            println!("cable-gui {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        cli::Action::Bench(paths) => return crate::bench::run(&paths),
        cli::Action::Run => {}
    }

    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([820.0, 520.0])
            .with_title("CABLE"),
        ..Default::default()
    };
    eframe::run_native(
        "CABLE",
        native,
        Box::new(move |cc| {
            Ok(Box::new(GuiApp::new(
                cc,
                &parsed.paths,
                parsed.options.clone(),
                parsed.theme_explicit,
            )?))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

struct GuiApp {
    core: CoreApp,
    dialog: Option<Dialog>,
    font_size: f32,
    allow_close: bool,
    scroll_accum: f32,
    /// Last title sent to the window manager, so the title bar only updates
    /// when the active document changes.
    window_title: String,
    /// Set from `CABLE_SCREENSHOT`: capture the window once it has settled,
    /// write it to this path and close.  Used for documentation and CI.
    screenshot: Option<ScreenshotRequest>,
    screenshot_done: Arc<AtomicBool>,
}

struct ScreenshotRequest {
    path: PathBuf,
    frames_left: u32,
}

enum Dialog {
    Search {
        text: String,
        focus: bool,
    },
    Offset {
        mode: OffsetMode,
        text: String,
        focus: bool,
    },
    ConfirmQuit,
    ConfirmOpen {
        pane: usize,
        path: PathBuf,
    },
    Options,
    Help,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OffsetMode {
    Goto,
    Add,
    Sub,
}

#[derive(Debug)]
enum Action {
    Open(usize),
    Save,
    Quit,
    Search,
    Offset(OffsetMode),
    Compare,
    Repeat,
    Edit,
    Help,
    Options,
    About,
    Theme(Theme),
    SideView(Option<Cpu>),
    ToggleCase,
    ToggleDirection,
    ToggleInput,
    ToggleConceal,
    ToggleCharset,
    ToggleScrollBoth,
    SearchText(String),
    OffsetText(OffsetMode, String),
    Load { pane: usize, path: PathBuf },
    CloseApp,
    Zoom(f32),
}

impl GuiApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        paths: &[PathBuf],
        options: Options,
        theme_explicit: bool,
    ) -> Result<Self> {
        Self::new_inner(&cc.egui_ctx, paths, options, theme_explicit)
    }

    fn new_inner(
        ctx: &egui::Context,
        paths: &[PathBuf],
        options: Options,
        theme_explicit: bool,
    ) -> Result<Self> {
        ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = vec2(8.0, 6.0);
            style.spacing.button_padding = vec2(10.0, 5.0);
        });

        let mut options = options;
        // The desktop front end opens in its own palette unless the command
        // line picked one; the terminal keeps the original colour default.
        if !theme_explicit {
            options.theme = Theme::ModernDark;
        }
        let mut core = CoreApp::new(paths, options)?;
        // The directory browser modal belongs to the terminal front end.
        core.modal = None;

        let screenshot = std::env::var_os("CABLE_SCREENSHOT").map(|path| ScreenshotRequest {
            path: PathBuf::from(path),
            frames_left: std::env::var("CABLE_SCREENSHOT_FRAMES")
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(12),
        });

        let app = Self {
            core,
            dialog: None,
            font_size: BASE_FONT_SIZE,
            allow_close: false,
            scroll_accum: 0.0,
            window_title: "CABLE".to_string(),
            screenshot,
            screenshot_done: Arc::new(AtomicBool::new(false)),
        };
        app.sync_chrome(ctx);
        Ok(app)
    }

    /// Match the window chrome to the palette chosen for the data panes.
    fn sync_chrome(&self, ctx: &egui::Context) {
        let light = self.core.options.theme == Theme::ModernLight;
        let mut visuals = if light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        if light {
            visuals.panel_fill = Color32::from_rgb(238, 240, 244);
            visuals.window_fill = Color32::from_rgb(255, 255, 255);
        } else {
            visuals.panel_fill = Color32::from_rgb(24, 25, 30);
            visuals.window_fill = Color32::from_rgb(32, 33, 40);
        }
        visuals.window_stroke = Stroke::new(
            1.0,
            if light {
                Color32::from_rgb(206, 210, 220)
            } else {
                Color32::from_rgb(58, 60, 70)
            },
        );
        ctx.set_theme(if light {
            egui::ThemePreference::Light
        } else {
            egui::ThemePreference::Dark
        });
        ctx.set_visuals(visuals);
    }

    fn theme(&self) -> Theme {
        self.core.options.theme
    }

    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Open(pane) => self.open_file(pane),
            Action::Save => self.core.save_direct(),
            Action::Quit => self.request_quit(ctx),
            Action::Search => {
                self.dialog = Some(Dialog::Search {
                    text: String::new(),
                    focus: true,
                });
            }
            Action::Offset(mode) => {
                self.dialog = Some(Dialog::Offset {
                    mode,
                    text: String::new(),
                    focus: true,
                });
            }
            Action::Compare => self.core.start_compare(),
            Action::Repeat => self.core.repeat_last(),
            Action::Edit => self.core.start_edit(),
            Action::Help => self.dialog = Some(Dialog::Help),
            Action::Options => self.dialog = Some(Dialog::Options),
            Action::About => self.dialog = Some(Dialog::About),
            Action::Theme(theme) => {
                self.core.options.theme = theme;
                self.core.set_message(format!("Theme: {}", theme.name()));
                self.sync_chrome(ctx);
            }
            Action::SideView(view) => {
                self.core.options.disasm = view;
                self.core
                    .set_message(format!("Side view: {}", self.core.options.side_view()));
            }
            Action::ToggleCase => {
                self.core.options.respect_case = !self.core.options.respect_case;
                self.core
                    .set_message(format!("Case: {}", self.core.options.case()));
            }
            Action::ToggleDirection => {
                self.core.options.forward = !self.core.options.forward;
                self.core
                    .set_message(format!("Direction: {}", self.core.options.direction()));
            }
            Action::ToggleInput => {
                self.core.options.hex_input = !self.core.options.hex_input;
                self.core
                    .set_message(format!("Input: {}", self.core.options.input()));
            }
            Action::ToggleConceal => {
                self.core.options.conceal = !self.core.options.conceal;
                self.core
                    .set_message(format!("Unprintables: {}", self.core.options.show()));
            }
            Action::ToggleCharset => {
                self.core.options.ebcdic = !self.core.options.ebcdic;
                self.core
                    .set_message(format!("Character set: {}", self.core.options.charset()));
            }
            Action::ToggleScrollBoth => {
                self.core.options.move_both = !self.core.options.move_both;
                self.core.set_message(format!(
                    "Scroll: {}",
                    if self.core.options.move_both {
                        "both windows"
                    } else {
                        "active window"
                    }
                ));
            }
            Action::SearchText(text) => {
                if let Err(e) = self.core.search_value(&text) {
                    self.core.set_message(e.to_string());
                }
                self.dialog = None;
            }
            Action::OffsetText(mode, text) => {
                let action = match mode {
                    OffsetMode::Goto => InputAction::Goto,
                    OffsetMode::Add => InputAction::Add,
                    OffsetMode::Sub => InputAction::Sub,
                };
                if let Err(e) = self.core.offset_value(action, &text) {
                    self.core.set_message(e.to_string());
                }
                self.dialog = None;
            }
            Action::Load { pane, path } => self.core.open_in(pane, path),
            Action::CloseApp => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Zoom(delta) => {
                self.font_size = (self.font_size + delta).clamp(9.0, 28.0);
            }
        }
    }

    fn open_file(&mut self, pane: usize) {
        let mut dialog = rfd::FileDialog::new();
        if let Ok(cwd) = std::env::current_dir() {
            dialog = dialog.set_directory(cwd);
        }
        let Some(path) = dialog.pick_file() else {
            return;
        };
        if self.core.panes[pane].dirty() {
            self.dialog = Some(Dialog::ConfirmOpen { pane, path });
        } else {
            self.core.open_in(pane, path);
        }
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        if self.core.any_dirty() {
            self.dialog = Some(Dialog::ConfirmQuit);
        } else {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ------------------------------------------------------------------
    // Keyboard

    fn handle_keys(&mut self, ctx: &egui::Context) {
        // Dialogs and open menus own the keyboard while they are up.
        if self.dialog.is_some() || ctx.any_popup_open() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if !pressed {
                continue;
            }

            let command = modifiers.command || modifiers.ctrl;
            if command {
                match key {
                    Key::O => self.apply(Action::Open(self.core.active), ctx),
                    Key::S => self.apply(Action::Save, ctx),
                    Key::Q => self.apply(Action::Quit, ctx),
                    Key::F => {
                        consume(ctx, modifiers, key);
                        self.apply(Action::Search, ctx);
                    }
                    Key::G => {
                        consume(ctx, modifiers, key);
                        self.apply(Action::Offset(OffsetMode::Goto), ctx);
                    }
                    Key::Plus | Key::Equals => self.apply(Action::Zoom(1.0), ctx),
                    Key::Minus => self.apply(Action::Zoom(-1.0), ctx),
                    _ => {}
                }
                continue;
            }

            // While editing, every key belongs to the byte editor.
            if self.core.edit.is_some() {
                if let Some(mapped) = map_key(key, modifiers) {
                    // Keep egui widgets from also acting on navigation keys.
                    consume(ctx, modifiers, key);
                    release_focus(ctx);
                    let _ = self.core.handle_key(mapped);
                }
                continue;
            }

            match key {
                Key::F1 => self.apply(Action::Help, ctx),
                Key::F2 => self.apply(Action::ToggleCase, ctx),
                Key::F3 => self.apply(Action::ToggleDirection, ctx),
                Key::F4 => self.apply(Action::ToggleInput, ctx),
                Key::F5 => self.apply(Action::ToggleConceal, ctx),
                Key::F6 => self.apply(Action::ToggleCharset, ctx),
                Key::F7 => self.apply(Action::ToggleScrollBoth, ctx),
                Key::F8 => {
                    self.core.options.cycle_side_view();
                    self.core
                        .set_message(format!("Side view: {}", self.core.options.side_view()));
                }
                Key::Escape
                | Key::Tab
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::PageUp
                | Key::PageDown
                | Key::Home
                | Key::End => {
                    if let Some(mapped) = map_key(key, modifiers) {
                        // Tab would otherwise move keyboard focus to the menu or
                        // toolbar, and the arrows would then change that widget.
                        consume(ctx, modifiers, key);
                        release_focus(ctx);
                        let _ = self.core.handle_key(mapped);
                    }
                }
                Key::Enter | Key::Space => {
                    consume(ctx, modifiers, key);
                    release_focus(ctx);
                }
                Key::A => {
                    consume(ctx, modifiers, key);
                    self.apply(Action::About, ctx)
                }
                Key::C => self.apply(Action::Compare, ctx),
                Key::E => self.apply(Action::Edit, ctx),
                Key::H => self.apply(Action::Help, ctx),
                Key::M | Key::O => self.apply(Action::Options, ctx),
                Key::N => self.apply(Action::Repeat, ctx),
                Key::Q => self.apply(Action::Quit, ctx),
                Key::S => {
                    consume(ctx, modifiers, key);
                    self.apply(Action::Search, ctx);
                }
                Key::V => self.apply(Action::Open(self.core.active), ctx),
                Key::W => self.apply(Action::Save, ctx),
                Key::Equals => {
                    consume(ctx, modifiers, key);
                    self.apply(Action::Offset(OffsetMode::Goto), ctx);
                }
                Key::Plus => {
                    consume(ctx, modifiers, key);
                    self.apply(Action::Offset(OffsetMode::Add), ctx);
                }
                Key::Minus => {
                    consume(ctx, modifiers, key);
                    self.apply(Action::Offset(OffsetMode::Sub), ctx);
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // Chrome

    fn menu_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open in active pane…").clicked() {
                    action = Some(Action::Open(self.core.active));
                    ui.close();
                }
                if ui.button("Open in other pane…").clicked() {
                    action = Some(Action::Open(1 - self.core.active));
                    ui.close();
                }
                ui.separator();
                if ui.button("Save changes").clicked() {
                    action = Some(Action::Save);
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    action = Some(Action::Quit);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Edit active file").clicked() {
                    action = Some(Action::Edit);
                    ui.close();
                }
                if ui.button("Write changes").clicked() {
                    action = Some(Action::Save);
                    ui.close();
                }
            });
            ui.menu_button("Search", |ui| {
                if ui.button("Search…").clicked() {
                    action = Some(Action::Search);
                    ui.close();
                }
                if ui.button("Repeat compare or search").clicked() {
                    action = Some(Action::Repeat);
                    ui.close();
                }
                ui.separator();
                if ui.button("Compare files").clicked() {
                    action = Some(Action::Compare);
                    ui.close();
                }
                ui.separator();
                if ui.button("Go to position…").clicked() {
                    action = Some(Action::Offset(OffsetMode::Goto));
                    ui.close();
                }
                if ui.button("Add to position…").clicked() {
                    action = Some(Action::Offset(OffsetMode::Add));
                    ui.close();
                }
                if ui.button("Subtract from position…").clicked() {
                    action = Some(Action::Offset(OffsetMode::Sub));
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                ui.menu_button("Theme", |ui| {
                    for theme in THEMES {
                        if ui
                            .selectable_label(self.core.options.theme == theme, theme.name())
                            .clicked()
                        {
                            action = Some(Action::Theme(theme));
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Side view", |ui| {
                    let current = self.core.options.disasm;
                    if ui
                        .selectable_label(current.is_none(), "ASCII / EBCDIC")
                        .clicked()
                    {
                        action = Some(Action::SideView(None));
                        ui.close();
                    }
                    for cpu in [Cpu::X86, Cpu::X64, Cpu::Arm, Cpu::Thumb, Cpu::Arm64] {
                        if ui
                            .selectable_label(current == Some(cpu), cpu.label())
                            .clicked()
                        {
                            action = Some(Action::SideView(Some(cpu)));
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui
                    .selectable_label(self.core.options.respect_case, "Respect case")
                    .clicked()
                {
                    action = Some(Action::ToggleCase);
                }
                if ui
                    .selectable_label(self.core.options.forward, "Search/compare forward")
                    .clicked()
                {
                    action = Some(Action::ToggleDirection);
                }
                if ui
                    .selectable_label(self.core.options.hex_input, "Hex input")
                    .clicked()
                {
                    action = Some(Action::ToggleInput);
                }
                if ui
                    .selectable_label(!self.core.options.conceal, "Show control pictures")
                    .clicked()
                {
                    action = Some(Action::ToggleConceal);
                }
                if ui
                    .selectable_label(self.core.options.ebcdic, "EBCDIC (CP037)")
                    .clicked()
                {
                    action = Some(Action::ToggleCharset);
                }
                if ui
                    .selectable_label(self.core.options.move_both, "Scroll both windows")
                    .clicked()
                {
                    action = Some(Action::ToggleScrollBoth);
                }
                ui.separator();
                if ui.button("Larger text").clicked() {
                    action = Some(Action::Zoom(1.0));
                }
                if ui.button("Smaller text").clicked() {
                    action = Some(Action::Zoom(-1.0));
                }
            });
            ui.menu_button("Mode", |ui| {
                if ui.button("Options…").clicked() {
                    action = Some(Action::Options);
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("CABLE help").clicked() {
                    action = Some(Action::Help);
                    ui.close();
                }
                if ui.button("About").clicked() {
                    action = Some(Action::About);
                    ui.close();
                }
            });
        });
        action
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("Open").clicked() {
                action = Some(Action::Open(self.core.active));
            }
            if ui.button("Save").clicked() {
                action = Some(Action::Save);
            }
            ui.separator();
            if ui.button("Search").clicked() {
                action = Some(Action::Search);
            }
            if ui.button("Compare").clicked() {
                action = Some(Action::Compare);
            }
            if ui.button("Edit").clicked() {
                action = Some(Action::Edit);
            }
            if ui.button("Go to").clicked() {
                action = Some(Action::Offset(OffsetMode::Goto));
            }
            ui.separator();
            ui.label("Theme");
            let before = self.core.options.theme;
            egui::ComboBox::from_id_salt("theme")
                .selected_text(before.name())
                .show_ui(ui, |ui| {
                    for theme in THEMES {
                        ui.selectable_value(&mut self.core.options.theme, theme, theme.name());
                    }
                });
            if self.core.options.theme != before {
                action = Some(Action::Theme(self.core.options.theme));
            }
            ui.separator();
            ui.label(format!("Side: {}", self.core.options.side_view()));
            ui.label(format!("Input: {}", self.core.options.input()));
            ui.label(format!("Charset: {}", self.core.options.charset()));
        });
        action
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme();
        let accent = tui_color(theme.accent());
        let dim = tui_color(theme.dim());
        let hint_size = self.font_size * 0.72;
        // Reserve room for the F-key hint first, so a long parameter line or
        // message is truncated instead of pushing the hint off the window.
        let hint_width = ui
            .painter()
            .layout_no_wrap(
                FKEY_HINT.to_owned(),
                egui::FontId::monospace(hint_size),
                dim,
            )
            .size()
            .x;
        ui.horizontal(|ui| {
            let left_width = (ui.available_width() - hint_width - 16.0).max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(left_width, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(self.core.parameter_line())
                                .monospace()
                                .color(accent),
                        )
                        .truncate(),
                    );
                    if let Some(msg) = &self.core.message {
                        ui.add_space(12.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(msg)
                                    .monospace()
                                    .color(tui_color(theme.header_fg())),
                            )
                            .truncate(),
                        );
                    }
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(FKEY_HINT).size(hint_size).color(dim));
            });
        });
    }

    // ------------------------------------------------------------------
    // Panes

    fn panes(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let avail = ui.available_rect_before_wrap();
        if avail.width() < 10.0 || avail.height() < 10.0 {
            return None;
        }

        let font = FontId::monospace(self.font_size);
        let (char_w, row_h) = ui
            .ctx()
            .fonts_mut(|f| (f.glyph_width(&font, '0'), f.row_height(&font)));

        let header_h = row_h + 12.0;
        // Two preview lines with a little padding, matching the six-row band of
        // the original screen in spirit while keeping modern proportions.
        let band_h = 2.0 * row_h + 14.0;
        let pane_h = (avail.height() - band_h - 2.0 * PANE_GAP) / 2.0;
        let rows = (((pane_h - header_h - 2.0 * PANE_PAD) / row_h).floor() as usize).max(1);
        let bpr = self.bytes_per_row(avail.width(), char_w);

        self.core.sync_geometry(rows, bpr);

        let grid = Grid {
            rows,
            bpr,
            char_w,
            row_h,
            header_h,
        };
        let top = Rect::from_min_size(avail.min, vec2(avail.width(), pane_h));
        let band_y = avail.min.y + pane_h + PANE_GAP;
        let band = Rect::from_min_size(pos2(avail.min.x, band_y), vec2(avail.width(), band_h));
        let bottom = Rect::from_min_size(
            pos2(avail.min.x, band_y + band_h + PANE_GAP),
            vec2(avail.width(), pane_h),
        );
        action = action.or(self.pane(ui, 0, top, grid, &font));
        self.preview_band(ui, band, grid, &font);
        action = action.or(self.pane(ui, 1, bottom, grid, &font));

        ui.allocate_space(avail.size());
        action
    }

    /// Draw the preview band between the two windows: one line per file, each
    /// showing bytes `cursor-16 ..= cursor+47` as characters.  The cursor byte
    /// is the seventeenth cell on both lines, so the two files line up.
    fn preview_band(&mut self, ui: &mut egui::Ui, rect: Rect, grid: Grid, font: &FontId) {
        let theme = self.theme();
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 4.0, tui_color(theme.bg()));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, tui_color(theme.border())),
            StrokeKind::Inside,
        );

        let label_chars = 28.0;
        let preview_x = rect.left() + PANE_PAD + (label_chars + 1.0) * grid.char_w;
        for index in 0..2 {
            let y = rect.top() + 6.0 + index as f32 * grid.row_h;
            let active = self.core.active == index;
            let cursor = self.core.panes[index].cursor;
            let label_color = if active {
                tui_color(theme.accent())
            } else {
                tui_color(theme.dim())
            };
            painter.text(
                pos2(rect.left() + PANE_PAD, y),
                Align2::LEFT_TOP,
                format!("Hex {cursor:08X}  Dec {cursor:>10}"),
                font.clone(),
                label_color,
            );

            let cells = self.preview_cells(index);
            for (i, ch) in cells.iter().enumerate() {
                let x = preview_x + i as f32 * grid.char_w;
                if x + grid.char_w > rect.right() - PANE_PAD {
                    break;
                }
                let at_cursor = i == PREVIEW_BEFORE;
                let (fg, bg) = if at_cursor {
                    (
                        tui_color(theme.cursor_fg()),
                        Some(tui_color(theme.cursor_bg())),
                    )
                } else if active {
                    (tui_color(theme.fg()), None)
                } else {
                    (tui_color(theme.dim()), None)
                };
                if let Some(bg) = bg {
                    painter.rect_filled(
                        Rect::from_min_size(pos2(x, y), vec2(grid.char_w, grid.row_h)),
                        0.0,
                        bg,
                    );
                }
                painter.text(pos2(x, y), Align2::LEFT_TOP, *ch, font.clone(), fg);
            }
        }
    }

    /// The 64 characters of one file's preview line; blanks outside the file.
    fn preview_cells(&mut self, index: usize) -> Vec<char> {
        let cursor = self.core.panes[index].cursor;
        let start = cursor as i64 - PREVIEW_BEFORE as i64;
        let ebcdic = self.core.options.ebcdic;
        let conceal = self.core.options.conceal;
        let mut cells = Vec::with_capacity(PREVIEW_LEN);
        for i in 0..PREVIEW_LEN {
            let offset = start + i as i64;
            let byte = if offset < 0 {
                None
            } else {
                self.core.panes[index]
                    .file
                    .as_mut()
                    .and_then(|file| file.read_byte(offset as u64).ok().flatten())
            };
            cells.push(match byte {
                Some(byte) => codec::display_byte(byte, ebcdic, conceal),
                None => ' ',
            });
        }
        cells
    }

    fn bytes_per_row(&self, width: f32, char_w: f32) -> usize {
        let max_chars = ((width - 2.0 * PANE_PAD) / char_w).floor().max(40.0) as usize;
        bytes_per_row_for(max_chars)
    }

    fn pane(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        rect: Rect,
        grid: Grid,
        font: &FontId,
    ) -> Option<Action> {
        let Grid {
            rows,
            bpr,
            char_w,
            row_h,
            header_h,
        } = grid;
        let mut action = None;
        let theme = self.theme();
        let active = self.core.active == index;
        let bg = tui_color(theme.bg());
        let fg = tui_color(theme.fg());
        let dim = tui_color(theme.dim());
        let accent = tui_color(theme.accent());
        let border = tui_color(theme.border());

        let painter = ui.painter();
        painter.rect_filled(rect, 4.0, bg);
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, if active { border } else { bg }),
            StrokeKind::Inside,
        );

        let header = pane_header_rect(rect, header_h, index);
        let header_bg = if active {
            tui_color(theme.bar_bg())
        } else {
            bg
        };
        let header_fg = if active {
            tui_color(theme.bar_fg())
        } else {
            dim
        };
        painter.rect_filled(header, 4.0, header_bg);

        let pane_name = self.core.panes[index].name();
        let len = self.core.panes[index].len();
        let cursor = self.core.panes[index].cursor;
        let mut title = format!("{pane_name}  [{len} bytes]");
        if self.core.panes[index].dirty() {
            title.push_str(" *");
        }
        if self.core.panes[index]
            .file
            .as_ref()
            .map(|f| f.is_read_only())
            .unwrap_or(false)
        {
            title.push_str(" (read-only)");
        }
        let flags = format!(
            "Hex {:08X}  Dec {}{}",
            cursor,
            cursor,
            if self.core.compare_mode { "  CMP" } else { "" }
        );
        painter.text(
            header.left_center() + vec2(PANE_PAD, 0.0),
            Align2::LEFT_CENTER,
            title,
            font.clone(),
            header_fg,
        );
        painter.text(
            header.right_center() - vec2(PANE_PAD, 0.0),
            Align2::RIGHT_CENTER,
            flags,
            font.clone(),
            header_fg,
        );

        let body = pane_body_rect(rect, header_h, index);
        let body_bg = painter.with_clip_rect(body);
        body_bg.rect_filled(body, 0.0, bg);

        let response = ui.interact(
            body,
            ui.scope_id().with(("cable-pane", index)),
            Sense::click_and_drag(),
        );
        if response.clicked() || response.dragged() {
            self.core.active = index;
            if let Some(pos) = response.interact_pointer_pos()
                && let Some(offset) = self.hit_test(index, pos, body, grid)
            {
                self.core.set_cursor(index, offset);
            }
        }
        if response.double_clicked() {
            // An empty window asks for a file; a loaded one starts byte editing
            // at the clicked position, like the terminal's E command.
            if self.core.panes[index].file.is_none() {
                action = Some(Action::Open(index));
            } else if !self.core.compare_mode {
                self.core.start_edit();
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.scroll_accum += scroll;
                let lines = (self.scroll_accum / row_h).trunc();
                if lines != 0.0 {
                    self.scroll_accum -= lines * row_h;
                    let delta = -(lines as i64) * bpr as i64;
                    self.core.active = index;
                    self.core.move_cursor(delta);
                }
            }
        }

        if self.core.panes[index].file.is_none() {
            painter.text(
                body.center(),
                Align2::CENTER_CENTER,
                "--- empty ---\nDouble-click to open a file",
                font.clone(),
                dim,
            );
            return action;
        }
        if len == 0 {
            painter.text(
                body.center(),
                Align2::CENTER_CENTER,
                "empty file",
                font.clone(),
                dim,
            );
            return action;
        }

        let origin = body.min + vec2(PANE_PAD, PANE_PAD * 0.5);
        let rows_origin = origin.y;
        let hex_x = origin.x + 9.0 * char_w;
        let groups = bpr / 16;
        let hex_width = (3 * bpr + groups.saturating_sub(1)) as f32;
        let ascii_x = hex_x + (hex_width + 2.0) * char_w;

        let top = self.core.panes[index].top;
        let cursor = self.core.panes[index].cursor;
        let compare = self.core.compare_mode;
        let ebcdic = self.core.options.ebcdic;
        let conceal = self.core.options.conceal;

        for row in 0..rows {
            let y = rows_origin + row as f32 * row_h;
            if y + row_h > body.max.y {
                break;
            }
            let offset = top.saturating_add((row as u64) * bpr as u64);
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;

            let normal = TextFormat {
                font_id: font.clone(),
                color: fg,
                ..Default::default()
            };
            let dim_fmt = TextFormat {
                font_id: font.clone(),
                color: dim,
                ..Default::default()
            };
            let accent_fmt = TextFormat {
                font_id: font.clone(),
                color: accent,
                ..Default::default()
            };
            let cursor_fmt = TextFormat {
                font_id: font.clone(),
                color: tui_color(theme.cursor_fg()),
                background: tui_color(theme.cursor_bg()),
                ..Default::default()
            };
            let diff_fmt = TextFormat {
                font_id: font.clone(),
                color: tui_color(theme.diff_fg()),
                background: tui_color(theme.diff_bg()),
                ..Default::default()
            };

            job.append(&format!("{offset:08X} "), 0.0, accent_fmt.clone());

            let mut bytes = Vec::with_capacity(bpr);
            for i in 0..bpr {
                let off = offset + i as u64;
                bytes.push(self.byte_at(index, off));
            }
            let mut diff = vec![false; bpr];
            if compare {
                for i in 0..bpr {
                    let off = offset + i as u64;
                    diff[i] = bytes[i] != self.byte_at(1 - index, off);
                }
            }

            for i in 0..bpr {
                if i > 0 && i % 16 == 0 {
                    job.append(" ", 0.0, normal.clone());
                }
                let off = offset + i as u64;
                let style = if active && off == cursor {
                    cursor_fmt.clone()
                } else if diff[i] {
                    diff_fmt.clone()
                } else {
                    normal.clone()
                };
                match bytes[i] {
                    Some(value) => {
                        job.append(&format!("{value:02X}"), 0.0, style);
                    }
                    None => job.append("**", 0.0, dim_fmt.clone()),
                }
                job.append(" ", 0.0, normal.clone());
            }

            job.append("  ", 0.0, normal.clone());

            if let Some(cpu) = self.core.options.disasm {
                let mut raw = Vec::with_capacity(bpr);
                for byte in &bytes {
                    if let Some(byte) = byte {
                        raw.push(*byte);
                    } else {
                        break;
                    }
                }
                let text = disasm::disassemble(cpu, &raw)
                    .into_iter()
                    .map(|i| i.text)
                    .collect::<Vec<_>>()
                    .join("  ");
                let cursor_row = cursor >= offset && cursor < offset + bpr as u64;
                let style = if active && cursor_row {
                    accent_fmt.clone()
                } else if diff.iter().any(|d| *d) {
                    diff_fmt.clone()
                } else {
                    normal.clone()
                };
                job.append(&text, 0.0, style);
            }

            let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
            body_bg.galley(pos2(origin.x, y), galley, fg);

            if self.core.options.disasm.is_none() {
                // The character column is drawn cell by cell.  CP437 graphics
                // fall back to a proportional font, so a single layout run
                // would drift; painting each glyph on its own cell boundary
                // keeps the table square.
                let cursor_cell = if active && cursor >= offset && cursor < offset + bpr as u64 {
                    Some((cursor - offset) as usize)
                } else {
                    None
                };
                for i in 0..bpr {
                    let cell = (i + i / 16) as f32;
                    let x = ascii_x + cell * char_w;
                    let (fgc, bgc) = if Some(i) == cursor_cell {
                        (
                            tui_color(theme.cursor_fg()),
                            Some(tui_color(theme.cursor_bg())),
                        )
                    } else if diff[i] {
                        (tui_color(theme.diff_fg()), Some(tui_color(theme.diff_bg())))
                    } else {
                        (fg, None)
                    };
                    if let Some(bgc) = bgc {
                        body_bg.rect_filled(
                            Rect::from_min_size(pos2(x, y), vec2(char_w, row_h)),
                            0.0,
                            bgc,
                        );
                    }
                    let ch = match bytes[i] {
                        Some(value) => codec::display_byte(value, ebcdic, conceal),
                        None => ' ',
                    };
                    body_bg.text(pos2(x, y), Align2::LEFT_TOP, ch, font.clone(), fgc);
                }
            }
        }

        action
    }

    fn hit_test(&self, index: usize, pos: egui::Pos2, body: Rect, grid: Grid) -> Option<u64> {
        let Grid {
            rows,
            bpr,
            char_w,
            row_h,
            ..
        } = grid;
        if self.core.panes[index].file.is_none() {
            return None;
        }
        let origin = body.min + vec2(PANE_PAD, PANE_PAD * 0.5);
        if pos.y < origin.y || pos.x < origin.x {
            return None;
        }
        let row = ((pos.y - origin.y) / row_h).floor() as usize;
        if row >= rows {
            return None;
        }
        let byte = column_at(pos.x, origin.x, char_w, bpr)?;

        let top = self.core.panes[index].top;
        let offset = top.saturating_add((row as u64) * bpr as u64 + byte as u64);
        Some(offset)
    }

    fn byte_at(&mut self, index: usize, offset: u64) -> Option<u8> {
        self.core
            .panes
            .get_mut(index)?
            .file
            .as_mut()?
            .read_byte(offset)
            .ok()
            .flatten()
    }

    /// Capture the window once the layout has settled, write a PNG-free PPM
    /// image and close the application.  `sips` or any image viewer can convert
    /// the result; keeping the encoder out of the binary avoids a dependency.
    fn maybe_screenshot(&mut self, ctx: &egui::Context) {
        if self.screenshot_done.load(Ordering::Relaxed) {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let Some(request) = self.screenshot.as_mut() else {
            return;
        };
        if request.frames_left > 0 {
            request.frames_left -= 1;
            ctx.request_repaint();
            return;
        }
        let Some(request) = self.screenshot.take() else {
            return;
        };
        let path = request.path;
        let done = Arc::clone(&self.screenshot_done);
        let repaint = ctx.clone();
        ctx.request_screenshot(move |image| {
            if let Err(e) = write_ppm(&path, &image) {
                eprintln!("cable-gui: screenshot failed: {e}");
            }
            done.store(true, Ordering::Relaxed);
            // The callback runs after the frame; ask for one more so the flag
            // is seen and the window closes.
            repaint.request_repaint();
        });
        ctx.request_repaint();
    }

    // ------------------------------------------------------------------
    // Dialogs

    fn show_dialog(&mut self, ctx: &egui::Context) -> Option<Action> {
        let Some(mut dialog) = self.dialog.take() else {
            return None;
        };
        let mut keep = true;
        let mut action = None;

        match &mut dialog {
            Dialog::Search { text, focus } => {
                egui::Window::new("Search")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        let label = if self.core.options.hex_input {
                            "Hex search string (blanks ignored)"
                        } else if self.core.options.ebcdic {
                            "EBCDIC search string"
                        } else {
                            "ASCII search string"
                        };
                        ui.label(label);
                        let response = ui.add(
                            egui::TextEdit::singleline(text)
                                .desired_width(360.0)
                                .hint_text("search…"),
                        );
                        if *focus {
                            response.request_focus();
                            *focus = false;
                        }
                        let enter = ui.input(|i| i.key_pressed(Key::Enter));
                        let escape = ui.input(|i| i.key_pressed(Key::Escape));
                        ui.horizontal(|ui| {
                            if ui.button("Search").clicked() || enter {
                                action = Some(Action::SearchText(text.clone()));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() || escape {
                                keep = false;
                            }
                        });
                    });
            }
            Dialog::Offset { mode, text, focus } => {
                let title = match mode {
                    OffsetMode::Goto => "Go to position",
                    OffsetMode::Add => "Add to position",
                    OffsetMode::Sub => "Subtract from position",
                };
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label("Decimal, or hex with a leading H (e.g. H1F40)");
                        let response = ui.add(
                            egui::TextEdit::singleline(text)
                                .desired_width(280.0)
                                .hint_text("0"),
                        );
                        if *focus {
                            response.request_focus();
                            *focus = false;
                        }
                        let enter = ui.input(|i| i.key_pressed(Key::Enter));
                        let escape = ui.input(|i| i.key_pressed(Key::Escape));
                        ui.horizontal(|ui| {
                            if ui.button("OK").clicked() || enter {
                                action = Some(Action::OffsetText(*mode, text.clone()));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() || escape {
                                keep = false;
                            }
                        });
                    });
            }
            Dialog::ConfirmQuit => {
                egui::Window::new("Unsaved changes")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label("Changes have not been written. Quit without saving?");
                        let enter = ui.input(|i| i.key_pressed(Key::Enter));
                        let escape = ui.input(|i| i.key_pressed(Key::Escape));
                        ui.horizontal(|ui| {
                            if ui.button("Quit without saving").clicked() || enter {
                                action = Some(Action::CloseApp);
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() || escape {
                                keep = false;
                            }
                        });
                    });
            }
            Dialog::ConfirmOpen { pane, path } => {
                let name = self.core.panes[*pane].name();
                egui::Window::new("Unsaved changes")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "Changes to {name} have not been written. Replace the file?"
                        ));
                        let enter = ui.input(|i| i.key_pressed(Key::Enter));
                        let escape = ui.input(|i| i.key_pressed(Key::Escape));
                        ui.horizontal(|ui| {
                            if ui.button("Replace").clicked() || enter {
                                action = Some(Action::Load {
                                    pane: *pane,
                                    path: path.clone(),
                                });
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() || escape {
                                keep = false;
                            }
                        });
                    });
            }
            Dialog::Options => {
                egui::Window::new("Options")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.checkbox(&mut self.core.options.ebcdic, "EBCDIC (CP037)");
                        ui.checkbox(&mut self.core.options.respect_case, "Respect case");
                        ui.checkbox(&mut self.core.options.conceal, "Conceal unprintables");
                        ui.checkbox(&mut self.core.options.hex_input, "Hex search/edit input");
                        ui.checkbox(&mut self.core.options.forward, "Search/compare forward");
                        ui.checkbox(&mut self.core.options.move_both, "Scroll both windows");
                        ui.separator();
                        ui.label("Character column");
                        for cpu in [
                            None,
                            Some(Cpu::X86),
                            Some(Cpu::X64),
                            Some(Cpu::Arm),
                            Some(Cpu::Thumb),
                            Some(Cpu::Arm64),
                        ] {
                            let label = cpu.map(Cpu::label).unwrap_or("ASCII / EBCDIC");
                            ui.radio_value(&mut self.core.options.disasm, cpu, label);
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Close").clicked() {
                                keep = false;
                            }
                        });
                        if ui.input(|i| i.key_pressed(Key::Escape)) {
                            keep = false;
                        }
                    });
            }
            Dialog::Help => {
                egui::Window::new("CABLE help")
                    .collapsible(false)
                    .resizable(true)
                    .fixed_size([820.0, 600.0])
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(500.0)
                            .show(ui, |ui| {
                                for line in help::LINES {
                                    ui.monospace(*line);
                                }
                            });
                        ui.separator();
                        if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(Key::Escape))
                        {
                            keep = false;
                        }
                    });
            }
            Dialog::About => {
                egui::Window::new("About CABLE")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.heading(format!("CABLE {}", env!("CARGO_PKG_VERSION")));
                        ui.label("Compare And Browse with Limited Editing");
                        ui.label("A cross-platform reimplementation of CABLE V1.7X.");
                        ui.label("Original DOS program by David Mitchell.");
                        ui.separator();
                        ui.label("Rust + egui desktop front end.");
                        if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(Key::Escape))
                        {
                            keep = false;
                        }
                    });
            }
        }

        if keep {
            self.dialog = Some(dialog);
        }
        action
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl GuiApp {
    /// One frame of the whole window.  Kept out of the `eframe::App` impl so
    /// headless tests can drive it with a plain `egui::Context`.
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        self.handle_drops(&ctx);
        self.update_title(&ctx);

        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.core.any_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if !matches!(self.dialog, Some(Dialog::ConfirmQuit)) {
                    self.dialog = Some(Dialog::ConfirmQuit);
                }
            }
        }

        self.handle_keys(&ctx);

        if let Some(action) = egui::Panel::top("cable-menu")
            .show(ui, |ui| self.menu_bar(ui))
            .inner
        {
            self.apply(action, &ctx);
        }
        if let Some(action) = egui::Panel::top("cable-toolbar")
            .show(ui, |ui| self.toolbar(ui))
            .inner
        {
            self.apply(action, &ctx);
        }
        egui::Panel::bottom("cable-status").show(ui, |ui| self.status_bar(ui));
        if let Some(action) = egui::CentralPanel::default()
            .show(ui, |ui| self.panes(ui))
            .inner
        {
            self.apply(action, &ctx);
        }
        if let Some(action) = self.show_dialog(&ctx) {
            self.apply(action, &ctx);
        }

        self.maybe_screenshot(&ctx);
    }

    /// Show the active document in the window title.
    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match self.core.panes[self.core.active].file.as_ref() {
            Some(file) => {
                let name = file
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| file.path().display().to_string());
                format!("{name} — CABLE")
            }
            None => "CABLE".to_string(),
        };
        if title != self.window_title {
            self.window_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    /// Accept files dragged from the desktop: the first goes into the active
    /// pane, a second into the other one.
    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|file| {
                    let path = file.path();
                    if path.as_os_str().is_empty() {
                        None
                    } else {
                        Some(path.to_path_buf())
                    }
                })
                .collect()
        });
        for (n, path) in dropped.into_iter().enumerate() {
            if n >= 2 {
                break;
            }
            let pane = if n == 0 {
                self.core.active
            } else {
                1 - self.core.active
            };
            if self.core.panes[pane].dirty() {
                self.dialog = Some(Dialog::ConfirmOpen { pane, path });
            } else {
                self.core.open_in(pane, path);
            }
        }
    }
}

// ----------------------------------------------------------------------
// Helpers

fn consume(ctx: &egui::Context, modifiers: egui::Modifiers, key: Key) {
    ctx.input_mut(|i| i.consume_key(modifiers, key));
}

/// Drop keyboard focus and the focus move egui derived from this frame's raw
/// key events.  Without this, Tab focuses the first menu or toolbar widget and
/// the arrow keys then change it instead of moving the file cursor.
fn release_focus(ctx: &egui::Context) {
    ctx.memory_mut(|memory| {
        memory.move_focus(egui::FocusDirection::None);
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
}

fn tui_color(color: TuiColor) -> Color32 {
    match color {
        TuiColor::Reset => Color32::from_rgb(220, 220, 220),
        TuiColor::Black => Color32::BLACK,
        TuiColor::Red => Color32::from_rgb(170, 0, 0),
        TuiColor::Green => Color32::from_rgb(0, 170, 0),
        TuiColor::Yellow => Color32::from_rgb(170, 85, 0),
        TuiColor::Blue => Color32::from_rgb(0, 0, 170),
        TuiColor::Magenta => Color32::from_rgb(170, 0, 170),
        TuiColor::Cyan => Color32::from_rgb(0, 170, 170),
        TuiColor::Gray => Color32::from_rgb(170, 170, 170),
        TuiColor::DarkGray => Color32::from_rgb(85, 85, 85),
        TuiColor::LightRed => Color32::from_rgb(255, 85, 85),
        TuiColor::LightGreen => Color32::from_rgb(85, 255, 85),
        TuiColor::LightYellow => Color32::from_rgb(255, 255, 224),
        TuiColor::LightBlue => Color32::from_rgb(85, 85, 255),
        TuiColor::LightMagenta => Color32::from_rgb(255, 85, 255),
        TuiColor::LightCyan => Color32::from_rgb(85, 255, 255),
        TuiColor::White => Color32::WHITE,
        TuiColor::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        TuiColor::Indexed(i) => Color32::from_gray(i),
    }
}

fn map_key(key: Key, modifiers: egui::Modifiers) -> Option<crossterm::event::KeyEvent> {
    use crossterm::event::{KeyCode as C, KeyEvent, KeyModifiers as M};
    let shift = modifiers.shift;
    let code = match key {
        Key::ArrowLeft => C::Left,
        Key::ArrowRight => C::Right,
        Key::ArrowUp => C::Up,
        Key::ArrowDown => C::Down,
        Key::Home => C::Home,
        Key::End => C::End,
        Key::PageUp => C::PageUp,
        Key::PageDown => C::PageDown,
        Key::Enter => C::Enter,
        Key::Escape => C::Esc,
        Key::Backspace => C::Backspace,
        Key::Delete => C::Delete,
        Key::Tab if shift => C::BackTab,
        Key::Tab => C::Tab,
        Key::Space => C::Char(' '),
        Key::ScrollLock => C::ScrollLock,
        Key::F1 => C::F(1),
        Key::F2 => C::F(2),
        Key::F3 => C::F(3),
        Key::F4 => C::F(4),
        Key::F5 => C::F(5),
        Key::F6 => C::F(6),
        Key::F7 => C::F(7),
        Key::F8 => C::F(8),
        Key::F9 => C::F(9),
        Key::F10 => C::F(10),
        Key::A => C::Char(letter('a', shift)),
        Key::B => C::Char(letter('b', shift)),
        Key::C => C::Char(letter('c', shift)),
        Key::D => C::Char(letter('d', shift)),
        Key::E => C::Char(letter('e', shift)),
        Key::F => C::Char(letter('f', shift)),
        Key::G => C::Char(letter('g', shift)),
        Key::H => C::Char(letter('h', shift)),
        Key::I => C::Char(letter('i', shift)),
        Key::J => C::Char(letter('j', shift)),
        Key::K => C::Char(letter('k', shift)),
        Key::L => C::Char(letter('l', shift)),
        Key::M => C::Char(letter('m', shift)),
        Key::N => C::Char(letter('n', shift)),
        Key::O => C::Char(letter('o', shift)),
        Key::P => C::Char(letter('p', shift)),
        Key::Q => C::Char(letter('q', shift)),
        Key::R => C::Char(letter('r', shift)),
        Key::S => C::Char(letter('s', shift)),
        Key::T => C::Char(letter('t', shift)),
        Key::U => C::Char(letter('u', shift)),
        Key::V => C::Char(letter('v', shift)),
        Key::W => C::Char(letter('w', shift)),
        Key::X => C::Char(letter('x', shift)),
        Key::Y => C::Char(letter('y', shift)),
        Key::Z => C::Char(letter('z', shift)),
        Key::Num0 => C::Char('0'),
        Key::Num1 => C::Char('1'),
        Key::Num2 => C::Char('2'),
        Key::Num3 => C::Char('3'),
        Key::Num4 => C::Char('4'),
        Key::Num5 => C::Char('5'),
        Key::Num6 => C::Char('6'),
        Key::Num7 => C::Char('7'),
        Key::Num8 => C::Char('8'),
        Key::Num9 => C::Char('9'),
        Key::Plus | Key::Equals => C::Char('+'),
        Key::Minus => C::Char('-'),
        Key::Period => C::Char('.'),
        Key::Comma => C::Char(','),
        Key::Slash => C::Char('/'),
        Key::Semicolon => C::Char(';'),
        Key::Quote => C::Char('\''),
        Key::Backslash => C::Char('\\'),
        Key::OpenBracket => C::Char('['),
        Key::CloseBracket => C::Char(']'),
        Key::Backtick => C::Char('`'),
        _ => return None,
    };
    let mut mods = M::empty();
    if modifiers.ctrl || modifiers.command {
        mods |= M::CONTROL;
    }
    if modifiers.shift {
        mods |= M::SHIFT;
    }
    if modifiers.alt {
        mods |= M::ALT;
    }
    Some(KeyEvent::new(code, mods))
}

fn letter(base: char, shift: bool) -> char {
    if shift {
        base.to_ascii_uppercase()
    } else {
        base
    }
}

/// Write an `egui` colour image as a binary PPM (P6), which every image tool
/// and the macOS `sips` command can read without extra dependencies.
fn write_ppm(path: &std::path::Path, image: &egui::ColorImage) -> std::io::Result<()> {
    let [width, height] = image.size;
    let mut out = Vec::with_capacity(32 + width * height * 3);
    out.extend_from_slice(format!("P6\n{width} {height}\n255\n").as_bytes());
    for pixel in &image.pixels {
        out.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b()]);
    }
    std::fs::write(path, out)
}

/// Largest multiple-of-16 byte count whose hex and character columns fit in
/// `max_chars` terminal cells.  The layout is: eight address digits, a space,
/// the hex groups separated by one space, two spaces, then the character
/// groups separated the same way.
fn bytes_per_row_for(max_chars: usize) -> usize {
    let mut best = 16;
    for candidate in [16usize, 32, 48, 64] {
        let groups = candidate / 16;
        let hex = 3 * candidate + groups - 1;
        let ascii = candidate + groups - 1;
        if 9 + hex + 2 + ascii <= max_chars {
            best = candidate;
        }
    }
    best
}

/// Title strip of a window.  The first file's title sits above its bytes and
/// the second file's below, so the two titles bracket the display exactly as
/// they do in the terminal front end.
fn pane_header_rect(rect: Rect, header_h: f32, index: usize) -> Rect {
    let y = if index == 1 {
        rect.max.y - header_h
    } else {
        rect.min.y
    };
    Rect::from_min_size(pos2(rect.min.x, y), vec2(rect.width(), header_h))
}

/// The byte grid of a window, leaving room for its title strip.
fn pane_body_rect(rect: Rect, header_h: f32, index: usize) -> Rect {
    if index == 1 {
        Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - header_h))
    } else {
        Rect::from_min_max(pos2(rect.min.x, rect.min.y + header_h), rect.max)
    }
}

/// Byte column under a pointer `x` inside a rendered row, or `None` when the
/// pointer is left of the hex area.  Both the hex and character regions group
/// bytes in sixteens separated by a single blank cell.
fn column_at(x: f32, origin_x: f32, char_w: f32, bpr: usize) -> Option<usize> {
    if x < origin_x || char_w <= 0.0 || bpr == 0 {
        return None;
    }
    let cell = ((x - origin_x) / char_w).floor() as usize;
    let hex_start = 9;
    let hex_width = 3 * bpr + bpr / 16 - 1;
    let ascii_start = hex_start + hex_width + 2;

    let (start, stride, cell_width) = if cell >= ascii_start {
        (ascii_start, 17, 1)
    } else if cell >= hex_start {
        (hex_start, 49, 3)
    } else {
        return None;
    };
    let within = cell - start;
    let group = within / stride;
    let column = (within % stride) / cell_width;
    Some((group * 16 + column).min(bpr - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lower_pane_title_sits_at_its_bottom() {
        let rect = egui::Rect::from_min_size(pos2(0.0, 100.0), vec2(400.0, 200.0));
        let header_h = 20.0;

        let top = pane_header_rect(rect, header_h, 0);
        assert_eq!(top.min.y, rect.min.y);
        assert_eq!(
            pane_body_rect(rect, header_h, 0).min.y,
            rect.min.y + header_h
        );

        let bottom = pane_header_rect(rect, header_h, 1);
        assert_eq!(bottom.max.y, rect.max.y);
        assert_eq!(bottom.min.y, rect.max.y - header_h);
        assert_eq!(pane_body_rect(rect, header_h, 1).max.y, bottom.min.y);
    }

    #[test]
    fn row_widths_grow_in_sixteen_byte_steps() {
        assert_eq!(bytes_per_row_for(80), 16);
        assert_eq!(bytes_per_row_for(140), 16);
        assert_eq!(bytes_per_row_for(141), 32);
        assert_eq!(bytes_per_row_for(10), 16);
        assert_eq!(bytes_per_row_for(400), 64);
    }

    #[test]
    fn hex_columns_map_to_the_expected_bytes() {
        let bpr = 32;
        let origin = 10.0;
        let char_w = 8.0;
        // Nine cells of address plus its trailing blank, then the first byte.
        let x = origin + 9.0 * char_w + 1.0;
        assert_eq!(column_at(x, origin, char_w, bpr), Some(0));
        // Each byte occupies three cells: two digits and a space.
        let x = origin + 12.0 * char_w + 1.0;
        assert_eq!(column_at(x, origin, char_w, bpr), Some(1));
    }

    #[test]
    fn character_columns_map_to_the_expected_bytes() {
        let bpr = 32;
        let origin = 10.0;
        let char_w = 8.0;
        let ascii_start = 9 + (3 * bpr + bpr / 16 - 1) + 2;
        // Byte 0 starts right after the two-space gap before the text column.
        let x = origin + (ascii_start as f32) * char_w + 1.0;
        assert_eq!(column_at(x, origin, char_w, bpr), Some(0));
        // The sixteenth character follows a one-cell group separator.
        let x = origin + (ascii_start + 17) as f32 * char_w + 1.0;
        assert_eq!(column_at(x, origin, char_w, bpr), Some(16));
    }

    #[test]
    fn address_column_has_no_byte() {
        assert_eq!(column_at(0.0, 10.0, 8.0, 16), None);
        assert_eq!(column_at(10.0 + 8.0, 10.0, 8.0, 16), None);
    }

    fn sample_app() -> (GuiApp, egui::Context) {
        // A directory per call: tests run in parallel and would otherwise
        // rewrite each other's files while a frame is being drawn.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cable-gui-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.bin");
        let second = dir.join("second.bin");
        std::fs::write(&first, (0..=255u8).collect::<Vec<_>>()).unwrap();
        std::fs::write(&second, vec![0u8; 4096]).unwrap();
        let ctx = egui::Context::default();
        let app = GuiApp::new_inner(&ctx, &[first, second], Options::default(), true).unwrap();
        (app, ctx)
    }

    #[test]
    fn a_frame_renders_without_panicking() {
        let (mut app, ctx) = sample_app();
        for _ in 0..3 {
            let mut output = ctx.run_ui(Default::default(), |ui| app.draw(ui));
            // Discard the texture uploads; nothing is painting in a headless test.
            output.textures_delta.clear();
        }
        assert!(app.core.panes[0].file.is_some());
        assert!(app.core.panes[1].file.is_some());
    }

    fn key_event(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn run_frame(app: &mut GuiApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.draw(ui));
        output.textures_delta.clear();
    }

    #[test]
    fn tab_and_arrows_do_not_reach_the_chrome() {
        let (mut app, ctx) = sample_app();
        let theme = app.core.options.theme;

        run_frame(&mut app, &ctx, vec![key_event(Key::Tab)]);
        assert_eq!(app.core.active, 1, "Tab should switch to the second window");
        assert!(
            ctx.memory(|m| m.focused().is_none()),
            "Tab left keyboard focus on a menu or toolbar widget"
        );

        let before = app.core.panes[1].cursor;
        run_frame(&mut app, &ctx, vec![key_event(Key::ArrowDown)]);
        assert!(
            app.core.panes[1].cursor > before,
            "the arrow key should move the second window's cursor"
        );
        assert_eq!(
            app.core.options.theme, theme,
            "the arrow key must not change a toolbar selection"
        );
    }

    #[test]
    fn preview_centres_the_cursor_byte() {
        let (mut app, _ctx) = sample_app();
        app.core.panes[0].cursor = 40;
        let conceal = app.core.options.conceal;
        let cells = app.preview_cells(0);
        assert_eq!(cells.len(), PREVIEW_LEN);
        assert_eq!(cells[0], codec::display_byte(24, false, conceal));
        assert_eq!(
            cells[PREVIEW_BEFORE],
            codec::display_byte(40, false, conceal)
        );
        assert_eq!(
            cells[PREVIEW_LEN - 1],
            codec::display_byte(87, false, conceal)
        );
    }

    #[test]
    fn preview_pads_before_the_start_of_the_file() {
        let (mut app, _ctx) = sample_app();
        let conceal = app.core.options.conceal;
        let cells = app.preview_cells(0);
        assert!(cells[..PREVIEW_BEFORE].iter().all(|c| *c == ' '));
        assert_eq!(
            cells[PREVIEW_BEFORE],
            codec::display_byte(0, false, conceal)
        );
    }

    #[test]
    fn preview_ends_with_blanks_past_the_file() {
        let (mut app, _ctx) = sample_app();
        let conceal = app.core.options.conceal;
        let last = app.core.panes[0].len() - 1;
        app.core.panes[0].cursor = last;
        let cells = app.preview_cells(0);
        assert_eq!(
            cells[PREVIEW_BEFORE],
            codec::display_byte(255, false, conceal)
        );
        assert_eq!(cells[PREVIEW_BEFORE + 1], ' ');
    }

    fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> String {
        fn walk(shape: &egui::Shape, out: &mut String) {
            match shape {
                egui::Shape::Text(text) => {
                    out.push_str(text.galley.text());
                    out.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = String::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn status_bar_shows_parameters_beside_a_message() {
        let (mut app, ctx) = sample_app();
        app.core.message = Some("Theme: Modern light".into());
        let mut output = ctx.run_ui(Default::default(), |ui| app.draw(ui));
        output.textures_delta.clear();
        let text = drawn_text(&output.shapes);
        assert!(
            text.contains(&app.core.parameter_line()),
            "the parameter summary vanished: {text:?}"
        );
        assert!(
            text.contains("Theme: Modern light"),
            "the message vanished: {text:?}"
        );
        assert!(text.contains(FKEY_HINT), "the key hint vanished: {text:?}");
    }

    #[test]
    fn theme_actions_update_the_palette() {
        let (mut app, ctx) = sample_app();
        app.apply(Action::Theme(Theme::ModernLight), &ctx);
        assert_eq!(app.core.options.theme, Theme::ModernLight);
        assert_eq!(app.core.status_line(), "Theme: Modern light");
    }

    #[test]
    fn every_dialog_draws_without_panicking() {
        let (mut app, ctx) = sample_app();
        let dialogs = [
            Action::Search,
            Action::Offset(OffsetMode::Goto),
            Action::Options,
            Action::Help,
            Action::About,
            Action::Quit,
        ];
        for action in dialogs {
            let label = format!("{action:?}");
            let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                app.apply(action, &ctx);
                let mut output = ctx.run_ui(Default::default(), |ui| app.draw(ui));
                output.textures_delta.clear();
            }));
            assert!(opened.is_ok(), "dialog {label} panicked");
            app.dialog = None;
        }
    }

    #[test]
    fn dialogs_open_and_close() {
        let (mut app, ctx) = sample_app();
        app.apply(Action::Search, &ctx);
        assert!(matches!(app.dialog, Some(Dialog::Search { .. })));
        app.apply(Action::SearchText("CABLE".to_string()), &ctx);
        assert!(app.dialog.is_none());
        app.apply(Action::Options, &ctx);
        assert!(matches!(app.dialog, Some(Dialog::Options)));
    }
}
