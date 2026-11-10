# CABLE 1.7X — Reverse-Engineering Analysis

This document records what was recovered from the original DOS binary and how the
Rust clone (`cable-rs`) maps onto it. Evidence is labelled **observed** (taken
directly from the disassembly, decompiler output, or the supplied screenshot),
**inferred** (a reasonable conclusion from the evidence), or **unknown**.

---

## 1. Target identity and method

| Item | Value |
| --- | --- |
| Artifact | `cable.com` |
| SHA-256 | `94dea6121b269f33f7344ec1465904fd19a6727567d171575d420d1cd7ef0ebd` |
| Size | 16,642 bytes |
| Format | DOS `.COM`, 16-bit real-mode x86 |
| Embedded title | `CABLE V1.7X` — *Compare And Browse with Limited Editing* |
| Authorship strings | `IBM Internal Use Only`, `Dave Mitchell`, `ZENO @ WINVMB` |

**Observed.** Analysis was performed with REA using the Ghidra 12.1.4 provider
(`dos-com` loader). All 116 functions recognised by Ghidra were decompiled to
`rea-decompiled/functions/`; the regions Ghidra did not promote to functions
(notably the compare loop at `CS:0x2CFD`) were disassembled directly.

### Address convention

A DOS `.COM` program is loaded with the PSP at `CS:0`, and execution starts at
`CS:0x100`; `DS = SS = CS` for this program.

- **`CS:0xNNNN`** below means the offset inside the code/data segment.
- File offset = `CS offset − 0x100`.
- Ghidra linear address = `0x10100 + file offset` = `0x10000 + CS offset`.
- Example: compare loop `CS:0x2CFD` = file `0x2BFD` = Ghidra linear `0x12CFD`.

---

## 2. Startup and runtime environment

**Observed.**

- Entry is `CS:0x100` (`linear 0x10100`, Ghidra `FUN_1000_19fd`) and jumps to the
  main initialisation at `CS:0x19FD`.
- The program uses DOS `INT 21h` for file I/O and `INT 16h` for the keyboard, and
  writes the screen via `INT 10h` plus direct video memory at `B800:` (colour) or
  `B000:` (mono).
- Memory checks are present for 47 KiB, 58 KiB and 64 KiB:
  - `CABLE can't run (less than 47K free)` (`CS:0x1445`)
  - `$HELP and DIR suppressed (less than 58K free)` (`CS:0x13F0`)
  - `$DIR suppressed (less than 64K free)` (`CS:0x141F`)
- DOS 2.0 or later is required: `CABLE requires DOS Version 2 or later`
  (`CS:0x13C9`).
- Two video modes are supported, selected from the `Mode` menu: Colour (`C`) and
  Mono (`M`). Errors: `No Colour Adapter installed` (`CS:0x1299`),
  `No Mono Adapter installed` (`CS:0x12F3`), `Video mode must be C or M`
  (`CS:0x130E`).

**Inferred.** The program is a conventional single-segment COM application with
a static near-data area; there is no DOS extender and no protected-mode code.

---

## 3. Per-window data model and paging

**Observed.** `CS:0x3393` selects one of two window contexts by returning
`BX = 0` (window 0) or `BX = 0xCB` (window 1). All window state is addressed as
`[BX + offset]`:

| Offset | Meaning |
| --- | --- |
| `+0x18A6` | filename / path buffer (up to 0x40 bytes) |
| `+0x18FC` | DOS file handle |
| `+0x18FE` | file-open flag (`1` = open) |
| `+0x18FF` | redraw-required flag |
| `+0x1900` | read-error flag |
| `+0x1905` / `+0x1907` | last valid page / last valid offset (EOF) |
| `+0x190D` / `+0x190F` | current page / current offset (cursor) |
| `+0x1915` … `+0x191B` | four cached page numbers (16-bit each) |
| `+0x191D` … `+0x1920` | four page-present bytes |
| `+0x1921` … `+0x1927` | four page-valid-length words |
| `+0x1929` … `+0x192F` | four page-buffer pointers |
| `+0x1931` | cursor/progress counter, reset to `0x64` |

**Observed page mechanism.** `CS:0x3AA2` loads a page with `INT 21h AH=3Fh`
(Read) for `0x1000` bytes. `CS:0x3B02` seeks with `INT 21h AH=42h`, multiplying
the page number by the value at `[0x1823]`. `CS:0x38E4` advances the cursor:

```
inc cx
cmp cx, 0x1000
je  wrap          ; offset == 4096 -> offset = 0, page++
```

Therefore:

- **Page size is 4 KiB** (`0x1000`).
- **Each window caches four pages** (16 KiB) — `CS:0x3B19` searches the four
  slots and evicts/reloads when the desired page is absent.
- Because the page number is a byte and the offset is 12 bits, the original can
  address roughly **1 MiB** (`0xFF × 0x1000 + 0xFFF`).
- The sentinel page value `0xFF` marks "no file / no data".

**Clone mapping.** `src/buffer.rs` uses a lazily loaded page cache with 64 KiB
pages plus a sparse edit overlay. The page size is an implementation detail and
does not affect file-visible behaviour. Saves are written to a temporary file
and atomically renamed; the file handle is reopened afterwards so subsequent
reads observe the new contents.

### 3.1 Screen layout (80x25 text mode)

**Observed** from the text-mode capture `cable.txt`, the pixel captures
(`cable_000.png`–`cable_003.png`) and the binary. CABLE uses the full 80x25
text screen:

| Screen rows | Content |
| --- | --- |
| 0 | top window title |
| 1–8 | top window body: eight rows of sixteen bytes |
| 8–13 | six-row position band (columns 0–14), plus the two 64-character previews on rows 10 and 11 (columns 15–78) |
| 13–20 | bottom window body |
| 21 | bottom window title |
| 22 | command menu |
| 23 | message line (highlighted-command description) |
| 24 | blank |

Rows 8 and 13 are shared: columns 0–14 hold the band art while columns 15+
hold the top window's last and the bottom window's first data row. There is no
per-row offset column; the hex area starts at column 15 and the character area
at column 64.

**Slot table.** A window context is selected by `CS:0x3393` (`BX = 0` or
`BX = 0xCB`). Each context has a 21-entry table at `[BX+0x1867]` of 3-byte
entries `[length, row, column]`; `FUN_1000_3D81` prints entry `AL` from the
string in `DX` at `row*0xA0 + column*2` (attributes are preserved).
`FUN_1000_3E26` applies an attribute to the same rectangle without printing.

| Slot | Length | Row | Col | Content |
| --- | --- | --- | --- | --- |
| 0 | 56 | 0 / 21 | 0 | file name |
| 1 | 17 | 0 / 21 | 57 | `{size:>8} {size:08X}` |
| 2 | 5 | 0 / 21 | 75 | `{attrs:>4} ` (`A` archive, `R` read-only) |
| 3 | 8 | 10 / 11 | 5 | decimal cursor position |
| 4 | 64 | 10 / 11 | 15 | character preview |
| 5–12 | 65 | 1–8 / 13–20 | 15 | the eight data rows |
| 13 | 8 | 9 / 12 | 5 | hexadecimal cursor position |
| 14 | 9 | 4 | 3 | `Character` / `HEX` |
| 15 | 6 | 7 | 3 | `Active` / `Both` |
| 16 | 2 | 5 / 17 | 30 | cursor byte pair (attribute only) |
| 17 | 7 | 2 | 3 | `Respect` / `Ignore` |
| 18 | 7 | 5 | 3 | `Conceal` / `Show` |
| 19 | 6 | 6 | 3 | `ASCII` / `EBCDIC` |
| 20 | 8 | 3 | 3 | `Forward` / `Backward` |

Slots 21–25 are unused padding (`0x20 0x20 0x20`).

**Title rows.** `FUN_1000_3201` prints slot 0 from `[BX+0x18A6]`, slot 1 from
`[BX+0x18EB]` and slot 2 from `[BX+0x18E6]` (`CS:0x0640` is a blank template).
The fields occupy columns 0–55, 57–73 and 75–79; columns 56 and 74 are
separators that keep the normal attribute even when the window is active. In
the capture (`cable.txt`, row 0):

```
cable.com                                                   16642 00004102    A
```

**Left strip.** The function-key legend is static art at `CS:0x0690`:

```
\r\n\nF2:\r\nF3:\r\nF4:\r\nF5:\r\nF6:\r\nSL:
```

so the labels occupy rows 2–7 and the changing values are slots 17, 20, 14, 18,
19 and 15 at column 3. The signed-on banner is static art at `CS:0x0737` on rows
14–19: `CABLE V1.7X`, `IBM Internal`, `Use Only`, blank, `Dave Mitchell`,
`ZENO @ WINVMB`.

**Cursor anchoring.** `[BX+0x1909]`/`[BX+0x190B]` is the window anchor (the byte
under the crosshair) as a page:offset pair. `FUN_1000_381A` subtracts the
constant pair at `CS:0x3816`—page `0x0000`, offset `0x0045` (69)—using
`FUN_1000_4075`/`FUN_1000_40C8`/`FUN_1000_4035`, and stores the resulting flat
offset in `[BX+0x190D]`/`[BX+0x190F]`. `FUN_1000_3BA9` then builds one 16-byte
row and `FUN_1000_38E4` advances that read cursor by one byte (wrapping
`0x1000` into the page). Row 0 therefore starts at `anchor − 69`, and the anchor
always lands on data row 4, byte slot 5, screen columns 30–31. This is why the
crosshair can be static art.

**Crosshair.** `FUN_1000_3B58` clears a 66-byte row buffer, calls
`FUN_1000_3BA9`, then writes two characters at row-buffer offsets `0x0E` and
`0x11` (screen columns 29 and 32). A free-running byte `[0x3B57]` is
incremented and masked with 7 for every row; the glyphs are:

| Counter | Top window | Bottom window |
| --- | --- | --- |
| < 4 | space | `║` |
| = 4 | `╔` `╗` | `╚` `╝` |
| > 4 | `║` | space |

Because eight rows are drawn per window, the phase repeats for the second
window. Chromatically the glyphs are ordinary data-row text, not part of the
active bar.

**Missing cells.** When `FUN_1000_3884` reports no byte (before the start or at
or past EOF), `FUN_1000_3BA9` writes `[0x103]` (default `**`) for the two hex
characters and a space in the character column, using the normal attribute.

**Character rendering.** `[0x109]` selects EBCDIC (translation table
`CS:0x035B`); `[0x10A]` selects conceal, which turns bytes below `0x20` into
`'.'` (`0x2E`) after translation. The screen is code page 437, so `0xE9`
appears as `Θ`, `0xFA` as `·`, `0xFF` as a no-break space, and so on. The clone
keeps a CP437 display table in `src/codec.rs`.

**Preview construction.** `FUN_1000_3233` builds the band values and then
assembles a 64-character line in `CS:0x171F` with `FUN_1000_32BF` (`AL` =
destination offset):

| Call | Destination | Source |
| --- | --- | --- |
| `AL=0x00` | 0 | 11 chars from row-buffer offset 5 of the row starting at `anchor−21` |
| `AL=0x0B` | 11 | 16 chars of the row starting at `anchor−5` (the cursor row) |
| `AL=0x1B` | 27 | 16 chars of the row at `anchor+11` |
| `AL=0x2B` | 43 | 16 chars of the row at `anchor+27` |
| `AL=0x3B` | 59 | 5 chars of the row at `anchor+43` |

The result is the 64 bytes from `anchor−16` through `anchor+47`, so the cursor
byte itself is preview character 16. Slot 4 prints it at row 10/11, column 15;
bytes outside the file are blank. (The earlier note that the preview was only
the cursor row's sixteen characters was wrong.)

**Attributes.** At startup `FUN_1000_1B99` copies the two-byte attribute pair
from `CS:0x0106` (colour: normal `0x1E`, active `0x78`) or `CS:0x0104` (mono:
`0x07`, `0x70`) into `[0x5E1]`/`[0x5E2]`. In colour mode the normal screen is
bright yellow on blue and the selection bar is dark grey on light grey.
`FUN_1000_332B`/`FUN_1000_3348` apply the active attribute only to slots 0–4,
13 and 16 of the active context, so:

- the whole title field is barred, but the separator columns 56 and 74 are not;
- only the eight value characters of `Hex`/`Dec` and the 64 preview characters
  (columns 15–78) are barred, not the box borders, the gap at columns 13–14, or
  column 79;
- only the two hex digits of the cursor byte are barred; the ASCII cell and the
  crosshair stay normal.

The highlighted menu command uses the same active attribute for its single
character cell.

**Menu descriptions.** The main loop's highlight handler `CS:0x2272` calls
`CS:0x1D13`, which prints the description of the highlighted command from the
14-entry pointer table at `CS:0x0D18` onto row 23 (`CS:0x3D07`). The default
highlight is index 4 (`Help`, one-based), whose description is the generic
hint, so the initial screen shows the hint. The Help description contains the
Enter keycap as the raw bytes `11 C4 D9`, rendered in CP437 as `◄─┘`.

**Clone mapping.** `src/ui.rs::draw_original` and friends reproduce this exact
80x25 geometry, including the pinned cursor, static crosshair, shared rows 8
and 13, slot-shaped active bars and per-command message line. On terminals
smaller than 80x25 the clone falls back to `draw_compact`.

**Deviation.** The original prints the file argument as typed (up to a 64-byte
path buffer) in the 56-column name field; the clone prints the base name so a
long host path does not crowd out the size fields.

---

## 4. Main menu, command dispatch and keys

### 4.1 Menu text

**Observed** at `CS:0x77C`:

```
CABLE: Compare Dir Edit Help Mode Next Options + - = Quit Search View Write
```

The first-letter shortcut string at `CS:0x7CA` is:

```
CDEHMNO+-=QSVW
```

### 4.2 First-letter key table

**Observed** at `CS:0x15CE`. It is an array of `(word key, word handler)` pairs.
The main loop at `CS:0x19FD` reads a key with `INT 16h` (`CS:0x1CE1`), uppercases
ASCII letters, then walks this table (33 real entries):

| Key | Handler (CS) | Function |
| --- | --- | --- |
| Home / Up / PgUp / Left / Right / End / Down / PgDn | `0x1DED` | move cursor |
| Tab | `0x2AE7` | switch active window |
| Space | `0x2272` | advance highlighted menu command |
| F9 (`0x4300`) | `0x2272` | move highlight backwards |
| F10 (`0x4400`) | `0x2272` | move highlight forwards |
| `+` / `-` / `=` | `0x24D8` | add / subtract / set position |
| Enter | `0x22A4` | execute highlighted command |
| `Q` | `0x1AB4` | quit |
| `V` | `0x22DD` | view |
| `C` | `0x22C3` | compare |
| `N` | `0x2884` | repeat compare/search |
| `S` | `0x261E` | search |
| `M` | `0x26E6` | mode |
| `E` | `0x2485` | edit |
| `H` | `0x29C8` | help |
| `D` | `0x2865` | directory |
| `W` | `0x22E9` | write |
| `O` | `0x22F7` | options |
| F1 (`0x3B00`) | `0x29B5` | help |
| F2 (`0x3C00`) | `0x240C` | toggle respect/ignore case |
| F3 (`0x3D00`) | `0x2427` | toggle forward/backward |
| F4 (`0x3E00`) | `0x2454` | toggle character/HEX input |
| F5 (`0x3F00`) | `0x2415` | toggle conceal/show |
| F6 (`0x4000`) | `0x241E` | toggle ASCII/EBCDIC |

Scroll Lock is absent from this table. The main loop polls the BIOS shift flags
(`CS:0x2EA8`, testing bit `0x10`) from its idle loop (`CS:0x1CE1` →
`CS:0x336F`), which updates `[0x692]` and the `SL:` status live. The shared
movement handler `CS:0x1DED` reads `[0x692]` and moves both windows when it is
non-zero.

**Observed.** `CS:0x2272` compares `AH` with `0x43` (F9 scan code): F9 goes
backwards, Space/F10 go forwards. The highlighted-command index is `[0x1CD5]`.

### 4.3 Enter / menu execution

`CS:0x22A4` takes `[0x1CD5]` (highlighted item), looks up the letter at
`CS:0x7CA`, and calls the handler through the pointer table at `CS:0x7D8`. This
is the second dispatch path: navigate the menu with Space/F9/F10 and press
Enter. The clone implements both the direct first-letter command and the
F9/F10/Space + Enter menu path.

### 4.4 Status and F-key legend

The original status fields are written by `CS:0x2F9D` and friends:

| Flag | Off | On | Field writer |
| --- | --- | --- | --- |
| `[0x108]` | `Respect` | `Ignore` | `CS:0x2F1F` |
| `[0x109]` | `ASCII` | `EBCDIC` | `CS:0x2F08` |
| `[0x10A]` | `Show` | `Conceal` | `CS:0x2F36` |
| `[0x10B]` | `Character` | `HEX` | `CS:0x2EB1` |
| `[0x10C]` | `Forward` | `Backward` | `CS:0x2F4D` |

There is an additional `SL:Active` / `SL:Both` status field (**verified**):
Scroll Lock switches between moving the active window alone and moving both
windows together. `CS:0x2EA8` polls BIOS `INT 16h AH=02h` and tests bit `0x10`
of the shift flags; `CS:0x336F` stores the result in `[0x692]` (0 = `Active`,
1 = `Both`) and redraws the status (`SL:` at `CS:0x6AC`, `Active` at
`CS:0x854`, `Both` at `CS:0x85A`). `CS:0x1DED`, the shared arrow/Home/End/PgUp/
PgDn handler, also moves the other window when `[0x692] == 1`. The original
polls the key state from its idle loop (`CS:0x1CE1` → `CS:0x336F`), so the
field updates as soon as Scroll Lock is pressed, without another keypress.

> **Correction to earlier notes.** `[0x10C]` is the **direction** flag
> (Forward/Backward), as proved by F3 → `CS:0x2427` → `xor [0x10C], 0xFF` and
> the `Forward`/`Backward` status strings. It is not the `Active/Both` flag.

### 4.5 Highlighted-command description

`[0x1D71]` is the one-based highlighted menu index; its initial value is 4, so
`Help` is highlighted on startup. `CS:0x2272` (Space/F10 forward, F9 backward)
changes the index and calls `CS:0x1D13`. That reads a pointer from the 14-word
table at `CS:0x0D18` and prints the corresponding description on row 23 through
`CS:0x3D07` (`AL = 1` line, `AH = 0x17` row). Help's description is the generic
hint, which is why the initial message line reads:

```
Type first letter of command or press ◄─┘ to select hilited command
```

`◄─┘` is the Enter keycap stored as `0x11 0xC4 0xD9` at `CS:0x093A` (see
`CS:0x0914`); the clone writes the three CP437 glyphs directly. Moving the menu
highlight replaces any previous message, matching the original's unconditional
redraw of row 23. The descriptions are static; the Edit and Search entries name
the ASCII variants even when EBCDIC or HEX input is selected.

---

## 5. Options and Mode menus

### 5.1 Options

**Observed** label string at `CS:0xFE3`:

```
OPTIONS: ASCII Back Conceal EBCDIC Fore Help Ignore Quit Respect Show heX/char
```

The handler table is at `CS:0x7F4` (11 word entries). The handlers are cleverly
shared between paired labels and branch on the selected label's first letter:

| Index | Label | Handler | Effect |
| --- | --- | --- | --- |
| 0 | ASCII | `0x245D` | `[0x109] = 0` |
| 1 | Back | `0x2471` | `[0x10C] = 0xFF` |
| 2 | Conceal | `0x2430` | `[0x10A] = 0xFF` |
| 3 | EBCDIC | `0x245D` | `[0x109] = 0xFF` |
| 4 | Fore | `0x2471` | `[0x10C] = 0` |
| 5 | Help | `0x29B5` | open help |
| 6 | Ignore | `0x2442` | `[0x108] = 0xFF` |
| 7 | Quit | — | close menu |
| 8 | Respect | `0x2442` | `[0x108] = 0` |
| 9 | Show | `0x2430` | `[0x10A] = 0` |
| 10 | heX/char | `0x2454` | toggle `[0x10B]` |

The clone's Options modal now uses this exact order and these effects.

### 5.2 Mode

**Observed** label string at `CS:0xF34`:

```
MODE: Attributes Color Help Mono Quit
```

Order: `Attributes`, `Color`, `Help`, `Mono`, `Quit` (indices 0–4). The clone
matches this order and maps `Color`/`Mono` to the classic/mono themes, with
`Attributes` opening the theme chooser.

---

## 6. Compare

### 6.1 Flow

- Command `C` → `CS:0x22C3`.
  - Prints `Comparing ...` (`CS:0xD54`).
  - Copies the direction flag `[0x10C]` into the working compare flag
    `[0x16CB]`.
  - Calls the compare loop at `CS:0x2CFD`.
- The loop (`CS:0x2CFD`–`CS:0x2D39`) calls `CS:0x34D9`.
  - `CS:0x34D9` reads one byte from the active window, switches window
    (`xor [0x16CC], 1`), reads one byte from the other window, and returns the
    two bytes in `AH`/`CH`.
  - The loop compares them at `CS:0x2D37` (`cmp ah, ch`) and continues while
    they are equal.
- On a difference the loop marks both windows (`CS:0x30C2`), places the cursors,
  and returns.
- Outcomes:
  - `No difference found before end of file` (`CS:0x122E`)
  - `Compare interrupted` (`CS:0x1346`)
  - `Error reading file` (`CS:0x11F4`)
- `N` (repeat) → `CS:0x2884`, printing `repeating Compare ...`
  (`CS:0xD79`); if there is no last action, `No compare/search to repeat`
  (`CS:0x1329`).

**Inferred.** `CS:0x34D9` reads each byte at that window's *own* cursor
(`[BX+0x190D/0x190F]`), so the original compares **relative positions** when the
two windows have been scrolled independently. In normal use the cursors are kept
in step. The clone compares **absolute offsets** (see §15).

### 6.2 What `**` means — important correction

The supplied screenshot compares two monolithic `.COM` programs named
`132x25.com` and `132x43.com`. Their visible contents are:

```
B8 55 00 CD 10 CD 20     ; mov ax,0055h; int 10h; int 20h
B8 54 00 CD 10 CD 20     ; mov ax,0054h; int 10h; int 20h
```

These are **7-byte mode-setting stubs**. Every other cell in both panes shows
`**`, because the files end at byte 7.

**Observed in the binary.** The row renderer at `CS:0x3BA9` calls the byte reader
`CS:0x3884` for each of 16 cells. When `CS:0x3884` reports “no byte” (past EOF,
or no open file), the renderer writes the two characters at `[0x103]` (default
`**`) and a space. `CS:0x3884` returns “no byte” when the page/offset exceeds
`[BX+0x1905]`/`[BX+0x1907]`, i.e. **at or past end of file**.

**Conclusion:** `**` means **“no data here” (past EOF)**, not “bytes are equal”.
Equal bytes are displayed normally. The clone was corrected to match this; it
highlights differing bytes in colour as a modern aid, but it never replaces equal
data with `**`.

**Unknown.** The original may also use the `**` path for suppressed positions in
some compare sub-modes (`CS:0x3B19` inspects four per-window marker slots). That
secondary use is not fully characterised.

### 6.3 Length differences

A file that ends earlier produces `**` for the missing cells, while the longer
file shows its byte. When the difference is a pure length mismatch, the reported
offset is the first byte past the shorter file. The clone allows a cursor at
`len` for the shorter pane and reports the mismatch once, so `Next` does not loop
on it.

---

## 7. Search

- Command `S` → `CS:0x261E`.
- Prompt depends on the input/data mode:
  - `SEARCH: Enter ASCII search string` (`CS:0xD90`)
  - `SEARCH: Enter EBCDIC search string` (`CS:0xDB3`)
  - `SEARCH: Enter HEX search string (blanks ignored)` (`CS:0xDD7`)
- Empty input: `Search requires a string` (`CS:0x12B6`).
- Failure: `No match found before end of file` (`CS:0x12D0`).
- Interrupted: `Search terminated` (`CS:0x135B`).
- `N` repeats the last search (`repeating Search ...`, `CS:0xD63`).
- Case handling follows `[0x108]`; EBCDIC follows `[0x109]`; HEX input follows
  `[0x10B]`; direction follows `[0x10C]`.

**Inferred.** Search is a byte-string scan (not a regular expression), run either
forwards or backwards from the cursor. The clone implements exactly that in
`src/compare.rs::find`, including case folding and EBCDIC transcoding.

---

## 8. Edit

- Command `E` → `CS:0x2485`; no open file gives `No file to edit` (`CS:0x1382`).
- Prompts:
  - `EDIT: Enter ASCII characters (Esc to quit)` (`CS:0xF5C`)
  - `EDIT: Enter EBCDIC characters (Esc to quit)` (`CS:0xF88`)
  - `EDIT: Enter HEX digits (Esc to quit)` (`CS:0xFB5`)
- Edits are held in memory. The original keeps 4 KiB pages and writes them back
  on demand; the clone keeps a sparse overlay.
- Write command `W` → `CS:0x22E9` prints the `WRITE:` prompt (`CS:0xFDB`).

**Observed write/confirm flow** (`CS:0x3A00`):

- Prompt text combines `Do you want to write changes to` and `(Y/N)?`
  (`CS:0x10D6`), with the filename in between.
- `N`:
  - If the file was new/read-only: `Changes not saved` (`CS:0x11B3`).
  - Otherwise: `Changes lost` (`CS:0x1140`).
- `Y`: writes with `INT 21h AH=40h`, then reports `<n> bytes written`
  (`CS:0x114E`).
- Errors: `Error writing file` (`CS:0x136E`),
  `File is read-only - can't write` (`CS:0x1393`),
  `No changes to write` (`CS:0x13B4`).

**Clone mapping.** Edits are kept in memory, dirty pages are marked with `*`,
and `W` opens a `WRITE` confirmation. Read-only files can be edited in memory but
writing reports the original error text. Saving uses an atomic temporary file
plus rename, and reopens the handle so a subsequent read sees the new bytes.

---

## 9. View

- Command `V` → `CS:0x22DD`.
- Prompt: `VIEW: Enter [drive:][path\]filespec` (`CS:0xE09`).
- Missing filespec: `View requires a filespec` (`CS:0x11C6`).
- Open failure: `Error opening file` (`CS:0x11E0`).

The clone opens the requested file into the active pane (or the other pane via
`Ctrl+Enter` in the directory browser).

---

## 10. Directory

- Command `D` → `CS:0x2865` / `CS:0x286C`.
- Prompt (`CS:0xEEC`):
  `DIR: Enter [drive:][path\][filespec] (press <key> for current directory)`
- Footer (`CS:0x14DE`):
  `Scroll using <keys>, PgUp, PgDn, Home, End  -  Esc returns to CABLE`
- A listing line template is built at `CS:0x1490`; the sort prompt is
  `Sort by N(ame), E(xtension), D(ate) or S(ize)?` (`CS:0x10A6`).
- Messages: `No matching files found in directory` (`CS:0x1208`),
  `Directory buffer full` (`CS:0x126F`).

**Inferred.** The original builds an in-memory directory of 0x17-byte entries
(`CS:0x36CC` copies 0x17 bytes per entry) and sorts it. The clone implements a
directory modal with name/extension/date/size sorting, path entry, target-pane
selection, and subdirectory navigation.

---

## 11. Help

- `H` → `CS:0x29C8`; `F1` → `CS:0x29B5`.
- The original reads an external `cable.hlp`; errors are
  `Error reading Help file` (`CS:0x1256`) and `Help file too big` (`CS:0x1286`).

**Unknown.** `cable.hlp` was not supplied, so its exact wording cannot be
recovered. The clone's `src/help.rs` reconstructs the help from the embedded
option/command descriptions and strings, and says so in the text.

---

## 12. Quit

- `Q` → `CS:0x1AB4`.
- With unsaved changes the original warns:
  `Changes lost.  Quit without writing changes? (Y/N)` (built from the
  `Changes lost` string at `CS:0x1140`).

The clone shows a confirmation and exits only on `Y`/Enter; `N`/Esc returns.

---

## 13. Embedded message catalogue

The main string block occupies roughly `CS:0x0700`–`CS:0x1520`
(Ghidra `0x10700`–`0x11520`). Selected exact strings:

| CS offset | String |
| --- | --- |
| `0x0737` | `CABLE V1.7X` |
| `0x077C` | `CABLE: Compare Dir Edit Help Mode Next Options + - = Quit Search View Write` |
| `0x0860` | `Compare files starting at current positions` |
| `0x088D` | `Display directory` |
| `0x08A0` | `Edit (in ASCII) file in active window` |
| `0x08C7` | `Edit (in EBCDIC) file in active window` |
| `0x08EF` | `Edit (in HEX) file in active window` |
| `0x0914` | `Type first letter of command or press ◄─┘ to select hilited command` (keycap bytes `11 C4 D9` at `0x093A`) |
| `0x0959` | `Switch between Colour and Mono monitors` |
| `0x0982` | `Repeat Compare or Search` |
| `0x099C` | `Change case, direction, data entry and show options` |
| `0x09D1` | `Move current position forwards` |
| `0x09F1` | `Move current position backwards` |
| `0x0A12` | `Set current position` |
| `0x0A28` | `Quit CABLE, return to DOS` |
| `0x0A43` | `Search for ASCII string` |
| `0x0A5C` | `Search for EBCDIC string` |
| `0x0A76` | `Search for HEX string` |
| `0x0A8D` | `View file in active window` |
| `0x0AA9` | `Write changes to active file back to disk` |
| `0x0AD4` | `Treat data as ASCII` |
| `0x0AE9` | `Make Compare and Search work backwards` |
| `0x0B11` | `Make Compare and Search work forwards` |
| `0x0B38` | `Treat data as EBCDIC` |
| `0x0B4E` | `Display unprintable characters as '.'` |
| `0x0B75` | `Give help about options` |
| `0x0B8E` | `Ignore case differences in search and compare` |
| `0x0BBD` | `Quit to main menu` |
| `0x0BD0` | `Respect case differences in search and compare` |
| `0x0C00` | `Display unprintable characters as they are` |
| `0x0C2C` | `Switch between HEX and character input for search/edit (now Character)` |
| `0x0C74` | `Switch between character and HEX input for search/edit (now HEX)` |
| `0x0CB6` | `Alter screen attributes` |
| `0x0CCF` | `Switch to Colour monitor` |
| `0x0CE9` | `Give help about modes` |
| `0x0D00` | `Switch to Mono monitor` |
| `0x0D54` | `Comparing ...` |
| `0x0D63` | `repeating Search ...` |
| `0x0D79` | `repeating Compare ...` |
| `0x0D90` | `SEARCH: Enter ASCII search string` |
| `0x0DB3` | `SEARCH: Enter EBCDIC search string` |
| `0x0DD7` | `SEARCH: Enter HEX search string (blanks ignored)` |
| `0x0E09` | `VIEW: Enter [drive:][path\]filespec` |
| `0x0E2E` | `+: Enter value to add to current position (if HEX start with H)` |
| `0x0E6F` | `-: Enter value to subtract from current position (if HEX start with H)` |
| `0x0EB7` | `=: Enter new current position (if HEX start with H)` |
| `0x0EEC` | `DIR: Enter [drive:][path\][filespec] (press <key> for current directory)` |
| `0x0F34` | `MODE: Attributes Color Help Mono Quit ` |
| `0x0F5C` | `EDIT: Enter ASCII characters (Esc to quit)` |
| `0x0F88` | `EDIT: Enter EBCDIC characters (Esc to quit)` |
| `0x0FB5` | `EDIT: Enter HEX digits (Esc to quit)` |
| `0x0FDB` | `WRITE:` |
| `0x0FE3` | `OPTIONS: ASCII Back Conceal EBCDIC Fore Help Ignore Quit Respect Show heX/char ` |
| `0x10A6` | `Sort by N(ame), E(xtension), D(ate) or S(ize)?` |
| `0x1140` | `Changes lost` |
| `0x114E` | `Confirm writing of changes to ... bytes written` |
| `0x11C6` | `View requires a filespec` |
| `0x11E0` | `Error opening file` |
| `0x11F4` | `Error reading file` |
| `0x1208` | `No matching files found in directory` |
| `0x122E` | `No difference found before end of file` |
| `0x1256` | `Error reading Help file` |
| `0x126F` | `Directory buffer full` |
| `0x1286` | `Help file too big` |
| `0x1299` | `No Colour Adapter installed` |
| `0x12B6` | `Search requires a string` |
| `0x12D0` | `No match found before end of file` |
| `0x12F3` | `No Mono Adapter installed` |
| `0x130E` | `Video mode must be C or M` |
| `0x1329` | `No compare/search to repeat` |
| `0x1346` | `Compare interrupted` |
| `0x135B` | `Search terminated` |
| `0x136E` | `Error writing file` |
| `0x1382` | `No file to edit` |
| `0x1393` | `File is read-only - can't write` |
| `0x13B4` | `No changes to write` |
| `0x13C9` | `CABLE requires DOS Version 2 or later` |
| `0x13F0` | `$HELP and DIR suppressed (less than 58K free)` |
| `0x141F` | `$DIR suppressed (less than 64K free)` |
| `0x1445` | `$CABLE can't run (less than 47K free)` |
| `0x146C` | `$Press any key to continue` |
| `0x148B` | `Dir: ... Unordered ... of ...` |
| `0x14DE` | `Scroll using ... , PgUp, PgDn, Home, End  -  Esc returns to CABLE` |

---

## 14. Feature map: original → clone

| Original feature | Clone status | Notes |
| --- | --- | --- |
| Two windows, Tab to switch | Implemented | `Tab`/`BackTab` |
| Paged file browsing | Implemented | 64 KiB pages instead of 4 KiB |
| Hex + ASCII/EBCDIC display | Implemented | CP037 for EBCDIC |
| Conceal/show unprintables | Implemented | F5 / Options |
| ASCII/EBCDIC/HEX search | Implemented | F6 / Options |
| Forward/backward search & compare | Implemented | F3 / Options |
| Respect/ignore case | Implemented | F2 / Options |
| Compare and Next/Repeat | Implemented | absolute-offset compare |
| Limited editing (ASCII/EBCDIC/HEX) | Implemented | append at EOF supported |
| Write with Y/N confirmation | Implemented | atomic save |
| Read-only protection | Implemented | POSIX permission based |
| View file | Implemented | directory `Ctrl+Enter` |
| Directory with sort/path | Implemented | N/E/D/S |
| Help | Reconstructed | `cable.hlp` unavailable |
| Mode (Colour/Mono) | Implemented | classic/mono themes |
| Options menu | Implemented | original order |
| `+` / `-` / `=` offsets | Implemented | decimal and `H`-prefixed hex |
| Menu highlight + Enter | Implemented | Space/F9/F10 + Enter |
| Highlighted-command description | Implemented | row 23 from `CS:0x0D18`; Help's entry is the hint |
| Colour/attribute editing | Partial | theme presets, not cell attributes |
| `SL:Active/Both` status | Implemented | Scroll Lock or `F7`; `--both`/`--active` |
| Middle position band | Implemented | cursor offsets in `Hex`/`Dec` boxes |
| 64-byte previews | Implemented | 64 bytes from `cursor−16` |
| Pinned cursor + static crosshair | Implemented | anchor at row 4 / slot 5; 69 bytes ahead |
| Slot-shaped active bars | Implemented | title fields, value cells, preview, cursor pair |
| `cable.hlp` external file | Omitted | help is built in |
| DOS memory/adaptor checks | Not applicable | modern host |
| 132-column/EGA text modes | Not applicable | terminal-sized layout |

---

## 15. Unknowns, inferences and intentional deviations

1. **Relative vs absolute compare (`CS:0x34D9`).** The original compares each
   window at its own cursor. The clone compares absolute file offsets. For
   normally synchronised windows the result is identical; after independent
   scrolling the original semantics are not reproduced.
2. **Secondary use of `**`.** `CS:0x3B19` consults four per-window marker slots;
   whether any compare sub-mode suppresses additional cells is unresolved.
3. **Help text.** `cable.hlp` was not supplied; the clone's help is a
   reconstruction from embedded strings, not the original document.
4. **EBCDIC code page.** The clone uses CP037. The original's translation tables
   live at `CS:0x015B`/`CS:0x025B` (case-fold) and `CS:0x035B` (EBCDIC) and were
   not exhaustively mapped; a different IBM code page could change a few
   characters.
5. **Colour attributes.** The original supports per-cell foreground/background
   and highlight editing (`Alter screen attributes`, `CS:0x1038`). The clone
   offers classic/amber/mono themes instead of raw attribute editing.
6. **Directory internals.** Entry layout (0x17 bytes) and the sort comparators
   were not decompiled in full; the clone reproduces the visible behaviour.
7. **Memory limits.** The original is constrained by DOS conventional memory and
   a 1 MiB address space; the clone is not.
8. **`N` semantics.** The clone steps one byte past the displayed match; the
   original prints `repeating ...` and resumes from the marked position. The
   visible behaviour (advance to the next hit) matches.
9. **Compare colouring.** The clone highlights every differing byte on screen.
   This is a deliberate modern aid; the original appears to rely mainly on the
   cursor and its attribute system.

---

## 16. Evidence index

| Topic | Primary evidence |
| --- | --- |
| Entry / init | Ghidra `FUN_1000_19fd` (`linear 0x119fd`) |
| Main loop | `CS:0x19FD`, key table `CS:0x15CE` |
| Window selection | `CS:0x3393` |
| Page cache | `CS:0x3AA2`, `CS:0x3B19`, `CS:0x38E4` |
| Byte read / EOF | `CS:0x3884`, bounds `[BX+0x1905/0x1907]` |
| Row renderer / `**` | `CS:0x3BA9` |
| Compare loop | `CS:0x2CFD`–`CS:0x2D56` (manual disassembly) |
| Byte-pair compare | `CS:0x34D9` |
| Search | `CS:0x261E` |
| Edit | `CS:0x2485`, prompts `CS:0xF5C/0xF88/0xFB5` |
| Write / confirm | `CS:0x22E9`, `CS:0x3A00` |
| Options | `CS:0x22F7`, table `CS:0x7F4`, labels `CS:0xFE3` |
| Mode | labels `CS:0xF34` |
| Status fields | `CS:0x2F08`, `0x2F1F`, `0x2F36`, `0x2F4D`, `0x2EB1`, `0x336F` |
| Screen geometry / slot tables | context select `CS:0x3393`; tables `CS:0x1867` (top) and `0x1932` (bottom), 21 slots × 3 bytes `[len,row,col]`; printers `CS:0x3D81`, attribute-only `CS:0x3E26` |
| Title row | `FUN_1000_3201` (`CS:0x3201`), sources `[BX+0x18A6]` / `[BX+0x18EB]` / `[BX+0x18E6]`, blank template `CS:0x0640` |
| Left legend / banner | static art `CS:0x0690` (legend, `SL:` at `CS:0x06AC`) and `CS:0x0737` (`CABLE V1.7X` banner) |
| Cursor anchoring | anchor `[BX+0x1909]`, `FUN_1000_381A` (`CS:0x381A`), constant pair `(0, 0x45)` at `CS:0x3816`, flat offset `[BX+0x190D]`/`[0x190F]`, row builder `FUN_1000_3BA9` (`CS:0x3BA9`) |
| Crosshair | `FUN_1000_3B58` (`CS:0x3B58`), glyphs at row-buffer offsets `0x0E`/`0x11`, phase counter `[0x3B57]` |
| Position band / boxes | band art `CS:0x06B1`–`0x0734` (`Hex`/`Dec` boxes and guides) inside the layout string `CS:0x0690`, loaded at `CS:0x1CB6`, `0x1CF6`, `0x201C` and `0x2912`; dynamic values/previews `CS:0x3233`/`0x32BF` |
| 64-byte previews | `FUN_1000_3233` (`CS:0x3233`) and `FUN_1000_32BF` (`CS:0x32BF`), source rows `CS:0x185B`/`CS:0x1856`, line buffer `CS:0x171F` |
| Attributes / active bars | start-up pair `CS:0x1B99` from `CS:0x0106` (`1E 78`, colour) or `CS:0x0104` (`07 70`, mono) into `[0x5E1]`/`[0x5E2]`; `FUN_1000_332B`/`FUN_1000_3348`, slots 0–4, 13, 16 |
| Menu descriptions | pointer table `CS:0x0D18`, printer `CS:0x1D13` → `CS:0x3D07` row 23, highlight handler `CS:0x2272`, index `[0x1D71]`, Help text `CS:0x0914` |
| Directory | `CS:0x286C`, prompt `CS:0xEEC`, footer `CS:0x14DE` |

Decompiled output lives in `../rea-decompiled/functions/`. The two long manual
disassembly dumps used during analysis are `../disasm_handlers.txt` and
`../disasm_compare_search.txt`.
