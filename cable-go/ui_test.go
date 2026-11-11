package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func bytesRepeat(b byte, n int) []byte {
	out := make([]byte, n)
	for i := range out {
		out[i] = b
	}
	return out
}

func lineColumns(bytes int) int {
	groups := bytes / 16
	return 9 + (32*groups + groups - 1) + 2 + (16*groups + groups - 1)
}

func TestAdaptiveLinesAreWholeSixteenByteGroups(t *testing.T) {
	for width := 59; width <= 400; width++ {
		bytes := adaptiveBytesPerRow(width)
		if bytes%16 != 0 {
			t.Fatalf("width %d: bytes = %d, not a multiple of 16", width, bytes)
		}
		if bytes < 16 {
			t.Fatalf("width %d: bytes = %d", width, bytes)
		}
		if got := lineColumns(bytes); got > width {
			t.Fatalf("width %d needs %d columns for %d bytes", width, got, bytes)
		}
	}
	if got := adaptiveBytesPerRow(80); got != 16 {
		t.Fatalf("width 80 = %d", got)
	}
	if got := adaptiveBytesPerRow(120); got != 32 {
		t.Fatalf("width 120 = %d", got)
	}
	if got := adaptiveBytesPerRow(160); got != 48 {
		t.Fatalf("width 160 = %d", got)
	}
}

func TestPreviewCursorSitsBetweenTheMiddleHexGroups(t *testing.T) {
	if got := adaptiveBytesPerRow(240); got != 64 {
		t.Fatalf("width 240 = %d", got)
	}
	if got := adaptiveHexCenter(64, 9); got != 74 {
		t.Fatalf("center at 240 columns = %d", got)
	}
	if got := adaptiveBytesPerRow(132); got != 32 {
		t.Fatalf("width 132 = %d", got)
	}
	if got := adaptiveHexCenter(32, 9); got != 41 {
		t.Fatalf("center at 132 columns = %d", got)
	}
	if got := adaptiveBytesPerRow(160); got != 48 {
		t.Fatalf("width 160 = %d", got)
	}
	if got := adaptiveHexCenter(48, 9); got != 58 {
		t.Fatalf("center at 160 columns = %d", got)
	}
}

func TestDisasmWindowCentresTheCursorInstruction(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ui.bin")
	if err := os.WriteFile(path, bytesRepeat(0x90, 64), 0666); err != nil {
		t.Fatal(err)
	}
	options := defaultOptions()
	cpu := CpuX86
	options.Disasm = &cpu
	app, err := newApp([]string{path}, options)
	if err != nil {
		t.Fatal(err)
	}
	defer app.Panes[0].File.file.Close()
	app.Panes[0].Cursor = 4

	rows := 9
	window := disasmWindow(app, 0, rows, 40)
	if len(window) != rows {
		t.Fatalf("window has %d rows", len(window))
	}
	for _, row := range window {
		if len(row) != 40 {
			t.Fatalf("row has %d cells", len(row))
		}
	}
	for row, cells := range window {
		var b strings.Builder
		for _, cell := range cells {
			b.WriteRune(cell.Ch)
		}
		line := b.String()
		prefix := fmt.Sprintf("%08X  nop", row)
		if !strings.HasPrefix(line, prefix) {
			t.Fatalf("row %d = %q, want prefix %q", row, line, prefix)
		}
	}
	if window[0][0].Style.Bold {
		t.Fatal("first row should not be bold")
	}
	if !window[rows/2][0].Style.Bold {
		t.Fatal("middle row should be bold")
	}
}
