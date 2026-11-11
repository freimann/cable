package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestScrollingUsesWideLines(t *testing.T) {
	path := filepath.Join(t.TempDir(), "wide.bin")
	if err := os.WriteFile(path, make([]byte, 4096), 0666); err != nil {
		t.Fatal(err)
	}
	pane, err := openPane(path)
	if err != nil {
		t.Fatal(err)
	}
	defer pane.File.file.Close()

	pane.Cursor = 1000
	pane.ensureVisible(8, 32)
	if pane.Top != 768 {
		t.Fatalf("top = %d, want 768", pane.Top)
	}

	pane.Cursor = 900
	pane.ensureVisible(8, 32)
	if pane.Top != 768 {
		t.Fatalf("top after small move = %d, want 768", pane.Top)
	}

	pane.Cursor = 100
	pane.ensureVisible(8, 32)
	if pane.Top != 96 {
		t.Fatalf("top after jump = %d, want 96", pane.Top)
	}
}

func TestScrollLockAcceptsKeyboardAliases(t *testing.T) {
	cases := []struct {
		key  Key
		want bool
	}{
		{Key{Code: keyScrollLock}, true},
		{Key{Code: keyF7}, true},
		{Key{Code: keyRune, Rune: '\f'}, true},
		{Key{Code: keyRune, Rune: 'l', Ctrl: true}, true},
		{Key{Code: keyRune, Rune: 'B', Ctrl: true}, true},
		{Key{Code: keyRune, Rune: 'l'}, false},
	}
	for _, c := range cases {
		if got := isScrollLockToggle(c.key); got != c.want {
			t.Fatalf("isScrollLockToggle(%+v) = %v, want %v", c.key, got, c.want)
		}
	}
}

func TestSideViewCyclesBackToCharacters(t *testing.T) {
	options := defaultOptions()
	if got := options.sideView(); got != "ASCII" {
		t.Fatalf("initial side view = %q", got)
	}
	want := []string{"x86", "x64", "ARM", "Thumb", "ARM64"}
	for _, name := range want {
		options.cycleSideView()
		if got := options.sideView(); got != name {
			t.Fatalf("side view = %q, want %q", got, name)
		}
	}
	options.cycleSideView()
	if options.Disasm != nil {
		t.Fatal("disasm should be nil after a full cycle")
	}
	if got := options.sideView(); got != "ASCII" {
		t.Fatalf("final side view = %q", got)
	}
}

func TestDisassemblyViewBlocksEditing(t *testing.T) {
	path := filepath.Join(t.TempDir(), "disasm.bin")
	if err := os.WriteFile(path, make([]byte, 64), 0666); err != nil {
		t.Fatal(err)
	}
	options := defaultOptions()
	cpu := CpuX86
	options.Disasm = &cpu
	app, err := newApp([]string{path}, options)
	if err != nil {
		t.Fatal(err)
	}
	app.startEdit()
	if app.Edit != nil {
		t.Fatal("editing should be disabled in a disassembly view")
	}
	if !strings.Contains(app.Message, "disassembly") {
		t.Fatalf("message = %q", app.Message)
	}
}
