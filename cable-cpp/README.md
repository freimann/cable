# cable-cpp

A C++20 port of the Rust `cable-rs` project: a cross-platform reimplementation
of the DOS program **CABLE V1.7X — Compare And Browse with Limited Editing**.

The port keeps the same paged 64 KiB file I/O, edit overlay, codecs, directory
browser, key handling and 80x25/adaptive UI as the Rust and Go versions.  It
has no required external libraries; a real terminal is needed for interactive
use.

## Build

```sh
make            # build ./cable for the host platform
make test       # smoke test: option parsing plus all benchmark phases
```

or with CMake:

```sh
cmake -S . -B build -G Ninja
cmake --build build
```

On Windows build with CMake and MSVC (the POSIX terminal translation unit is
empty there; `term_windows.cpp` supplies the console implementation).

### Cross-built binaries

`dist/` contains the binaries built for this repository (git-ignored, so
rebuild them after a fresh clone):

| File | Platform |
| --- | --- |
| `cable-cpp-darwin-arm64` | macOS 11+, Apple silicon |
| `cable-cpp-darwin-amd64` | macOS 11+, Intel |
| `cable-cpp-linux-amd64` | Linux x86-64, statically linked |
| `cable-cpp-linux-arm64` | Linux arm64, statically linked |
| `cable-cpp-windows-amd64.exe` | Windows 10/11 console (x86-64) |

The Darwin and Windows builds use the host clang and mingw-w64 toolchains.  The
Linux builds were produced in an Ubuntu 22.04 container:

```sh
# from the cable-cpp directory
clang++ -std=c++20 -O3 -DNDEBUG -o dist/cable-cpp-darwin-arm64 \
  core.cpp ui.cpp app.cpp key.cpp term_posix.cpp term_windows.cpp \
  main.cpp bench.cpp

x86_64-w64-mingw32-g++ -std=c++20 -O3 -DNDEBUG -static \
  -o dist/cable-cpp-windows-amd64.exe \
  core.cpp ui.cpp app.cpp key.cpp term_posix.cpp term_windows.cpp \
  main.cpp bench.cpp
```

For Linux, build inside Ubuntu 22.04 so the result does not depend on the host
toolchain:

```sh
docker run --rm --platform linux/amd64 -v "$PWD/..":/w -w /w/cable-cpp \
  ubuntu:22.04 sh -c 'apt-get update -qq && apt-get install -y g++ &&
  g++ -std=c++20 -O3 -DNDEBUG -static -o dist/cable-cpp-linux-amd64 \
  core.cpp ui.cpp app.cpp key.cpp term_posix.cpp term_windows.cpp \
  main.cpp bench.cpp'
```

Drop `--platform linux/amd64` for the arm64 build.

## Usage

```sh
./cable [OPTIONS] [FILE1 [FILE2]]
```

With no files the directory browser opens.  `./cable --help` lists the same
options as the Rust and Go ports, including `--ebcdic`, `--hex`, `--both`,
`--amber` and the `--x86`/`--x64`/`--arm`/`--thumb`/`--arm64` side views.

The screen matches the original 80x25 layout exactly.  Wider terminals get the
same adaptive layout as the Rust and Go ports.

### Disassembly note

The Rust version uses the `yaxpeax` decoders and the Go version uses
`golang.org/x/arch`.  This C++ port has no decoder dependency: the right-hand
column still cycles through x86, x64, ARM, Thumb and ARM64 with `F8`, but each
native instruction slot is shown as a raw assembler directive
(`.byte`, `.hword` or `.word`) rather than a mnemonic.  This keeps the view and
keyboard behaviour stable without adding an external dependency.

## Benchmark

The C++ binary supports the same non-interactive I/O benchmark as the Rust and
Go versions:

```sh
./cable --bench FILE1 [FILE2]
```

It prints CSV rows for the `open`, `scan`, `search`, `compare` and `save`
phases.  The benchmark data generator and comparison runner live in
[`../bench`](../bench):

```sh
python3 ../bench/gen_data.py
python3 ../bench/run.py --runs 3
```

`run.py` builds a median across runs, checks that all implementations report
the same per-phase checksums, and prints a side-by-side table.
