#!/usr/bin/env python3
"""Run the non-interactive I/O benchmark for Rust, Go and C++.

Each implementation prints CSV rows with phase, milliseconds, bytes and a
checksum.  This script runs every implementation several times, takes the
median time for each phase, checks that all implementations produced the same
checksums, and prints a comparison table.
"""

from __future__ import annotations

import argparse
import csv
import io
import statistics
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PHASES = ("open", "scan", "search", "compare", "save")


def default_binaries() -> dict[str, Path]:
    return {
        "rust": ROOT / "target" / "release" / "cable-rs",
        "go": ROOT / "cable-go" / "cable",
        "cpp": ROOT / "cable-cpp" / "cable",
    }


def run_once(binary: Path, file1: Path, file2: Path) -> dict[str, tuple[float, int, int]]:
    proc = subprocess.run(
        [str(binary), "--bench", str(file1), str(file2)],
        check=True,
        capture_output=True,
        text=True,
    )
    reader = csv.reader(io.StringIO(proc.stdout))
    header = next(reader, None)
    if header is None or header[:1] != ["phase"]:
        raise SystemExit(f"{binary}: unexpected benchmark output:\n{proc.stdout}")
    rows = {}
    for row in reader:
        if len(row) != 4:
            continue
        rows[row[0]] = (float(row[1]), int(row[2]), int(row[3]))
    missing = [phase for phase in PHASES if phase not in rows]
    if missing:
        raise SystemExit(f"{binary}: missing benchmark phases: {', '.join(missing)}")
    return rows


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=int, default=3, help="timed repetitions (default 3)")
    parser.add_argument("--warmups", type=int, default=1, help="untimed warm-up runs (default 1)")
    parser.add_argument("--data", type=Path, default=ROOT / "bench" / "data")
    parser.add_argument(
        "--binaries",
        nargs="*",
        default=[],
        metavar="NAME=PATH",
        help="override binaries, e.g. rust=target/release/cable-rs",
    )
    args = parser.parse_args()

    file1 = args.data / "file1.bin"
    file2 = args.data / "file2.bin"
    if not file1.exists() or not file2.exists():
        raise SystemExit(f"benchmark data not found in {args.data}; run bench/gen_data.py first")

    binaries = default_binaries()
    if args.binaries:
        binaries = {}
        for item in args.binaries:
            name, sep, path = item.partition("=")
            if not sep:
                raise SystemExit(f"expected NAME=PATH, got {item!r}")
            binaries[name] = Path(path)

    active = {name: path for name, path in binaries.items() if path.exists()}
    skipped = [name for name in binaries if name not in active]
    if not active:
        raise SystemExit("no benchmark binaries found; build them first")
    if skipped:
        print(f"skipping missing binaries: {', '.join(skipped)}", file=sys.stderr)

    times: dict[str, dict[str, list[float]]] = {
        name: {phase: [] for phase in PHASES} for name in active
    }
    results: dict[str, dict[str, tuple[int, int]]] = {}

    for name, binary in active.items():
        for _ in range(max(args.warmups, 0)):
            run_once(binary, file1, file2)
        for _ in range(max(args.runs, 1)):
            rows = run_once(binary, file1, file2)
            results[name] = {phase: (rows[phase][1], rows[phase][2]) for phase in PHASES}
            for phase in PHASES:
                times[name][phase].append(rows[phase][0])

    names = list(active)
    checksum_ok = True
    for phase in PHASES:
        reference = results[names[0]][phase][1]
        for name in names[1:]:
            if results[name][phase][1] != reference:
                checksum_ok = False
                print(
                    f"warning: checksum mismatch in {phase}: "
                    f"{names[0]}={reference} {name}={results[name][phase][1]}",
                    file=sys.stderr,
                )

    widths = {name: max(len(name), len("median ms")) for name in names}
    header = f"{'phase':8} {'bytes':>12}  " + "  ".join(
        f"{name + ' ms':>{widths[name]}}" for name in names
    )
    print(header)
    print("-" * len(header))
    totals = {name: 0.0 for name in names}
    for phase in PHASES:
        byte_count = results[names[0]][phase][0]
        cells = []
        for name in names:
            median = statistics.median(times[name][phase])
            totals[name] += median
            cells.append(f"{median:>{widths[name]}.3f}")
        print(f"{phase:8} {byte_count:>12}  " + "  ".join(cells))
    print("-" * len(header))
    cells = [f"{totals[name]:>{widths[name]}.3f}" for name in names]
    print(f"{'total':8} {'':>12}  " + "  ".join(cells))
    if not checksum_ok:
        raise SystemExit("checksum verification failed")


if __name__ == "__main__":
    main()
