// Cell buffer, styles and ANSI rendering.

package main

import (
	"fmt"
	"strings"
)

// Color is one SGR parameter list for a foreground or background colour.
// The empty string leaves the terminal default in place.
type Color string

func fg256(n int) Color { return Color(fmt.Sprintf("38;5;%d", n)) }
func bg256(n int) Color { return Color(fmt.Sprintf("48;5;%d", n)) }

func fgRGB(r, g, b int) Color { return Color(fmt.Sprintf("38;2;%d;%d;%d", r, g, b)) }
func bgRGB(r, g, b int) Color { return Color(fmt.Sprintf("48;2;%d;%d;%d", r, g, b)) }

// Style is the appearance of one screen cell.
type Style struct {
	FG   Color
	BG   Color
	Bold bool
}

func (s Style) withBold(bold bool) Style {
	s.Bold = bold
	return s
}

func (s Style) sgr() string {
	var b strings.Builder
	b.WriteString("\x1b[0")
	if s.Bold {
		b.WriteString(";1")
	}
	if s.FG != "" {
		b.WriteByte(';')
		b.WriteString(string(s.FG))
	}
	if s.BG != "" {
		b.WriteByte(';')
		b.WriteString(string(s.BG))
	}
	b.WriteByte('m')
	return b.String()
}

// Rect is a screen rectangle.
type Rect struct {
	X, Y, W, H int
}

// Cell is one character cell.
type Cell struct {
	Ch    rune
	Style Style
}

// Screen is an off-screen cell buffer.  Rendering rewrites the whole buffer;
// terminals are fast enough for an 80x25 text-mode clone.
type Screen struct {
	W, H  int
	Cells []Cell
}

func newScreen(w, h int, base Style) *Screen {
	if w < 0 {
		w = 0
	}
	if h < 0 {
		h = 0
	}
	cells := make([]Cell, w*h)
	for i := range cells {
		cells[i] = Cell{Ch: ' ', Style: base}
	}
	return &Screen{W: w, H: h, Cells: cells}
}

func (s *Screen) Set(x, y int, ch rune, st Style) {
	if x < 0 || y < 0 || x >= s.W || y >= s.H {
		return
	}
	s.Cells[y*s.W+x] = Cell{Ch: ch, Style: st}
}

func (s *Screen) Put(x, y int, text string, st Style) int {
	for _, ch := range text {
		if x >= s.W {
			break
		}
		s.Set(x, y, ch, st)
		x++
	}
	return x
}

func (s *Screen) PutLimit(x, y int, text string, st Style, limit int) int {
	n := 0
	for _, ch := range text {
		if n >= limit || x >= s.W {
			break
		}
		s.Set(x, y, ch, st)
		x++
		n++
	}
	return x
}

func (s *Screen) Restyle(rect Rect, st Style) {
	for y := rect.Y; y < rect.Y+rect.H; y++ {
		for x := rect.X; x < rect.X+rect.W; x++ {
			if x < 0 || y < 0 || x >= s.W || y >= s.H {
				continue
			}
			s.Cells[y*s.W+x].Style = st
		}
	}
}

func (s *Screen) Fill(rect Rect, ch rune, st Style) {
	for y := rect.Y; y < rect.Y+rect.H; y++ {
		for x := rect.X; x < rect.X+rect.W; x++ {
			s.Set(x, y, ch, st)
		}
	}
}

// Flush writes the buffer to the terminal.  Every row is addressed
// absolutely, so the display does not depend on cursor wrapping.
func (s *Screen) Flush(b *strings.Builder) {
	b.WriteString("\x1b[H")
	last := Style{FG: Color("\x00")} // sentinel that no real style equals
	for y := 0; y < s.H; y++ {
		fmt.Fprintf(b, "\x1b[%d;1H", y+1)
		for x := 0; x < s.W; x++ {
			cell := s.Cells[y*s.W+x]
			if cell.Style != last {
				b.WriteString(cell.Style.sgr())
				last = cell.Style
			}
			b.WriteRune(cell.Ch)
		}
	}
	b.WriteString("\x1b[0m")
}

func runeLen(s string) int {
	return len([]rune(s))
}

func padRight(s string, width int) string {
	n := runeLen(s)
	if n >= width {
		return s
	}
	return s + strings.Repeat(" ", width-n)
}

func padLeft(s string, width int) string {
	n := runeLen(s)
	if n >= width {
		return s
	}
	return strings.Repeat(" ", width-n) + s
}

func truncateRunes(s string, width int) string {
	if width <= 0 {
		return ""
	}
	r := []rune(s)
	if len(r) <= width {
		return s
	}
	return string(r[:width])
}

// wrapText breaks text into lines of at most width runes, preferring word
// boundaries.
func wrapText(text string, width int) []string {
	if width <= 0 {
		return []string{text}
	}
	words := strings.Fields(text)
	if len(words) == 0 {
		return []string{""}
	}
	lines := []string{}
	line := ""
	for _, word := range words {
		if line == "" {
			line = word
			continue
		}
		if runeLen(line)+1+runeLen(word) <= width {
			line += " " + word
		} else {
			lines = append(lines, line)
			line = word
		}
	}
	lines = append(lines, line)
	return lines
}
