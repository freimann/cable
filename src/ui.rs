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
/// Width of the preview line built by `CS:0x3233` in `CS:0x171F`.
const PREVIEW_LEN: usize = 64;
/// The preview starts sixteen bytes before the cursor, so the active byte is
/// the seventeenth character of the line.
const PREVIEW_BEFORE: usize = 16;

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

    // The original is an 80x25 text screen.  Every other size uses the
    // adaptive layout, which fills the terminal but keeps each displayed line
    // a multiple of sixteen bytes wide.
    if area.width == ORIGINAL_COLS && area.height == ORIGINAL_ROWS {
        draw_original(frame, app, area);
    } else {
        draw_adaptive(frame, app, area);
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
    app.sync_geometry(PANE_ROWS, 16);
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

    let disasm = disasm_window(app, index, PANE_ROWS, 16);

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
                    if app.options.disasm.is_none() {
                        let ch = codec::display_byte(v, ebcdic, conceal);
                        put_cell(
                            frame,
                            x + ASCII_COL + i as u16,
                            line_y,
                            &ch.to_string(),
                            style,
                        );
                    }
                }
                None => {
                    put_cell(frame, bx, line_y, "**", normal);
                    if app.options.disasm.is_none() {
                        put_cell(frame, x + ASCII_COL + i as u16, line_y, " ", normal);
                    }
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

    // Draw the vertical instruction listing over the right-hand character
    // area.  The cursor's instruction was placed on the middle row by
    // `disasm_window`.
    if app.options.disasm.is_some() {
        for (row, cells) in disasm.iter().enumerate() {
            for (i, (ch, style)) in cells.iter().enumerate() {
                put_cell(
                    frame,
                    x + ASCII_COL + i as u16,
                    y + row as u16,
                    &ch.to_string(),
                    *style,
                );
            }
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

/// Number of bytes on each line of the adaptive layout.  The line is
/// `8 hex digits + space + 32n hex + (n-1) group gaps + 2 + 16n characters +
/// (n-1) group gaps`, i.e. `50n + 9` columns for `n` sixteen-byte groups.
/// Keeping whole groups means every line is a multiple of 16 bytes.
fn adaptive_bytes_per_row(width: u16) -> usize {
    let groups = ((width as usize).saturating_sub(9) / 50).clamp(1, 8);
    groups * 16
}

/// Adaptive layout used at every size except the original 80x25 screen.  Both
/// windows, the position band and the footer are sized from the terminal, so
/// the display fills most of the enclosing window.  Data lines are widened by
/// whole sixteen-byte groups on the horizontal axis and by whole lines on the
/// vertical axis.
fn draw_adaptive(frame: &mut Frame, app: &mut App, area: Rect) {
    // On very short terminals the footer shrinks before the data does.
    let footer_h = FOOTER_ROWS.min(area.height).min((area.height / 3).max(1));
    let body_h = area.height.saturating_sub(footer_h);
    // Two rows for the `Hex`/`Dec` values and previews; they are dropped when
    // there is not enough room for a useful amount of data.
    let middle_h = if body_h >= 12 { 2 } else { 0 };
    let panes_h = body_h.saturating_sub(middle_h);
    let top_h = panes_h / 2;
    let bottom_h = panes_h - top_h;
    let top = Rect::new(area.x, area.y, area.width, top_h);
    let middle = Rect::new(area.x, area.y + top_h, area.width, middle_h);
    let bottom = Rect::new(area.x, area.y + top_h + middle_h, area.width, bottom_h);
    let footer = Rect::new(area.x, area.y + body_h, area.width, footer_h);

    let bytes_per_row = adaptive_bytes_per_row(area.width);
    // Both windows must use the same row count for cursor movement, so take
    // the smaller of the two bodies (they differ by at most one row).
    let rows = top_h.saturating_sub(1).min(bottom_h.saturating_sub(1)) as usize;
    app.sync_geometry(rows, bytes_per_row);

    draw_pane(frame, app, 0, top);
    draw_pane(frame, app, 1, bottom);
    if middle_h > 0 {
        draw_adaptive_middle(frame, app, middle);
    }
    draw_footer(frame, app, footer);
}

/// Field widths of the `Hex`/`Dec` values.  They are derived from the largest
/// offset in either pane rather than from the current cursor, so the values
/// keep the same width while browsing and the preview never slides sideways as
/// the decimal count grows.
fn adaptive_offset_widths(app: &App) -> (usize, usize) {
    let max = app.panes.iter().map(|pane| pane.len()).max().unwrap_or(0);
    let hex = format!("{max:X}").len().max(8);
    let dec = max.to_string().len().max(1);
    (hex, dec)
}

/// Horizontal middle of the hex columns of one adaptive data line.  Each byte
/// is two columns wide, sixteen-byte groups are separated by one column, and
/// the offset field is `data_col` wide, so `n` groups occupy `33n - 1` columns.
/// With four groups the middle falls between the second and third group.
fn adaptive_hex_center(bytes_per_row: usize, data_col: usize) -> usize {
    let groups = (bytes_per_row / 16).max(1);
    data_col + (33 * groups - 1) / 2
}

/// The two-line adaptive position band: cursor offset in both notations and
/// the original 64-byte preview.  The original uses box-drawing crosshairs to
/// connect these lines to the cursor byte, but the adaptive layout has no fixed
/// byte columns to point at.  Instead the preview is placed so that its cursor
/// character (the seventeenth, since the preview starts sixteen bytes early)
/// sits in the horizontal middle of the hex display, and both the offset fields
/// and the preview start are fixed widths so the line does not shift while the
/// file is browsed.
fn draw_adaptive_middle(frame: &mut Frame, app: &mut App, rect: Rect) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let theme = app.options.theme;
    let normal = Style::default().fg(theme.fg()).bg(theme.bg());
    let cursor_style = Style::default()
        .fg(theme.cursor_fg())
        .bg(theme.cursor_bg())
        .add_modifier(Modifier::BOLD);

    let (hex_width, dec_width) = adaptive_offset_widths(app);
    let data_col = hex_width + 1;
    let center = adaptive_hex_center(app.bytes_per_row, data_col);
    let preview_col = center.saturating_sub(PREVIEW_BEFORE);

    for index in 0..2usize {
        if index as u16 >= rect.height {
            break;
        }
        let y = rect.y + index as u16;
        let active = app.active == index;
        let style = if active { bar_style(theme) } else { normal };
        if active {
            restyle_cells(frame, rect.x, y, rect.x + rect.width, style);
        }

        let cursor = app.panes[index].cursor;
        let head = format!(
            "Hex {:0hw$X}  Dec {:>dw$}",
            cursor,
            cursor,
            hw = hex_width,
            dw = dec_width
        );
        let head_len = head.chars().count();
        // Keep two spaces after the values, and otherwise start the preview at
        // the column that centres its cursor character in the hex display.
        let start = preview_col.max(head_len + 2);
        let mut spans = vec![Span::styled(head, style)];
        if start < rect.width as usize {
            spans.push(Span::styled(" ".repeat(start - head_len), style));
            let room = rect.width as usize - start;
            let preview = preview_text(app, index);
            for (i, ch) in preview.chars().take(room.min(PREVIEW_LEN)).enumerate() {
                let cell = if i == PREVIEW_BEFORE {
                    cursor_style
                } else {
                    style
                };
                spans.push(Span::styled(ch.to_string(), cell));
            }
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(rect.x, y, rect.width, 1),
        );
    }
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

    // The original puts the first file's title above its window and the second
    // file's title below its window, so the two headers bracket the display.
    let header_at_bottom = index == 1;
    let header_y = if header_at_bottom {
        rect.y + rect.height.saturating_sub(1)
    } else {
        rect.y
    };
    let header = Rect::new(rect.x, header_y, rect.width, 1);
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
        if header_at_bottom { rect.y } else { rect.y + 1 },
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

    let bpr = app.bytes_per_row.max(16) as u64;
    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for row in 0..rows {
        let offset = top.saturating_add((row as u64) * bpr);
        lines.push(hex_line(app, index, offset));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().fg(theme.fg()).bg(theme.bg())),
        body,
    );

    // The disassembly view is a vertical listing in the right-hand character
    // area: one instruction per screen row, with the instruction at the cursor
    // on the middle row and the rest scrolling around it.
    if app.options.disasm.is_some() {
        let groups = (bpr as usize / 16).max(1);
        let prefix = 9 + (33 * groups - 1) + 2;
        if prefix < body.width as usize {
            let width = body.width as usize - prefix;
            let window = disasm_window(app, index, rows, width);
            for (row, cells) in window.iter().enumerate() {
                let y = body.y + row as u16;
                for (i, (ch, style)) in cells.iter().enumerate() {
                    put_cell(
                        frame,
                        body.x + prefix as u16 + i as u16,
                        y,
                        &ch.to_string(),
                        *style,
                    );
                }
            }
        }
    }
}

fn hex_line(app: &mut App, index: usize, offset: u64) -> Line<'static> {
    let theme = app.options.theme;
    let active = app.active == index;
    let cursor = app.panes[index].cursor;
    let compare = app.compare_mode;
    let ebcdic = app.options.ebcdic;
    let conceal = app.options.conceal;
    let bpr = app.bytes_per_row.max(16);

    let normal = Style::default().fg(theme.fg()).bg(theme.bg());
    let dim = Style::default().fg(theme.dim()).bg(theme.bg());
    let cursor_style = Style::default()
        .fg(theme.cursor_fg())
        .bg(theme.cursor_bg())
        .add_modifier(Modifier::BOLD);
    let diff_style = Style::default().fg(theme.diff_fg()).bg(theme.diff_bg());
    let offset_style = Style::default().fg(theme.accent()).bg(theme.bg());

    let mut spans = Vec::with_capacity(3 * bpr + 16);
    spans.push(Span::styled(format!("{offset:08X} "), offset_style));

    let mut bytes = Vec::with_capacity(bpr);
    let mut other = Vec::with_capacity(bpr);
    for i in 0..bpr {
        bytes.push(byte_at(app, index, offset + i as u64));
        other.push(if compare {
            byte_at(app, 1 - index, offset + i as u64)
        } else {
            None
        });
    }

    // Styles are computed once so the hex pair and the character column can
    // share the cursor/difference attribute.
    let mut styles = Vec::with_capacity(bpr);
    for i in 0..bpr {
        let off = offset + i as u64;
        let is_cursor = active && off == cursor;
        // In compare mode a byte that exists in one file but not the other is
        // a length difference; equal bytes are shown normally.  `**` is
        // reserved for positions past the end of *this* file, matching the
        // original renderer (CS:0x3ba9 writes `**` when CS:0x3884 reports no
        // byte).
        let differs = compare && bytes[i] != other[i];
        styles.push(if is_cursor {
            cursor_style
        } else if differs {
            diff_style
        } else {
            normal
        });
    }

    for i in 0..bpr {
        if i > 0 && i % 16 == 0 {
            spans.push(Span::raw(" "));
        }
        match bytes[i] {
            Some(v) => spans.push(Span::styled(format!("{v:02X}"), styles[i])),
            None => spans.push(Span::styled("**", dim)),
        }
    }

    spans.push(Span::raw("  "));
    if app.options.disasm.is_none() {
        for i in 0..bpr {
            if i > 0 && i % 16 == 0 {
                spans.push(Span::raw(" "));
            }
            match bytes[i] {
                Some(v) => {
                    let ch = codec::display_byte(v, ebcdic, conceal);
                    spans.push(Span::styled(ch.to_string(), styles[i]));
                }
                None => spans.push(Span::styled(" ", dim)),
            }
        }
    }
    Line::from(spans)
}

/// Build a vertically centred instruction listing for one window.  The
/// instruction containing the cursor is placed on the middle row, earlier
/// instructions above it and later ones below.  Each entry is one screen row
/// of `(character, style)` cells, padded to `width`.
///
/// Decoding starts on a 256-byte boundary rather than at the cursor, so the
/// instruction boundaries stay stable while the cursor moves inside a block.
/// The listing then scrolls one instruction at a time to keep the current
/// instruction on the middle row.
fn disasm_window(
    app: &mut App,
    index: usize,
    rows: usize,
    width: usize,
) -> Vec<Vec<(char, Style)>> {
    let theme = app.options.theme;
    let normal = Style::default().fg(theme.fg()).bg(theme.bg());
    // The current instruction is shown in bold rather than as a reversed
    // block, so the listing stays easy to scan.
    let current_style = normal.add_modifier(Modifier::BOLD);
    let diff_style = Style::default().fg(theme.diff_fg()).bg(theme.diff_bg());

    let mut out = vec![vec![(' ', normal); width]; rows];
    let Some(cpu) = app.options.disasm else {
        return out;
    };
    if rows == 0 || width == 0 || app.panes[index].file.is_none() {
        return out;
    }

    let compare = app.compare_mode;
    let cursor = app.panes[index].cursor;
    let center = rows / 2;

    // Decode a generous window around the cursor.  Rounding the start to 256
    // bytes keeps instruction alignment stable while scrolling through a block.
    let anchor = (cursor.saturating_sub(512) / 256) * 256;
    let end = cursor.saturating_add(512).min(app.panes[index].len());
    let mut bytes = Vec::with_capacity(end.saturating_sub(anchor) as usize);
    for off in anchor..end {
        match byte_at(app, index, off) {
            Some(b) => bytes.push(b),
            None => break,
        }
    }
    let insns = crate::disasm::disassemble(cpu, &bytes);
    if insns.is_empty() {
        return out;
    }

    let rel = cursor.saturating_sub(anchor) as usize;
    let ci = insns.iter().rposition(|i| i.offset <= rel).unwrap_or(0);
    let on_instruction = rel >= insns[ci].offset && rel < insns[ci].offset + insns[ci].len;

    for (row, cells) in out.iter_mut().enumerate() {
        let target = row as isize - center as isize;
        let idx = ci as isize + target;
        if idx < 0 || idx >= insns.len() as isize {
            continue;
        }
        let insn = &insns[idx as usize];
        let abs = anchor + insn.offset as u64;
        let is_cursor = on_instruction && idx as usize == ci;
        let differs = compare
            && (0..insn.len).any(|k| {
                let off = abs + k as u64;
                match (byte_at(app, index, off), byte_at(app, 1 - index, off)) {
                    (Some(a), Some(b)) => a != b,
                    (None, None) => false,
                    _ => true,
                }
            });
        let style = if is_cursor {
            current_style
        } else if differs {
            diff_style
        } else {
            normal
        };
        let text = if width >= 24 {
            format!("{abs:08X}  {}", insn.text)
        } else {
            insn.text.clone()
        };
        *cells = text.chars().take(width).map(|ch| (ch, style)).collect();
        cells.resize(width, (' ', normal));
    }
    out
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

/// Build the original 64-character preview: bytes `cursor-16` through
/// `cursor+47`, with blanks outside the file (`CS:0x3233` → `CS:0x32BF`).
fn preview_text(app: &mut App, index: usize) -> String {
    let cursor = app.panes[index].cursor;
    let start = cursor as i64 - PREVIEW_BEFORE as i64;
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

/// Re-apply a style to a row span without changing the glyphs already there.
fn restyle_cells(frame: &mut Frame, from: u16, y: u16, to: u16, style: Style) {
    for x in from..to {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
            cell.set_style(style);
        }
    }
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
    // The original 80x25 screen has no room for extra hints, but the adaptive
    // footer does, and Scroll Lock is absent from most Mac keyboards (and is
    // not mapped by crossterm on Windows).  F7 is the most reliable alias.
    let status = if app.message.is_some() {
        app.status_line()
    } else {
        format!("{}  (F7 / Ctrl+L toggles)", app.status_line())
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(status, status_style))),
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
/// reverse video.  The accelerator letter of every command is bold, as in the
/// original, whose screen attributes set it apart from the rest of the word.
/// Shared by the original and adaptive footers.
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
        let base = if i == app.menu_index {
            Style::default().fg(theme.bar_fg()).bg(theme.bar_bg())
        } else {
            Style::default().fg(theme.fg()).bg(theme.bg())
        };
        let mut chars = label.chars();
        if let Some(first) = chars.next() {
            let mut head = base;
            head = head.add_modifier(Modifier::BOLD);
            menu.push(Span::styled(first.to_string(), head));
            let rest: String = chars.collect();
            if !rest.is_empty() {
                menu.push(Span::styled(rest, base));
            }
        }
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
            let labels = ["Attributes", "Color", "Help", "Mono", "Quit"];
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A line with `n` sixteen-byte groups is
    /// `9 + (32n + n-1) + 2 + (16n + n-1)` columns long.
    fn line_columns(bytes: usize) -> usize {
        let groups = bytes / 16;
        9 + (32 * groups + groups - 1) + 2 + (16 * groups + groups - 1)
    }

    #[test]
    fn adaptive_lines_are_whole_sixteen_byte_groups() {
        for width in 59..=400u16 {
            let bytes = adaptive_bytes_per_row(width);
            assert_eq!(bytes % 16, 0, "width {width}");
            assert!(bytes >= 16, "width {width}");
            assert!(
                line_columns(bytes) <= width as usize,
                "width {width} needs {} columns for {bytes} bytes",
                line_columns(bytes)
            );
        }
        assert_eq!(adaptive_bytes_per_row(80), 16);
        assert_eq!(adaptive_bytes_per_row(120), 32);
        assert_eq!(adaptive_bytes_per_row(160), 48);
    }

    #[test]
    fn preview_cursor_sits_between_the_middle_hex_groups() {
        // 240 columns: four groups; the middle falls between groups two and
        // three, i.e. after the second 32-column group.
        let bpr = adaptive_bytes_per_row(240);
        assert_eq!(bpr, 64);
        assert_eq!(adaptive_hex_center(bpr, 9), 74);
        // 132 columns: two groups; the middle is the gap between them.
        let bpr = adaptive_bytes_per_row(132);
        assert_eq!(bpr, 32);
        assert_eq!(adaptive_hex_center(bpr, 9), 41);
        // 160 columns: three groups; the middle is the centre of the second.
        let bpr = adaptive_bytes_per_row(160);
        assert_eq!(bpr, 48);
        assert_eq!(adaptive_hex_center(bpr, 9), 58);
    }

    #[test]
    fn disasm_window_centres_the_cursor_instruction() {
        let path = std::env::temp_dir().join(format!("cable-ui-{}.bin", std::process::id()));
        std::fs::write(&path, [0x90u8; 64]).unwrap();
        let mut options = crate::app::Options::default();
        options.disasm = Some(crate::disasm::Cpu::X86);
        let mut app = crate::app::App::new(&[path.clone()], options).unwrap();
        app.panes[0].cursor = 4;

        let rows = 9;
        let window = disasm_window(&mut app, 0, rows, 40);
        assert_eq!(window.len(), rows);
        assert!(window.iter().all(|row| row.len() == 40));

        // One NOP per row, and the listing is scrolled so that the instruction
        // containing the cursor (offset 4) lands on the middle row.
        for (row, cells) in window.iter().enumerate() {
            let line: String = cells.iter().map(|(ch, _)| *ch).collect();
            assert!(
                line.starts_with(&format!("{row:08X}  nop")),
                "row {row}: {line:?}"
            );
        }
        assert!(!window[0][0].1.add_modifier.contains(Modifier::BOLD));
        assert!(window[rows / 2][0].1.add_modifier.contains(Modifier::BOLD));

        std::fs::remove_file(path).unwrap();
    }
}
