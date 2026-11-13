#!/usr/bin/env bash
# Assemble a release archive from binaries that have already been built.
#
#   scripts/package-release.sh macos
#   scripts/package-release.sh windows-x64
#   scripts/package-release.sh windows-x86
#   scripts/package-release.sh linux-x64
#
# VERSION overrides the version taken from Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

version=${VERSION:-$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -n1)}
platform=${1:-}
repo=$PWD
out=dist
mkdir -p "$out"

write_readme() {
    local file=$1 platform_name=$2 extra=$3
    cat > "$file" <<EOF
CABLE $version - $platform_name

Two programs are included:

  cable-gui   desktop application, with a native window
  cable       terminal application

Usage:

  cable-gui [OPTIONS] [FILE1 [FILE2]]
  cable     [OPTIONS] [FILE1 [FILE2]]

Run either program with --help for the options.
$extra
The desktop application needs Metal on macOS, or OpenGL 3.3 on Windows and
Linux.  Files can also be opened by dragging them onto the window.
EOF
}

case "$platform" in
macos)
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    stage="$work/stage"
    app="$stage/CABLE.app"
    mkdir -p "$app/Contents/MacOS"

    lipo -create -output "$app/Contents/MacOS/cable-gui" \
        target/aarch64-apple-darwin/release/cable-gui \
        target/x86_64-apple-darwin/release/cable-gui
    lipo -create -output "$stage/cable" \
        target/aarch64-apple-darwin/release/cable-rs \
        target/x86_64-apple-darwin/release/cable-rs
    sed "s/__VERSION__/$version/g" packaging/macos/Info.plist \
        > "$app/Contents/Info.plist"
    codesign --force --sign - "$app"

    write_readme "$stage/README.txt" "macOS (universal)" \
"
Drag CABLE.app to Applications to install the desktop application.  The
'universal' build runs on Apple Silicon (macOS 11 or newer) and on Intel Macs
(macOS 10.12 or newer).
"
    archive=cable-v$version-macos-universal.zip
    ditto -c -k --norsrc --noextattr "$stage" "$out/$archive"
    ;;

windows-x64)
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    stage="$work/stage"
    mkdir -p "$stage"
    cp target/x86_64-pc-windows-gnu/release/cable-gui.exe "$stage/cable-gui.exe"
    cp target/x86_64-pc-windows-gnu/release/cable-rs.exe "$stage/cable.exe"
    archive=cable-v$version-windows-x86_64.zip

    write_readme "$stage/README.txt" "Windows (64-bit)" \
"
Requires Windows 10 or newer.
"
    (cd "$stage" && zip -qr "$repo/$out/$archive" .)
    ;;

windows-x86)
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    stage="$work/stage"
    mkdir -p "$stage"
    cp target/i686-pc-windows-gnullvm/release/cable-gui.exe "$stage/cable-gui.exe"
    cp target/i686-pc-windows-gnullvm/release/cable-rs.exe "$stage/cable.exe"
    cp cable-cpp/dist/cable-cpp-windows-x86.exe "$stage/cable-cpp-windows-x86.exe"
    archive=cable-v$version-windows-x86.zip

    write_readme "$stage/README.txt" "Windows (32-bit)" \
"
The Rust programs (cable-gui.exe and cable.exe) require Windows 10 or newer
(32-bit).  For Windows 7 or 8.1, run cable-cpp-windows-x86.exe instead: it is
the console version from the C++ port and supports 32-bit Windows 7 and newer.
"
    (cd "$stage" && zip -qr "$repo/$out/$archive" .)
    ;;

linux-x64)
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    stage="$work/stage"
    mkdir -p "$stage"
    cp target/x86_64-unknown-linux-gnu/release/cable-gui "$stage/cable-gui"
    cp target/x86_64-unknown-linux-gnu/release/cable-rs "$stage/cable"
    archive=cable-v$version-linux-x86_64.tar.gz

    write_readme "$stage/README.txt" "Linux (x86-64)" \
"
Built against glibc 2.17, so it runs on most current distributions.  X11 or
Wayland is needed for the desktop application.
"
    tar -czf "$out/$archive" \
        -C "$stage" cable-gui cable README.txt
    ;;

*)
    echo "usage: $0 macos|windows-x64|windows-x86|linux-x64" >&2
    exit 2
    ;;
esac

echo "wrote $out/$archive"
