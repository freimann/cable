# bench

Non-interactive I/O benchmark shared by the Rust, Go and C++ implementations.

```sh
python3 bench/gen_data.py        # writes data/file1.bin and data/file2.bin
python3 bench/run.py --runs 3    # runs every built binary and compares
```

`gen_data.py` writes two deterministic 128 MiB files.  File 1 contains
`CABLE-NEEDLE-1234` near the end; file 2 differs from file 1 near the end, so
`search` and `compare` both have to read most of the file.  `data/` is ignored
by git.

`run.py` runs each binary once as a warm-up (configurable with `--warmups`) and
then several timed runs, reports the median per phase, and fails if the
implementations disagree about any phase checksum.  Use `--binaries` to point
at other builds:

```sh
python3 bench/run.py --runs 5 \
  --binaries rust=target/release/cable-rs \
             go=cable-go/cable \
             cpp=cable-cpp/cable
```

Every binary exposes the same interface:

```sh
cable --bench FILE1 [FILE2]
```

and prints `phase,milliseconds,bytes,checksum` rows for `open`, `scan`,
`search`, `compare` and `save`.  `save` copies the input to a temporary file,
edits one byte and writes the copy back, so the source files are left alone.
