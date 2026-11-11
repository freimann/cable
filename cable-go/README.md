# cable-go

A complete Go port of the Rust `cable-rs` project: a cross-platform
reimplementation of the DOS program **CABLE V1.7X — Compare And Browse with
Limited Editing**.

The original Rust sources remain in the repository root (`src/`), including
the dependency on `crossterm`/`ratatui`, `yaxpeax` disassemblers and the
recovered CAB 1.40 help text.  This directory is a self-contained Go module
(`module cable`) that reproduces the same terminal application with a small
zero-dependency ANSI renderer.

## Build

```sh
make            # build ./cable for the host platform
make test       # run the ported unit tests
make dist       # cross-compile into dist/ for all platforms
```

`make dist` writes:

```
dist/cable-darwin-amd64
dist/cable-darwin-arm64
dist/cable-linux-amd64
dist/cable-linux-arm64
dist/cable-windows-amd64.exe
dist/cable-windows-arm64.exe
```

All binaries are built with `CGO_ENABLED=0`.

## Usage

```sh
./cable [OPTIONS] [FILE1 [FILE2]]
```

With no files, the directory browser opens.  Run `./cable --help` for the
option list.  The defaults match the original: ASCII, respect case, forward
search, conceal unprintables, colour (blue/yellow) theme and only the active
window moves.

The screen is laid out exactly like the 80x25 original.  Wider terminals get
an adaptive layout with the same information; small terminals use a compact
footer.

### Keys

| Key | Action |
| --- | --- |
| `F1` / `H` | Help |
| `F2` | Case: respect / ignore |
| `F3` | Direction: forward / backward |
| `F4` | Character / HEX input |
| `F5` | Conceal / show unprintables |
| `F6` | ASCII / EBCDIC (CP037) |
| `F8` | Cycle right-hand column: characters, x86, x64, ARM, Thumb, ARM64 |
| `F9` / `F10` / Space | Previous / next menu command |
| `F7`, `Ctrl+L`, `Ctrl+B` | Scroll Lock: active window / both windows |
| `Tab` | Switch active window |
| Arrow keys, PgUp/PgDn, Home/End | Move the cursor |
| `C`, `N` | Compare, repeat last compare/search |
| `S` | Search |
| `E` | Edit (disabled in disassembly views) |
| `D` | Directory browser |
| `M` | Mode menu |
| `O` | Options menu |
| `+`, `-`, `=` | Add, subtract or set the current position |
| `V` | View a file |
| `W` | Write changes |
| `Q` | Quit |

Enter executes the highlighted menu command.  Menu letters also work directly.

## Implementation notes

* `style.go` is a cell buffer plus ANSI renderer; the whole screen is redrawn
  after every key, which keeps resize handling and modal redraws simple.
* `term_unix.go`, `term_darwin.go`, `term_linux.go` and `term_windows.go`
  provide raw mode and terminal size per platform (`syscall` on Unix, Win32
  console calls through `kernel32` on Windows).
* `buffer.go` keeps a paged view of the file with an edit overlay; writes go to
  a temporary file that is renamed over the original.
* `codec.go` and `cptables.go` implement IBM CP037 (EBCDIC) and CP437 display
  glyphs.
* `help_text.go` is generated from the recovered CAB 1.40 help pages, so the
  in-app help contains the same lines as the Rust build.
* `disasm.go` uses `golang.org/x/arch` for x86/x64/ARM/ARM64 and
  `github.com/loov/disasm/thumbasm` for Thumb/Thumb-2.  Instruction text is
  normalised towards the reference output, but the reference uses `yaxpeax`,
  so uncommon encodings (notably some DSP Thumb instructions and a few
  encodings `x/arch` rejects as invalid) may render differently or as
  `.byte`/`.hword`/`.word` directives.

## Differences from the Rust version

The interactive behaviour, screen layout, messages, defaults and key handling
are ported directly.  Terminal output is a full redraw with absolute cursor
positioning instead of ratatui's incremental diff.  Disassembly is provided by
different decoder libraries, as described above.  Everything else in the
original `src/` tree is left untouched.
