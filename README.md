# cable-rs

A modern, cross-platform Rust reimplementation of **CABLE V1.7X** — *Compare And
Browse with Limited Editing*, the DOS-era IBM hex editor/comparer.

The original is a 16-bit DOS `.COM` program. This clone keeps its two-window
workflow, command vocabulary, Option/Mode menus and exact user-facing messages,
while running as a normal terminal application on macOS, Linux and Windows.

- Original analysis: [`ANALYSIS.md`](ANALYSIS.md)
- Original artifact: `../cable.com`
- Screenshot reference: `../Screenshot 2026-11-10 at 17.12.44.png`

---

## Build and run

```sh
cd cable-rs
cargo build --release
./target/release/cable-rs [OPTIONS] [FILE1 [FILE2]]
```

Requires a Rust toolchain with edition 2024 support and a real terminal
(ratatui/crossterm). With no file arguments the built-in directory browser opens.

```sh
cargo test      # unit tests
cargo fmt
cargo clippy    # optional
```

### Release binaries

`dist/` contains the cross-compiled release builds:

| File | Platform |
| --- | --- |
| `cable-rs-v0.2.3-linux-x86_64` | Linux ELF, glibc 2.17 or newer |
| `cable-rs-v0.2.3-windows-x86_64.exe` | Windows 10/11 console (x86-64) |

Both were produced from macOS with rustup targets and two linkers:

```sh
rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-gnu

# Linux: Zig supplies the linker and a glibc 2.17 sysroot.
printf '#!/bin/sh\nexec zig cc -target x86_64-linux-gnu.2.17 "$@"\n' > zig-linux
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$PWD/zig-linux" \
    cargo build --release --target x86_64-unknown-linux-gnu

# Windows: the mingw-w64 toolchain supplies the linker and import libraries.
cargo build --release --target x86_64-pc-windows-gnu
```

### Command-line options

| Option | Meaning |
| --- | --- |
| `--mono` | start in mono |
| `--colour`, `--color` | start in colour (default) |
| `--amber` | start in amber |
| `--ascii` | treat data as ASCII (default) |
| `--ebcdic` | treat data as EBCDIC (CP037) |
| `--conceal` | show unprintables as `.` (default) |
| `--show` | show unprintables as they are |
| `--respect-case` | case-sensitive search/compare (default) |
| `--ignore-case` | case-insensitive search/compare |
| `--hex` | HEX search/edit input |
| `--character` | character search/edit input (default) |
| `--forward` | compare/search forwards (default) |
| `--backward` | compare/search backwards |
| `--active` | scroll only the active window (default) |
| `--both` | scroll both windows together |
| `-h`, `--help` | show help |
| `-V`, `--version` | show version |

---

## Screen layout

On an 80x25 terminal the clone reproduces CABLE's text-mode screen:

```
cable.com                                                   16642 00004102    A
               ** ** ** ** ** ** ** ** ** ** ** ** ** ** ** **
F2:Respect     ** ** ** ** ** ** ** ** ** ** ** ** ** ** ** **
F3:Forward     ** ** ** ** ** ** ** ** ** ** ** ** ** ** ** **
F4:Character   ** ** ** ** ** ** ** ** ** ** ** ** ** ** ** **
F5:Conceal     ** ** ** ** **╔E9╗FA 18 2A 07 70 1E 78 FF 00 00       Θ·.*.p.x ..
F6:ASCII       00 00 06 07 09║01║08 06 08 05 05 05 0A 06 04 09  ................
SL:Active      5C 63 61 62 6C║65║2E 68 6C 70 00 20 20 20 20 20  \cable.hlp.
╔════════════╗ 20 20 20 20 20║20║20 20 20 20 20 20 20 20 20 20
║Hex 00000000╠═══════════════╩═╦╝
╚Dec        0║                 Θ·.*.p.x ..................\cable.hlp.
╔Dec        0║                 Θ·.*.p.x ..................\cable.hlp.
║Hex 00000000╠═══════════════╦═╩╗
╚════════════╝ ** ** ** ** **║**║** ** ** ** ** ** ** ** ** **
CABLE V1.7X    ** ** ** ** **║**║** ** ** ** ** ** ** ** ** **
IBM Internal   ** ** ** ** **║**║** ** ** ** ** ** ** ** ** **
Use Only       ** ** ** ** **║**║** ** ** ** ** ** ** ** ** **
               ** ** ** ** **╚E9╝FA 18 2A 07 70 1E 78 FF 00 00       Θ·.*.p.x ..
Dave Mitchell  00 00 06 07 09 01 08 06 08 05 05 05 0A 06 04 09  ................
ZENO @ WINVMB  5C 63 61 62 6C 65 2E 68 6C 70 00 20 20 20 20 20  \cable.hlp.
               20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20
cable.com                                                   16642 00004102    A
CABLE: Compare Dir Edit Help Mode Next Options + - = Quit Search View Write
Type first letter of command or press ◄─┘ to select hilited command
```

- The left 14-column strip holds the `F2:`-`SL:` option legend, the signed-on
  banner, and the two overlapping `Hex`/`Dec` boxes. The data windows start at
  column 15 and their ASCII column is 64; the panes overlap the band on rows 8
  and 13.
- The cursor is always the sixth byte of each window's fifth displayed row.
  The original draws a fixed crosshair there (`╔`/`╗` for the top window,
  `╚`/`╝` for the bottom) and scrolls the data underneath it, so a window
  always shows 69 bytes before the cursor and 58 after it. Positions before the
  start or past EOF render as `**`.
- The boxes show the cursor offset (`Hex 00000000`, `Dec        0`) and the
  active window's values and preview are drawn on the grey selection bar.
- Each `Dec` row carries a 64-character preview of the bytes starting sixteen
  before the cursor; bytes outside the file are blanks.
- The title row shows the file name, decimal size, size in hex, and the DOS
  attribute (`A` for a normal archive file, `R` for read-only).
- Row 23 describes the highlighted command; `Help` is highlighted at start-up,
  which is why the first screen shows the generic hint (including the original
  `◄─┘` Enter keycap). The accelerator letter of each command on the `CABLE:`
  line is bold, as in the original's screen attributes.
- At exactly 80×25 the screen above is reproduced cell for cell. At every other
  size the clone uses an adaptive layout that fills the terminal: each window
  takes half the available body, the footer stays four rows, and data lines are
  widened by whole sixteen-byte groups. A 120-column terminal shows 32 bytes per
  line, 160 columns shows 48, and so on; lines never end on a partial group.
  `Up`/`Down` move by one displayed line, and `PgUp`/`PgDn` by one window, so
  multi-group lines scroll naturally. The first file's header sits above the
  upper window and the second file's header below the lower window, as on the
  original screen. The position band keeps the `Hex`/`Dec`
  values in fixed-width fields, so growing offsets do not move the preview. The
  preview character at the cursor position is drawn in reverse video and placed
  in the horizontal middle of the hex columns (with four groups, between the
  second and third), where the original's fixed crosshair used to point.


---

## Commands

Type the first letter of a command, or move the highlight with `Space` / `F9` /
`F10` and press `Enter`.

| Command | Key | Action |
| --- | --- | --- |
| Compare | `C` | compare from the current position |
| Dir | `D` | directory browser |
| Edit | `E` | edit in the active window |
| Help | `H`, `F1` | help |
| Mode | `M` | Colour/Mono/Attributes |
| Next | `N` | repeat the last compare or search |
| Options | `O` | option menu |
| `+` | `+` | move forward by a value |
| `-` | `-` | move backward by a value |
| `=` | `=` | set position |
| Quit | `Q` | quit (confirms unsaved edits) |
| Search | `S` | search |
| View | `V` | view a filespec |
| Write | `W` | write changes (Y/N confirmation) |

### Keys

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | switch active window |
| Arrows | move cursor (`Left`/`Right` one byte, `Up`/`Down` one displayed line) |
| `PgUp` / `PgDn` | move one screen |
| `Home` / `End` | start / end of file |
| `F1` | help |
| `F2` | toggle respect/ignore case |
| `F3` | toggle forward/backward |
| `F4` | toggle character/HEX input |
| `F5` | toggle conceal/show |
| `F6` | toggle ASCII/EBCDIC |
| `Scroll Lock` / `F7` / `Ctrl+L` / `Ctrl+B` | toggle `SL:Active` (active window only) / `SL:Both` (both windows) |
| `F9` / `F10` | move the highlighted menu command |
| `Space` | advance the highlighted menu command |
| `Enter` | run the highlighted command |
| `Esc` | close a modal / leave compare display |

### Offset input (`+`, `-`, `=`)

Values are decimal by default; prefix with `H` for hex, e.g. `H10` = 16.
`=` takes a new absolute position; `+` and `-` are relative.

---

## Editing

`E` enters edit mode using the current input mode:

- **Character** — type ASCII or EBCDIC characters; the byte under the cursor is
  replaced and the cursor advances.
- **HEX** — type pairs of hex digits (`DE AD BE EF`); the high nibble is buffered
  until the low nibble arrives.
- `Backspace` steps back; arrows/PgUp/PgDn/Home/End navigate.
- `End` positions the cursor at end-of-file, so a following character appends.
- `Esc` leaves edit mode. `Tab` switches to the other window and stays in edit.

Edits are kept in memory. `W` asks for confirmation before writing. Read-only
files may be edited in memory but cannot be written; the original message
`File is read-only - can't write` is reported.

Writes go to a temporary file in the same directory and are atomically renamed,
so an interrupted write cannot truncate the original. The file is reopened after
saving, so reads and the pane length refresh immediately.

---

## Compare display: what `**` means

`**` marks a cell with **no data** — that is, a position at or past end of file.
It is *not* an equality mask. Both the original disassembly (the row renderer at
`CS:0x3BA9` writes `**` when the byte reader at `CS:0x3884` reports no byte) and
the supplied screenshot (two 7-byte `mov ax,0054h/int 10h/int 20h` stubs with
`**` after byte 7) agree.

Equal bytes are displayed normally. In compare mode the clone additionally tints
differing bytes so they are easy to spot; that tint is a modern convenience, not
part of the original display.

A pure length difference is reported at the first byte past the shorter file.
The shorter pane shows `**` there while the longer pane shows its byte.

---

## Options and modes

### Options (`O`)

The order and effects match the original label string
`ASCII Back Conceal EBCDIC Fore Help Ignore Quit Respect Show heX/char`:

- `ASCII` / `EBCDIC` — data and search character set.
- `Back` / `Fore` — compare/search direction.
- `Conceal` / `Show` — unprintable display.
- `Ignore` / `Respect` — case handling.
- `Help` — contextual help.
- `Quit` — close the menu.
- `heX/char` — search/edit input mode.

### Mode (`M`)

`Attributes`, `Color`, `Help`, `Mono`, `Quit` — matching the original
`MODE: Attributes Color Help Mono Quit` order. `Attributes` offers the
classic blue/yellow, amber and mono themes.

### Directory (`D`)

- `Enter` opens the selected file (or enters a directory).
- `Ctrl+Enter` opens it in the other window.
- `Tab` changes the target window.
- `S` sorts: `N`ame, `E`xtension, `D`ate, `S`ize.
- `/` edits the path.
- `Backspace` goes to the parent directory.
- `Esc` returns to CABLE.

The original DIR prompt and scroll footer are reproduced verbatim.

---

## Project layout

| File | Responsibility |
| --- | --- |
| `src/main.rs` | CLI parsing, terminal setup and event loop |
| `src/app.rs` | application state, key handling, commands, options, modals |
| `src/buffer.rs` | paged file storage, edit overlay, atomic save |
| `src/codec.rs` | ASCII/EBCDIC (CP037), display characters, offset parsing |
| `src/compare.rs` | forward/backward search and difference scanning |
| `src/dir.rs` | directory listing, sorting, path handling |
| `src/help.rs` | reconstructed help text |
| `src/ui.rs` | ratatui panes, footer, modals, directory browser |

---

## Differences from the original

These are deliberate or unavoidable deviations; see `ANALYSIS.md` §15 for the
evidence and open questions.

- Compares absolute file offsets rather than each pane's own cursor position.
- 64 KiB pages instead of the original 4 KiB/4-page cache (invisible to users).
- Help is built in because the original `cable.hlp` was not supplied.
- Scroll Lock may not be reported by every terminal (and is missing from most
  Mac keyboards); `F7`, `Ctrl+L`, `Ctrl+B` and `--both` / `--active` provide
  equivalents. Windows terminals that do not deliver `Ctrl+L` as a control
  chord still accept the bare form-feed byte, and `F7` is always available.
- Colour themes replace raw per-cell attribute editing.
- Compare mode tints all visible differences (modern aid).
- No DOS memory/adaptor checks and no EGA/132-column text modes.
- The original's fixed 80/132-column text modes are replaced by a layout that
  adapts to the terminal; 80×25 still reproduces the original screen exactly.

---

## Verification

`cargo test` covers:

- paged reads/edits, append, save-without-changes, and reads after save,
- forward/backward search, case-insensitive search,
- first/last difference, length-difference reporting and Next behaviour,
- hex/offset parsing, ASCII/EBCDIC display,
- directory sorting by name, extension, date and size.

Manual terminal checks (pty) cover the initial UI, compare/next/search, editing
in HEX/character, append, save/reload, read-only write errors, quit
confirmation, directory navigation/sorting/path/target pane, EBCDIC/HEX search,
offset commands, F2–F6/F9/F10, Tab, Options, Mode and Help.
