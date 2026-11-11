#!/usr/bin/env python3
"""Generate deterministic benchmark files for the Rust, Go and C++ ports.

The files are non-zero and large enough to exercise real disk I/O.  A search
needle is placed near the end of file 1, and file 2 differs from file 1 near
the end so compare and search both scan almost the whole file.
"""

from __future__ import annotations

import argparse
from pathlib import Path

MARKER = b"CABLE-NEEDLE-1234"


def generate(out_dir: Path, size_mib: int) -> tuple[Path, Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    size = size_mib * 1024 * 1024
    if size < 8192:
        raise SystemExit("size must be at least 8192 bytes")
    if size % 65536 != 0:
        raise SystemExit("size must be a multiple of 64 KiB")

    block = bytes(((i * 131 + 17) % 255) + 1 for i in range(65536))
    file1 = out_dir / "file1.bin"
    with file1.open("wb") as f:
        remaining = size
        while remaining:
            chunk = min(remaining, len(block))
            f.write(block[:chunk])
            remaining -= chunk
        f.seek(size - 4096)
        f.write(MARKER)

    file2 = out_dir / "file2.bin"
    with file1.open("rb") as src, file2.open("wb") as dst:
        while True:
            chunk = src.read(1024 * 1024)
            if not chunk:
                break
            dst.write(chunk)
    with file2.open("r+b") as f:
        f.seek(size - 1024)
        original = f.read(1)
        f.seek(size - 1024)
        f.write(bytes([original[0] ^ 0xFF]))

    return file1, file2


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--outdir", type=Path, default=Path(__file__).parent / "data")
    parser.add_argument("--size-mib", type=int, default=128)
    args = parser.parse_args()

    file1, file2 = generate(args.outdir, args.size_mib)
    print(f"wrote {file1} ({file1.stat().st_size} bytes)")
    print(f"wrote {file2} ({file2.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
