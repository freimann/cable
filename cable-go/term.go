// Terminal setup and screen output shared by all platforms.

package main

import (
	"os"
	"strings"
)

type terminalState struct {
	restore func()
}

func startTerminal() (*terminalState, error) {
	restore, err := makeRaw()
	if err != nil {
		return nil, err
	}
	st := &terminalState{restore: restore}
	os.Stdout.WriteString("\x1b[?1049h\x1b[?7l\x1b[?25l\x1b[2J")
	return st, nil
}

func (t *terminalState) Close() {
	os.Stdout.WriteString("\x1b[?7h\x1b[?25h\x1b[?1049l")
	if t.restore != nil {
		t.restore()
	}
}

func redraw(app *App) error {
	w, h, err := terminalSize()
	if err != nil {
		return err
	}
	if w < 1 || h < 1 {
		return nil
	}
	s := newScreen(w, h, baseStyle(app.Options.Theme))
	drawUI(s, app)
	var b strings.Builder
	s.Flush(&b)
	_, err = os.Stdout.WriteString(b.String())
	return err
}
