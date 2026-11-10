//! Terminal rendering.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, InputAction, MENU, Modal, Theme};
use crate::codec;
use crate::dir::{format_size, format_time};
use crate::help;

const FOOTER_ROWS: u16 = 4;
/// Rows reserved between the two panes for the position boxes and the two
/// single-line previews, matching the original's middle section.
const MIDDLE_ROWS: u16 = 6;
/// Width of the preview line built by `CS:0x3233` in `CS:0x171F`.
const PREVIEW_LEN: usize = 64;

/// Fixed geometry of the original 80x25 screen: a title row for each window,
/// eight data rows per window, the six-row position band between them, the
/// two-row bottom title/banner area, and a three-row command footer.
const ORIGINAL_COLS: u16 = 80;
const ORIGINAL_ROWS: u16 = 25;
const PANE_ROWS: usize = 8;
/// The cursor is always the sixth byte of the fifth data row.  The original's
/// crosshair is part of the static screen art, so the window scrolls until the
/// cursor sits in this cell.
const CURSOR_ROW: usize = 4;
const CURSOR_SLOT: usize = 5;
const VIEW_AHEAD: i64 = (CURSOR_ROW as i64) * 16 + CURSOR_SLOT as i64;
/// Column of the first hex byte and of the first ASCII character.
const DATA_COL: u16 = 15;
const ASCII_COL: u16 = 64;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = app.options.theme;
    let base = Block::default().style(Style::default().bg(theme.bg()).fg(theme.fg()));
    frame.render_widget(base, area);

    // The original is an 80x25 text screen.  On smaller terminals fall back to
    // the compact adaptive layout so the panes stay usable.
    if area.width >= ORIGINAL_COLS && area.height >= ORIGINAL_ROWS {
        draw_original(frame, app, area);
    } else {
        draw_compact(frame, app, area);
    }

    if app.modal.is_some() {
        let options = app.options.clone();
        if let Some(modal) = app.modal.as_mut() {
            draw_modal(frame, modal, &options, theme, area);
        }
    }
}

/// Draw the screen exactly as CABLE does on an 80x25 text mode display.
fn draw_original(frame: &mut Frame, app: &mut App, area: Rect) {
    app.rows_per_pane = PANE_ROWS;
    let x = area.x;
    let y = area.y;

    draw_original_title(frame, app, 0, x, y);
    draw_original_title(frame, app, 1, x, y + 21);
    draw_original_body(frame, app, 0, x, y + 1);
    draw_original_body(frame, app, 1, x, y + 13);
    draw_original_strip(frame, app, x, y);
    draw_original_band(frame, app, x, y + 8);
    draw_original_footer(frame, app, x, y + 22);
}

/// The selection bar the original uses for the active window's title, position
/// values, preview line and cursor byte (`attribute 0x78` in colour mode:
/// dark grey on light grey).
fn bar_style(theme: Theme) -> Style {
    Style::default().fg(theme.bar_fg()).bg(theme.bar_bg())
}

/// Title row: file name (56), decimal size (17), size in hex and DOS file
/// attributes.  Both windows show the archive attribute `A` for a normal file.
fn draw_original_title(frame: &mut Frame, app: &App, index: usize, x: u16, y: u16) {
    let theme = app.options.theme;
    let active = app.active == index;
    let style = if active {
        bar_style(theme)
    } else {
        Style::default().fg(theme.fg()).bg(theme.bg())
    };
    let pane = &app.panes[index];
    let name = pane
        .file
        .as_ref()
        .and_then(|f| f.path().file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| pane.name());
    let len = pane.len();
    let read_only = pane
        .file
        .as_ref()
        .map(|f| f.is_read_only())
        .unwrap_or(false);
    let attrs = if read_only { "R" } else { "A" };
    let mut name: String = name.chars().take(56).collect();
    if pane.dirty() {
        name.insert(0, '*');
        name.truncate(56);
    }
    // The active attribute is applied per slot, not to the whole row: slots 0,
    // 1 and 2 occupy columns 0-55, 57-73 and 75-79, leaving columns 56 and 74
    // in the normal colours (CS:0x1867 entries 0-2, `FUN_1000_3348`).
    put_line(frame, x, y, &format!("{name:<56}"), style);
    put_line(frame, x + 57, y, &format!("{len:>8} {len:08X}"), style);
    put_line(frame, x + 75, y, &format!("{attrs:>4} "), style);
}

/// Left part of the screen: the function-key option legend above the signed-on
/// banner.  Both are static art in the original; only the values change.
fn draw_original_strip(frame: &mut Frame, app: &App, x: u16, y: u16) {
    let theme = app.options.theme;
    let style = Style::default().fg(theme.fg()).bg(theme.bg());
    let entries = [
        ("F2:", app.options.case(), 7usize),
        ("F3:", app.options.direction(), 8),
        ("F4:", app.options.input(), 9),
        ("F5:", app.options.show(), 7),
        ("F6:", app.options.charset(), 6),
        ("SL:", app.options.scroll_lock(), 6),
    ];
    for (i, (label, value, width)) in entries.iter().enumerate() {
        let text = format!("{label}{value:<width$}");
        put_line(frame, x, y + 2 + i as u16, &text, style);
    }
    let banner = [
        "CABLE V1.7X",
        "IBM Internal",
        "Use Only",
        "",
        "Dave Mitchell",
        "ZENO @ WINVMB",
    ];
    for (i, text) in banner.iter().enumerate() {
        put_line(frame, x, y + 14 + i as u16, text, style);
    }
}

/// Eight sixteen-byte rows, starting sixty-nine bytes before the cursor.  The
/// cursor therefore always lands on row five, byte six, where the static
/// crosshair is drawn.
fn draw_original_body(frame: &mut Frame, app: &mut App, index: usize, x: u16, y: u16) {
    let theme = app.options.theme;
    let active = app.active == index;
    let compare = app.compare_mode;
    let ebcdic = app.options.ebcdic;
    let conceal = app.options.conceal;
    let cursor = app.panes[index].cursor;
    let start = cursor as i64 - VIEW_AHEAD;
    let normal = Style::default().fg(theme.fg()).bg(theme.bg());
    let dim = Style::default().fg(theme.dim()).bg(theme.bg());
    let diff = Style::default().fg(theme.diff_fg()).bg(theme.diff_bg());

    if app.panes[index].file.is_none() {
        put_line(frame, x, y, "--- empty ---", dim);
        return;
    }

    for row in 0..PANE_ROWS {
        let row_start = start + (row as i64) * 16;
        let line_y = y + row as u16;
        for i in 0..16usize {
            let off = row_start + i as i64;
            let blank = off < 0 || off as u64 >= app.panes[index].len();
            let b = if blank {
                None
            } else {
                byte_at(app, index, off as u64)
            };
            let other = if compare && !blank {
                byte_at(app, 1 - index, off as u64)
            } else {
                None
            };
            let bx = x + DATA_COL + (i as u16) * 3;
            match b {
                Some(v) => {
                    let style = if compare && Some(v) != other {
                        diff
                    } else {
                        normal
                    };
                    put_cell(frame, bx, line_y, &format!("{:02X}", v), style);
                    let ch = codec::display_byte(v, ebcdic, conceal);
                    put_cell(
                        frame,
                        x + ASCII_COL + i as u16,
                        line_y,
                        &ch.to_string(),
                        style,
                    );
                }
                None => {
                    put_cell(frame, bx, line_y, "**", normal);
                    put_cell(frame, x + ASCII_COL + i as u16, line_y, " ", normal);
                }
            }
        }

        // The crosshair is fixed at screen columns 29 and 32 (row-string
        // offsets 14 and 17, a row is drawn from column 15).
        let (left, right) = crosshair_chars(index, active, row);
        if let (Some(l), Some(r)) = (left, right) {
            put_cell(frame, x + DATA_COL + 14, line_y, l, normal);
            put_cell(frame, x + DATA_COL + 17, line_y, r, normal);
        }

        // Slot 16 (`[2, row, 30]`) redraws only the attribute of the two hex
        // digits already written for the cursor byte; the ASCII cell and the
        // crosshair glyphs keep the normal colours.
        if active && row == CURSOR_ROW {
            let cx = x + DATA_COL + (CURSOR_SLOT as u16) * 3;
            restyle_cells(frame, cx, line_y, cx + 2, bar_style(theme));
        }
    }
}

/// Crosshair glyphs for one row of one window.  The top window's corner opens
/// downwards (`╔ ╗`) and the guides continue below it; the bottom window's
/// guides run down to its corner (`╚ ╝`).
fn crosshair_chars(
    index: usize,
    _active: bool,
    row: usize,
) -> (Option<&'static str>, Option<&'static str>) {
    if index == 0 {
        match row {
            CURSOR_ROW => (Some("╔"), Some("╗")),
            r if r > CURSOR_ROW => (Some("║"), Some("║")),
            _ => (None, None),
        }
    } else {
        match row {
            CURSOR_ROW => (Some("╚"), Some("╝")),
            r if r < CURSOR_ROW => (Some("║"), Some("║")),
            _ => (None, None),
        }
    }
}

/// The six-row band: two overlapping `Hex`/`Dec` boxes plus one preview line
/// per window.  This is static art in the original (`CS:0x6B1`), including the
/// `╩═╦╝` / `╦═╩╗` crosshair tails, which only line up because the cursor is
/// always at the same cell.
fn draw_original_band(frame: &mut Frame, app: &mut App, x: u16, y: u16) {
    let theme = app.options.theme;
    let normal = Style::default().fg(theme.fg()).bg(theme.bg());
    let bar = bar_style(theme);
    let hex0 = hex_value(app.panes[0].cursor);
    let dec0 = dec_value(app.panes[0].cursor);
    let hex1 = hex_value(app.panes[1].cursor);
    let dec1 = dec_value(app.panes[1].cursor);
    let lines = [
        "╔════════════╗".to_string(),
        format!("║{hex0}╠═══════════════╩═╦╝"),
        format!("╚{dec0}║"),
        format!("╔{dec1}║"),
        format!("║{hex1}╠═══════════════╦═╩╗"),
        "╚════════════╝".to_string(),
    ];
    for (i, line) in lines.iter().enumerate() {
        put_line(frame, x, y + i as u16, line, normal);
    }

    // The active window's Hex and Dec values and its preview line are drawn in
    // the selection colours; the box border and the one-column gap keep the
    // normal colours.
    let (hex_row, dec_row) = if app.active == 0 { (1, 2) } else { (4, 3) };
    for row in [hex_row, dec_row] {
        restyle_cells(frame, x + 5, y + row, x + 5 + 8, bar);
    }

    for index in 0..2 {
        let row = if index == 0 { 2 } else { 3 };
        let active = app.active == index;
        let style = if active { bar } else { normal };
        if active {
            // The allocation is exactly the 64 preview cells (columns 15-78);
            // the last screen column keeps the normal attribute.
            restyle_cells(
                frame,
                x + DATA_COL,
                y + row,
                x + DATA_COL + PREVIEW_LEN as u16,
                bar,
            );
        }
        let text = preview_text(app, index);
        put_line(frame, x + DATA_COL, y + row, &text, style);
    }
}

fn hex_value(offset: u64) -> String {
    format!("Hex {:08X}", offset as u32)
}

fn dec_value(offset: u64) -> String {
    let digits = offset.to_string();
    let shown = if digits.len() > 8 {
        &digits[digits.len() - 8..]
    } else {
        &digits[..]
    };
    format!("Dec {shown:>8}")
}

/// Menu, message line and a blank bottom row, matching rows 22-24 of the
/// original screen.
fn draw_original_footer(frame: &mut Frame, app: &App, x: u16, y: u16) {
    let theme = app.options.theme;
    draw_menu_line(frame, app, x, y);
    let (text, style) = if let Some(msg) = &app.message {
        (
            msg.clone(),
            Style::default()
                .fg(theme.header_fg())
                .bg(theme.bg())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            app.menu_description().to_string(),
            Style::default().fg(theme.fg()).bg(theme.bg()),
        )
    };
    put_line(frame, x, y + 1, &text, style);
}

/// Adaptive layout used on terminals smaller than the original 80x25 screen.
fn draw_compact(frame: &mut Frame, app: &mut App, area: Rect) {
    let footer_h = FOOTER_ROWS.min(area.height);
    let body_h = area.height.saturating_sub(footer_h);
    // Keep the panes usable on short terminals; drop the middle band if
    // there is not enough room for both boxes and at least a few data rows.
    let middle_h = if body_h >= 18 { MIDDLE_ROWS } else { 0 };
    let panes_h = body_h.saturating_sub(middle_h);
    let top_h = panes_h / 2;
    let bottom_h = panes_h - top_h;
    let top = Rect::new(area.x, area.y, area.width, top_h);
    let middle = Rect::new(area.x, area.y + top_h, area.width, middle_h);
    let bottom = Rect::new(area.x, area.y + top_h + middle_h, area.width, bottom_h);
    let footer = Rect::new(area.x, area.y + body_h, area.width, footer_h);

    app.rows_per_pane = top_h.saturating_sub(1) as usize;
    draw_pane(frame, app, 0, top);
    draw_pane(frame, app, 1, bottom);
    if middle_h > 0 {
        draw_middle(frame, app, middle);
        draw_position_markers(frame, app, top, middle, bottom);
    }
    draw_footer(frame, app, footer);
}

fn draw_pane(frame: &mut Frame, app: &mut App, index: usize, rect: Rect) {
    if rect.height == 0 || rect.width == 0 {
        return;
    }
    let theme = app.options.theme;
    let active = app.active == index;
    let cursor = app.panes[index].cursor;
    let top = app.panes[index].top;
    let name = app.panes[index].name();
    let len = app.panes[index].len();
    let dirty = app.panes[index].dirty();
    let read_only = app.panes[index]
        .file
        .as_ref()
        .map(|f| f.is_read_only())
        .unwrap_or(false);

    let header = Rect::new(rect.x, rect.y, rect.width, 1);
    // The original inverts the active window's title bar.
    let title_fg = if active { theme.bg() } else { theme.dim() };
    let title_bg = if active {
        theme.header_fg()
    } else {
        theme.bg()
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(title_bg).fg(title_fg)),
        header,
    );
    let header_chunks =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(38)]).split(header);
    let mut title_text = format!("{}  [{} bytes]", name, len);
    if dirty {
        title_text.push_str(" *");
    }
    if read_only {
        title_text.push_str(" (read-only)");
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title_text,
            Style::default()
                .fg(title_fg)
                .bg(title_bg)
                .add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))),
        header_chunks[0],
    );
    let flags = format!(
        "Hex {:08X}  Dec {}  {}{}",
        cursor,
        cursor,
        if active { "A" } else { " " },
        if app.compare_mode { " CMP" } else { "" }
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            flags,
            Style::default().fg(title_fg).bg(title_bg),
        )))
        .alignment(Alignment::Right),
        header_chunks[1],
    );

    let body = Rect::new(
        rect.x,
        rect.y + 1,
        rect.width,
        rect.height.saturating_sub(1),
    );
    if app.panes[index].file.is_none() {
        frame.render_widget(
            Paragraph::new("--- empty ---")
                .style(Style::default().fg(theme.dim()).bg(theme.bg()))
                .alignment(Alignment::Center),
            body,
        );
        return;
    }
    let rows = body.height as usize;
    if rows == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for row in 0..rows {
        let offset = top.saturating_add((row as u64) * 16);
        lines.push(hex_line(app, index, offset));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().fg(theme.fg()).bg(theme.bg())),
        body,
    );
}

fn hex_line(app: &mut App, index: usize, offset: u64) -> Line<'static> {
    let theme = app.options.theme;
    let active = app.active == index;
    let cursor = app.panes[index].cursor;
    let compare = app.compare_mode;
    let ebcdic = app.options.ebcdic;
    let conceal = app.options.conceal;

    let normal = Style::default().fg(theme.fg()).bg(theme.bg());
    let dim = Style::default().fg(theme.dim()).bg(theme.bg());
    let cursor_style = Style::default()
        .fg(theme.cursor_fg())
        .bg(theme.cursor_bg())
        .add_modifier(Modifier::BOLD);
    let diff_style = Style::default().fg(theme.diff_fg()).bg(theme.diff_bg());
    let offset_style = Style::default().fg(theme.accent()).bg(theme.bg());

    let mut spans = Vec::with_capacity(40);
    spans.push(Span::styled(format!("{offset:08X} "), offset_style));

    let mut bytes = Vec::with_capacity(16);
    let mut other = Vec::with_capacity(16);
    for i in 0..16 {
        bytes.push(byte_at(app, index, offset + i));
        other.push(if compare {
            byte_at(app, 1 - index, offset + i)
        } else {
            None
        });
    }

    let mut ascii = Vec::with_capacity(16);
    for i in 0..16 {
        let off = offset + i as u64;
        let b = bytes[i];
        let ob = other[i];
        let is_cursor = active && off == cursor;
        // In compare mode a byte that exists in one file but not the other is
        // a length difference; equal bytes are shown normally.  `**` is
        // reserved for positions past the end of *this* file, matching the
        // original renderer (CS:0x3ba9 writes `**` when CS:0x3884 reports no
        // byte).
        let differs = compare && b != ob;
        let style = if is_cursor && active {
            cursor_style
        } else if differs {
            diff_style
        } else {
            normal
        };

        match b {
            Some(v) => {
                spans.push(Span::styled(format!("{v:02X} "), style));
                let ch = codec::display_byte(v, ebcdic, conceal);
                ascii.push(Span::styled(ch.to_string(), style));
            }
            None => {
                spans.push(Span::styled("** ", dim));
                ascii.push(Span::styled(" ", dim));
            }
        }
    }

    spans.push(Span::raw(" "));
    spans.extend(ascii);
    Line::from(spans)
}

fn byte_at(app: &mut App, index: usize, offset: u64) -> Option<u8> {
    app.panes
        .get_mut(index)?
        .file
        .as_mut()?
        .read_byte(offset)
        .ok()
        .flatten()
}

/// Width of the two offset boxes in the middle band.  The original uses a
/// 12-character interior: `Hex ` plus eight hex digits, and `Dec ` plus the
/// decimal offset.
const BOX_W: u16 = 14;

/// The six box lines, in order: the top window's `Hex`/`Dec` pair, then the
/// bottom window's `Dec`/`Hex` pair.  Both windows' boxes share the same
/// screen rows; only the corner/guide characters differ.
///
/// ```text
/// ╔════════════╗
/// ║Hex 00000000╠═══════════════╩═╦╝
/// ╚Dec 0       ║
/// ╔Dec 0       ║
/// ║Hex 00000000╠═══════════════╦═╩╗
/// ╚════════════╝
/// ```
fn box_lines(app: &App) -> [String; 6] {
    let cw = (BOX_W - 2) as usize;
    let rule = "═".repeat(cw);
    [
        format!("╔{rule}╗"),
        format!("║{}╠", fit_hex("Hex ", row_start(app, 0), cw)),
        format!("╚{}║", fit_dec("Dec ", row_start(app, 0), cw)),
        format!("╔{}║", fit_dec("Dec ", row_start(app, 1), cw)),
        format!("║{}╠", fit_hex("Hex ", row_start(app, 1), cw)),
        format!("╚{rule}╝"),
    ]
}

/// First byte of the sixteen-byte row that contains this window's cursor.
fn row_start(app: &App, index: usize) -> u64 {
    app.panes[index].cursor & !0xF
}

/// Draw the original's middle section: two overlapping `Hex`/`Dec` position
/// boxes on the left.  The preview lines and crosshair are added afterwards,
/// because their starting column depends on the cursor's byte column.
fn draw_middle(frame: &mut Frame, app: &App, rect: Rect) {
    let theme = app.options.theme;
    let base = Style::default().fg(theme.fg()).bg(theme.bg());
    frame.render_widget(Block::default().style(base), rect);
    if rect.height < 6 || rect.width < BOX_W + 12 {
        return;
    }

    let active_style = Style::default().fg(theme.cursor_fg()).bg(theme.cursor_bg());

    let lines = box_lines(app);
    for (i, line) in lines.iter().enumerate() {
        put_line(
            frame,
            rect.x,
            rect.y + i as u16,
            line,
            Style::default().fg(theme.fg()).bg(theme.bg()),
        );
    }

    // The active window's value cells are reverse video; the labels and box
    // borders stay in the normal colour, as in the original.
    let value_row = if app.active == 0 { 1 } else { 4 };
    restyle_cells(
        frame,
        rect.x + 5,
        rect.y + value_row,
        rect.x + BOX_W - 1,
        active_style,
    );
}

/// Box-drawing crosshair that joins each position box to the cursor byte:
/// horizontal guides in the middle band and vertical double lines running
/// from the band to the byte.
fn draw_position_markers(frame: &mut Frame, app: &mut App, top: Rect, middle: Rect, bottom: Rect) {
    if middle.height < 4 || middle.width < BOX_W + 12 {
        return;
    }
    let theme = app.options.theme;
    let style = Style::default().fg(theme.fg()).bg(theme.bg());
    for (index, rect) in [(0usize, top), (1usize, bottom)] {
        let marker = marker_columns(app, index, rect);
        if let Some((left, right, row)) = marker {
            let body_y = rect.y + 1;
            let body_h = rect.height.saturating_sub(1);
            if index == 0 {
                // Frame opens downwards: current row to the bottom of the pane.
                put_cell(frame, left, body_y + row, "╓", style);
                put_cell(frame, right, body_y + row, "╖", style);
                for y in body_y + row + 1..body_y + body_h {
                    put_cell(frame, left, y, "║", style);
                    put_cell(frame, right, y, "║", style);
                }
                if left > middle.x + BOX_W {
                    put_cell(frame, left, middle.y, "║", style);
                    put_cell(frame, right, middle.y, "║", style);
                }
                draw_guide(frame, middle, 1, left, right, "╩", style);
            } else {
                for y in body_y..body_y + row {
                    put_cell(frame, left, y, "║", style);
                    put_cell(frame, right, y, "║", style);
                }
                put_cell(frame, left, body_y + row, "╙", style);
                put_cell(frame, right, body_y + row, "╜", style);
                if left > middle.x + BOX_W {
                    put_cell(frame, left, middle.y + middle.height - 1, "║", style);
                    put_cell(frame, right, middle.y + middle.height - 1, "║", style);
                }
                draw_guide(frame, middle, 4, left, right, "╦", style);
            }
        }
        // Previews sit on the `Dec` rows, below the top window's guide and
        // above the bottom window's guide.  Start them under the cursor byte
        // so the preview lines up with the highlighted position.
        draw_preview(frame, app, middle, index);
    }
}

/// One-line character preview of the sixty-four bytes around this window's
/// cursor.  The original builds the line in `CS:0x171F` from the four display
/// rows around the cursor (`CS:0x3233` → `CS:0x32BF`) and draws it with slot 4
/// at column 15, immediately right of the `Hex`/`Dec` box.  Bytes before the
/// start of the file or past its end leave blanks.  The active window's line is
/// drawn as a reverse-video bar starting after the `Dec ` label, matching the
/// original.
fn draw_preview(frame: &mut Frame, app: &mut App, middle: Rect, index: usize) {
    let row = if index == 0 { 2 } else { 3 };
    if middle.y + row >= middle.bottom() || middle.width <= BOX_W {
        return;
    }
    let theme = app.options.theme;
    let active = app.active == index;
    let normal = Style::default().fg(theme.fg()).bg(theme.bg());
    let bar = Style::default().fg(theme.cursor_fg()).bg(theme.cursor_bg());
    let y = middle.y + row;
    let preview_x = middle.x + BOX_W + 1;
    if active {
        // The original applies the active attribute to slot 3 (the value
        // cells, columns 5-12) and slot 4 (the preview, columns 15-78).  The
        // border and the one-column gap keep their normal colours.
        let bar_x = middle.x + 5;
        let value_end = middle.x + BOX_W - 1;
        for x in bar_x..value_end {
            put_cell(frame, x, y, " ", bar);
        }
        for x in preview_x..middle.right() {
            put_cell(frame, x, y, " ", bar);
        }
        // Restore the box glyphs, then tint the value/preview areas.
        let line = box_lines(app)[row as usize].clone();
        put_line(frame, middle.x, y, &line, normal);
        restyle_cells(frame, bar_x, y, value_end, bar);
        restyle_cells(frame, preview_x, y, middle.right(), bar);
    }
    put_line(
        frame,
        preview_x,
        y,
        &preview_text(app, index),
        if active { bar } else { normal },
    );
}

/// Re-apply a style to a row span without changing the glyphs already there.
fn restyle_cells(frame: &mut Frame, from: u16, y: u16, to: u16, style: Style) {
    for x in from..to {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
            cell.set_style(style);
        }
    }
}

fn draw_guide(
    frame: &mut Frame,
    middle: Rect,
    row: u16,
    left: u16,
    right: u16,
    joint: &str,
    style: Style,
) {
    let y = middle.y + row;
    if y >= middle.bottom() || right <= middle.x + BOX_W || left <= middle.x + BOX_W {
        return;
    }
    // Do not draw through the byte itself; the guide lives in the band, so
    // the full connector is safe, but keep it inside the band.
    for x in middle.x + BOX_W..right {
        put_cell(frame, x, y, "═", style);
    }
    put_cell(frame, left, y, joint, style);
    put_cell(frame, right, y, joint, style);
}

/// Screen column of the first and last cell of the cursor byte, plus its row
/// inside the pane body.  `None` when the cursor is outside the visible rows.
fn marker_columns(app: &App, index: usize, rect: Rect) -> Option<(u16, u16, u16)> {
    let pane = &app.panes[index];
    if pane.file.is_none() || rect.height < 2 {
        return None;
    }
    let rows = rect.height.saturating_sub(1) as u64;
    let top = pane.top;
    let cursor = pane.cursor;
    if cursor < top || cursor >= top.saturating_add(rows * 16) {
        return None;
    }
    let row = ((cursor - top) / 16) as u16;
    let col = (cursor % 16) as u16;
    // Address field is 8 hex digits + one space, then 3 columns per byte.
    let x = rect.x + 9 + col * 3;
    let left = x.checked_sub(1)?;
    let right = x + 2;
    if right >= rect.right() || left < rect.x {
        return None;
    }
    Some((left, right, row))
}

fn preview_text(app: &mut App, index: usize) -> String {
    let cursor = app.panes[index].cursor;
    // `CS:0x3233` builds a 64-character line in `CS:0x171F` by copying the
    // sixteeen display characters of the rows around the cursor.  The first
    // segment starts at character 5 of the row above the cursor (`CS:0x32BF`
    // with `AL=0` copies from `CS:0x185B`), so the preview begins sixteen bytes
    // before the cursor.  Bytes outside the file are blank.
    let start = cursor as i64 - 16;
    let ebcdic = app.options.ebcdic;
    let conceal = app.options.conceal;
    let mut text = String::with_capacity(PREVIEW_LEN);
    for i in 0..PREVIEW_LEN {
        let off = start + i as i64;
        if off < 0 {
            text.push(' ');
            continue;
        }
        match byte_at(app, index, off as u64) {
            Some(b) => text.push(codec::display_byte(b, ebcdic, conceal)),
            None => text.push(' '),
        }
    }
    text
}

fn fit_hex(prefix: &str, value: u64, width: usize) -> String {
    let digits = format!("{value:08X}");
    let avail = width.saturating_sub(prefix.chars().count());
    let shown = if digits.len() > avail {
        &digits[digits.len() - avail..]
    } else {
        &digits[..]
    };
    format!("{prefix}{shown:<avail$}")
}

fn fit_dec(prefix: &str, value: u64, width: usize) -> String {
    let digits = value.to_string();
    let avail = width.saturating_sub(prefix.chars().count());
    let shown = if digits.len() > avail {
        &digits[digits.len() - avail..]
    } else {
        &digits[..]
    };
    format!("{prefix}{shown:<avail$}")
}

fn put_cell(frame: &mut Frame, x: u16, y: u16, symbol: &str, style: Style) {
    if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
        cell.set_symbol(symbol);
        cell.set_style(style);
    }
}

fn put_line(frame: &mut Frame, x: u16, y: u16, text: &str, style: Style) {
    for (i, ch) in text.chars().enumerate() {
        put_cell(frame, x + i as u16, y, &ch.to_string(), style);
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.options.theme;
    let bg = Style::default().bg(theme.bg()).fg(theme.fg());
    frame.render_widget(Block::default().style(bg), area);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let status_style = if app.message.is_some() {
        Style::default()
            .fg(theme.header_fg())
            .bg(theme.bg())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.accent()).bg(theme.bg())
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(app.status_line(), status_style))),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            app.fkey_line(),
            Style::default().fg(theme.dim()).bg(theme.bg()),
        ))),
        rows[1],
    );

    draw_menu_line(frame, app, rows[2].x, rows[2].y);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            app.menu_description(),
            Style::default().fg(theme.fg()).bg(theme.bg()),
        ))),
        rows[3],
    );
}

/// `CABLE: Compare Dir Edit ...` with the currently selected command shown in
/// reverse video.  Shared by the original and compact footers.
fn draw_menu_line(frame: &mut Frame, app: &App, x: u16, y: u16) {
    let theme = app.options.theme;
    let mut menu = vec![Span::styled(
        "CABLE: ",
        Style::default().fg(theme.fg()).bg(theme.bg()),
    )];
    for (i, (label, _)) in MENU.iter().enumerate() {
        if i > 0 {
            menu.push(Span::raw(" "));
        }
        let style = if i == app.menu_index {
            Style::default().fg(theme.bar_fg()).bg(theme.bar_bg())
        } else {
            Style::default().fg(theme.fg()).bg(theme.bg())
        };
        menu.push(Span::styled(*label, style));
    }
    let rect = Rect::new(x, y, ORIGINAL_COLS, 1);
    frame.render_widget(Paragraph::new(Line::from(menu)), rect);
}

fn draw_modal(
    frame: &mut Frame,
    modal: &mut Modal,
    options: &crate::app::Options,
    theme: Theme,
    area: Rect,
) {
    match modal {
        Modal::Confirm { title, text, .. } => {
            let rect = centered(area, 76, 7);
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(title.as_str())
                .style(Style::default().fg(theme.fg()).bg(theme.bg()))
                .border_style(Style::default().fg(theme.border()));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    text.clone(),
                    Style::default()
                        .fg(theme.header_fg())
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Y = yes    N = no    Esc = cancel",
                    Style::default().fg(theme.dim()),
                )),
            ];
            frame.render_widget(
                Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
                inner,
            );
        }
        Modal::Input {
            title,
            prompt,
            value,
            action,
        } => {
            let rect = centered(area, 78, 8);
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(title.as_str())
                .style(Style::default().fg(theme.fg()).bg(theme.bg()))
                .border_style(Style::default().fg(theme.border()));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let hint = match action {
                InputAction::Add | InputAction::Sub | InputAction::Goto => {
                    "Decimal, or HEX with a leading H or 0x"
                }
                InputAction::Search => "Enter = search    Esc = cancel",
                InputAction::View => "Enter = open    Esc = cancel",
            };
            let lines = vec![
                Line::from(Span::styled(
                    prompt.clone(),
                    Style::default().fg(theme.accent()),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme.accent())),
                    Span::styled(
                        value.clone(),
                        Style::default().fg(theme.cursor_fg()).bg(theme.cursor_bg()),
                    ),
                    Span::styled("_", Style::default().fg(theme.fg())),
                ]),
                Line::from(""),
                Line::from(Span::styled(hint, Style::default().fg(theme.dim()))),
            ];
            frame.render_widget(
                Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
                inner,
            );
        }
        Modal::Options { index } => {
            let rect = centered(area, 64, 15);
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .title("OPTIONS")
                .style(Style::default().fg(theme.fg()).bg(theme.bg()))
                .border_style(Style::default().fg(theme.border()));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let lines = vec![
                option_line("ASCII", *index == 0, !options.ebcdic, theme),
                option_line("Back", *index == 1, !options.forward, theme),
                option_line("Conceal", *index == 2, options.conceal, theme),
                option_line("EBCDIC", *index == 3, options.ebcdic, theme),
                option_line("Fore", *index == 4, options.forward, theme),
                option_line("Help", *index == 5, false, theme),
                option_line("Ignore", *index == 6, !options.respect_case, theme),
                option_line("Quit", *index == 7, false, theme),
                option_line("Respect", *index == 8, options.respect_case, theme),
                option_line("Show", *index == 9, !options.conceal, theme),
                option_line("heX/char", *index == 10, options.hex_input, theme),
                Line::from(Span::styled(
                    "Enter/Space = select   Esc = close",
                    Style::default().fg(theme.dim()),
                )),
            ];
            frame.render_widget(Paragraph::new(Text::from(lines)), inner);
        }
        Modal::Modes { index } => {
            let rect = centered(area, 44, 9);
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .title("MODE")
                .style(Style::default().fg(theme.fg()).bg(theme.bg()))
                .border_style(Style::default().fg(theme.border()));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let labels = ["Attributes", "Color", "Mono", "Help", "Quit"];
            let lines: Vec<Line> = labels
                .iter()
                .enumerate()
                .map(|(i, label)| selectable(label, i == *index, theme))
                .collect();
            frame.render_widget(Paragraph::new(Text::from(lines)), inner);
        }
        Modal::Attributes { index } => {
            let rect = centered(area, 46, 8);
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .title("ATTRIBUTES")
                .style(Style::default().fg(theme.fg()).bg(theme.bg()))
                .border_style(Style::default().fg(theme.border()));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let labels = [Theme::Classic, Theme::Amber, Theme::Mono];
            let mut lines: Vec<Line> = labels
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let text = if *t == theme {
                        format!("{}  (current)", t.name())
                    } else {
                        t.name().to_string()
                    };
                    selectable(&text, i == *index, theme)
                })
                .collect();
            lines.push(Line::from(Span::styled(
                "Enter = choose   Esc = close",
                Style::default().fg(theme.dim()),
            )));
            frame.render_widget(Paragraph::new(Text::from(lines)), inner);
        }
        Modal::Help { scroll } => {
            let rect = centered(area, 88, area.height.saturating_sub(2));
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(help::TITLE)
                .style(Style::default().fg(theme.fg()).bg(theme.bg()))
                .border_style(Style::default().fg(theme.border()));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let visible = inner.height as usize;
            let max = help::LINES.len().saturating_sub(1);
            let start = (*scroll).min(max);
            let end = (start + visible).min(help::LINES.len());
            let lines: Vec<Line> = help::LINES[start..end]
                .iter()
                .map(|s| Line::from(Span::styled(*s, Style::default().fg(theme.fg()))))
                .collect();
            frame.render_widget(Paragraph::new(Text::from(lines)), inner);
            let footer = format!(
                "Line {} of {}   Up/Down, PgUp/PgDn, Home/End scroll   Esc close",
                start + 1,
                help::LINES.len()
            );
            let footer_rect = Rect::new(
                inner.x,
                inner.y + inner.height.saturating_sub(1),
                inner.width,
                1,
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    footer,
                    Style::default().fg(theme.dim()),
                ))),
                footer_rect,
            );
        }
        Modal::Dir(b) => draw_dir(frame, b, theme, area),
    }
}

fn option_line(label: &str, selected: bool, current: bool, theme: Theme) -> Line<'static> {
    let style = if selected {
        Style::default()
            .fg(theme.bg())
            .bg(theme.cursor_bg())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg()).bg(theme.bg())
    };
    let mark = if current { "(*)" } else { "   " };
    Line::from(Span::styled(format!("  {label:<24} {mark}"), style))
}

fn selectable(label: &str, selected: bool, theme: Theme) -> Line<'static> {
    let style = if selected {
        Style::default()
            .fg(theme.bg())
            .bg(theme.cursor_bg())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg()).bg(theme.bg())
    };
    Line::from(Span::styled(format!("  {label:<24}"), style))
}

fn draw_dir(frame: &mut Frame, b: &mut crate::dir::DirBrowser, theme: Theme, area: Rect) {
    let rect = centered(
        area,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    frame.render_widget(Clear, rect);
    let title = format!("Dir: {}", b.dir.display());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(Style::default().fg(theme.fg()).bg(theme.bg()))
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height < 4 || inner.width == 0 {
        return;
    }

    let status = "DIR: Enter [drive:][path\\][filespec] (press <key> for current directory)";
    let status_rect = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            status,
            Style::default().fg(theme.accent()),
        ))),
        status_rect,
    );

    let footer_rows = 3u16;
    let list_h = inner.height.saturating_sub(1 + footer_rows) as usize;
    b.ensure_visible(list_h.max(1));
    let list_rect = Rect::new(inner.x, inner.y + 1, inner.width, list_h as u16);
    let mut lines = Vec::with_capacity(list_h);
    for (i, entry) in b.entries.iter().enumerate().skip(b.scroll).take(list_h) {
        let size = if entry.is_dir {
            "<DIR>".to_string()
        } else {
            format_size(entry.size)
        };
        let line = format!(
            "{}{:<36} {:>9}  {}",
            if entry.is_dir { "/" } else { " " },
            entry.name,
            size,
            format_time(entry.modified)
        );
        let style = if i == b.selected {
            Style::default()
                .fg(theme.bg())
                .bg(theme.cursor_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg()).bg(theme.bg())
        };
        lines.push(Line::from(Span::styled(line, style)));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), list_rect);

    let mut footer = Vec::new();
    if let Some(input) = &b.path_input {
        footer.push(Line::from(Span::styled(
            format!("Path: {input}_"),
            Style::default().fg(theme.header_fg()),
        )));
    } else if b.sort_prompt {
        footer.push(Line::from(Span::styled(
            "Sort by N(ame), E(xtension), D(ate) or S(ize)?",
            Style::default()
                .fg(theme.header_fg())
                .add_modifier(Modifier::BOLD),
        )));
    } else {
        footer.push(Line::from(Span::styled(
            "Scroll using <keys>, PgUp, PgDn, Home, End  -  Esc returns to CABLE",
            Style::default().fg(theme.dim()),
        )));
        footer.push(Line::from(Span::styled(
            format!(
                "{}   {} of {}   Enter = open   Ctrl+Enter = view in other window   Tab = target   S = sort   / = path",
                b.sort.label(),
                if b.entries.is_empty() {
                    0
                } else {
                    b.selected + 1
                },
                b.entries.len()
            ),
            Style::default().fg(theme.dim()),
        )));
    }
    if let Some(err) = &b.error {
        footer.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(theme.diff_bg()),
        )));
    }
    let footer_rect = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(footer.len() as u16),
        inner.width,
        footer.len() as u16,
    );
    frame.render_widget(Paragraph::new(Text::from(footer)), footer_rect);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width).max(3);
    let h = height.min(area.height).max(3);
    Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    )
}
