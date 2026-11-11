// Terminal rendering.

package main

import (
	"fmt"
	"path/filepath"
	"strings"
)

const footerRows = 4

// Width of the preview line built by CS:0x3233 in CS:0x171F.
const previewLen = 64

// The preview starts sixteen bytes before the cursor, so the active byte is
// the seventeenth character of the line.
const previewBefore = 16

// Fixed geometry of the original 80x25 screen.
const originalCols = 80
const originalRows = 25
const paneRows = 8

// The cursor is always the sixth byte of the fifth data row.
const cursorRow = 4
const cursorSlot = 5
const viewAhead = int64(cursorRow*16 + cursorSlot)

// Column of the first hex byte and of the first ASCII character.
const dataCol = 15
const asciiCol = 64

func baseStyle(theme Theme) Style {
	return Style{FG: theme.fg(), BG: theme.bg()}
}

func barStyle(theme Theme) Style {
	return Style{FG: theme.barFG(), BG: theme.barBG()}
}

func drawUI(s *Screen, app *App) {
	theme := app.Options.Theme
	if s.W == originalCols && s.H == originalRows {
		drawOriginal(s, app)
	} else {
		drawAdaptive(s, app, Rect{0, 0, s.W, s.H})
	}
	if app.Modal != nil {
		drawModal(s, app.Modal, &app.Options, theme, Rect{0, 0, s.W, s.H})
	}
}

// drawOriginal draws the screen exactly as CABLE does on an 80x25 text-mode
// display.
func drawOriginal(s *Screen, app *App) {
	app.syncGeometry(paneRows, 16)
	drawOriginalTitle(s, app, 0, 0, 0)
	drawOriginalTitle(s, app, 1, 0, 21)
	drawOriginalBody(s, app, 0, 0, 1)
	drawOriginalBody(s, app, 1, 0, 13)
	drawOriginalStrip(s, app, 0, 0)
	drawOriginalBand(s, app, 0, 8)
	drawOriginalFooter(s, app, 0, 22)
}

func drawOriginalTitle(s *Screen, app *App, index, x, y int) {
	theme := app.Options.Theme
	active := app.Active == index
	style := baseStyle(theme)
	if active {
		style = barStyle(theme)
	}
	pane := app.Panes[index]
	name := ""
	if pane.File != nil {
		name = filepath.Base(pane.File.Path())
	} else {
		name = pane.Name()
	}
	length := pane.Len()
	readOnly := pane.File != nil && pane.File.ReadOnly()
	attrs := "A"
	if readOnly {
		attrs = "R"
	}
	name = truncateRunes(name, 56)
	if pane.Dirty() {
		name = truncateRunes("*"+name, 56)
	}
	s.Put(x, y, padRight(name, 56), style)
	s.Put(x+57, y, fmt.Sprintf("%8d %08X", length, length), style)
	s.Put(x+75, y, fmt.Sprintf("%4s ", attrs), style)
}

// drawOriginalStrip draws the function-key legend above the signed-on banner.
func drawOriginalStrip(s *Screen, app *App, x, y int) {
	theme := app.Options.Theme
	style := baseStyle(theme)
	entries := []struct {
		label string
		value string
		width int
	}{
		{"F2:", app.Options.caseLabel(), 7},
		{"F3:", app.Options.direction(), 8},
		{"F4:", app.Options.input(), 9},
		{"F5:", app.Options.show(), 7},
		{"F6:", app.Options.charset(), 6},
		{"SL:", app.Options.scrollLock(), 6},
	}
	for i, e := range entries {
		s.Put(x, y+2+i, e.label+padRight(e.value, e.width), style)
	}
	banner := []string{
		"CABLE V1.7X",
		"IBM Internal",
		"Use Only",
		"",
		"Dave Mitchell",
		"ZENO @ WINVMB",
	}
	for i, text := range banner {
		s.Put(x, y+14+i, text, style)
	}
}

// drawOriginalBody draws eight sixteen-byte rows, starting sixty-nine bytes
// before the cursor.
func drawOriginalBody(s *Screen, app *App, index, x, y int) {
	theme := app.Options.Theme
	active := app.Active == index
	compare := app.CompareMode
	ebcdic := app.Options.Ebcdic
	conceal := app.Options.Conceal
	cursor := app.Panes[index].Cursor
	start := int64(cursor) - viewAhead
	normal := baseStyle(theme)
	dim := Style{FG: theme.dim(), BG: theme.bg()}
	diff := Style{FG: theme.diffFG(), BG: theme.diffBG()}

	if app.Panes[index].File == nil {
		s.Put(x, y, "--- empty ---", dim)
		return
	}

	var disasmCells [][]Cell
	if app.Options.Disasm != nil {
		disasmCells = disasmWindow(app, index, paneRows, 16)
	}

	for row := 0; row < paneRows; row++ {
		rowStart := start + int64(row)*16
		lineY := y + row
		for i := 0; i < 16; i++ {
			off := rowStart + int64(i)
			blank := off < 0 || uint64(off) >= app.Panes[index].Len()
			var b byte
			has := false
			if !blank {
				b, has = byteAt(app, index, uint64(off))
			}
			other := byte(0)
			otherHas := false
			if compare && !blank {
				other, otherHas = byteAt(app, 1-index, uint64(off))
			}
			bx := x + dataCol + i*3
			if has {
				style := normal
				if compare && (!otherHas || b != other) {
					style = diff
				}
				s.Put(bx, lineY, fmt.Sprintf("%02X", b), style)
				if app.Options.Disasm == nil {
					s.Set(x+asciiCol+i, lineY, displayByte(b, ebcdic, conceal), style)
				}
			} else {
				s.Put(bx, lineY, "**", normal)
				if app.Options.Disasm == nil {
					s.Set(x+asciiCol+i, lineY, ' ', normal)
				}
			}
		}

		// The crosshair is fixed at screen columns 29 and 32.
		if left, right, ok := crosshairChars(index, row); ok {
			s.Put(x+dataCol+14, lineY, left, normal)
			s.Put(x+dataCol+17, lineY, right, normal)
		}

		// Slot 16 redraws only the attribute of the two hex digits of the
		// cursor byte.
		if active && row == cursorRow {
			cx := x + dataCol + cursorSlot*3
			s.Restyle(Rect{cx, lineY, 2, 1}, barStyle(theme))
		}
	}

	if disasmCells != nil {
		for row, cells := range disasmCells {
			for i, cell := range cells {
				s.Set(x+asciiCol+i, y+row, cell.Ch, cell.Style)
			}
		}
	}
}

// crosshairChars returns the crosshair glyphs for one row of one window.
func crosshairChars(index, row int) (string, string, bool) {
	if index == 0 {
		switch {
		case row == cursorRow:
			return "╔", "╗", true
		case row > cursorRow:
			return "║", "║", true
		}
		return "", "", false
	}
	switch {
	case row == cursorRow:
		return "╚", "╝", true
	case row < cursorRow:
		return "║", "║", true
	}
	return "", "", false
}

// drawOriginalBand draws the six-row band: two overlapping Hex/Dec boxes plus
// one preview line per window.
func drawOriginalBand(s *Screen, app *App, x, y int) {
	theme := app.Options.Theme
	normal := baseStyle(theme)
	bar := barStyle(theme)
	hex0 := hexValue(app.Panes[0].Cursor)
	dec0 := decValue(app.Panes[0].Cursor)
	hex1 := hexValue(app.Panes[1].Cursor)
	dec1 := decValue(app.Panes[1].Cursor)
	lines := []string{
		"╔════════════╗",
		"║" + hex0 + "╠═══════════════╩═╦╝",
		"╚" + dec0 + "║",
		"╔" + dec1 + "║",
		"║" + hex1 + "╠═══════════════╦═╩╗",
		"╚════════════╝",
	}
	for i, line := range lines {
		s.Put(x, y+i, line, normal)
	}

	hexRow, decRow := 1, 2
	if app.Active == 1 {
		hexRow, decRow = 4, 3
	}
	for _, row := range []int{hexRow, decRow} {
		s.Restyle(Rect{x + 5, y + row, 8, 1}, bar)
	}

	for index := 0; index < 2; index++ {
		row := 2
		if index == 1 {
			row = 3
		}
		active := app.Active == index
		style := normal
		if active {
			style = bar
			s.Restyle(Rect{x + dataCol, y + row, previewLen, 1}, bar)
		}
		s.Put(x+dataCol, y+row, previewText(app, index), style)
	}
}

func hexValue(offset uint64) string {
	return fmt.Sprintf("Hex %08X", uint32(offset))
}

func decValue(offset uint64) string {
	digits := fmt.Sprintf("%d", offset)
	if len(digits) > 8 {
		digits = digits[len(digits)-8:]
	}
	return fmt.Sprintf("Dec %8s", digits)
}

// drawOriginalFooter draws the menu, message line and a blank bottom row.
func drawOriginalFooter(s *Screen, app *App, x, y int) {
	theme := app.Options.Theme
	drawMenuLine(s, app, x, y)
	var text string
	style := baseStyle(theme)
	if app.HasMsg {
		text = app.Message
		style = Style{FG: theme.headerFG(), BG: theme.bg(), Bold: true}
	} else {
		text = app.menuDescription()
	}
	s.Put(x, y+1, text, style)
}

// adaptiveBytesPerRow returns the number of bytes on each line of the adaptive
// layout.  A line is 50n+9 columns for n sixteen-byte groups, so lines never
// end on a partial group.
func adaptiveBytesPerRow(width int) int {
	groups := (width - 9) / 50
	if width < 9 {
		groups = 0
	}
	if groups < 1 {
		groups = 1
	}
	if groups > 8 {
		groups = 8
	}
	return groups * 16
}

func drawAdaptive(s *Screen, app *App, area Rect) {
	footerH := footerRows
	if footerH > area.H {
		footerH = area.H
	}
	if third := area.H / 3; footerH > maxInt(third, 1) {
		footerH = maxInt(third, 1)
	}
	bodyH := area.H - footerH
	middleH := 0
	if bodyH >= 12 {
		middleH = 2
	}
	panesH := bodyH - middleH
	topH := panesH / 2
	bottomH := panesH - topH
	top := Rect{area.X, area.Y, area.W, topH}
	middle := Rect{area.X, area.Y + topH, area.W, middleH}
	bottom := Rect{area.X, area.Y + topH + middleH, area.W, bottomH}
	footer := Rect{area.X, area.Y + bodyH, area.W, footerH}

	bytesPerRow := adaptiveBytesPerRow(area.W)
	rows := satSub(topH, 1)
	if r := satSub(bottomH, 1); r < rows {
		rows = r
	}
	app.syncGeometry(rows, bytesPerRow)

	drawPane(s, app, 0, top)
	drawPane(s, app, 1, bottom)
	if middleH > 0 {
		drawAdaptiveMiddle(s, app, middle)
	}
	drawFooter(s, app, footer)
}

// adaptiveOffsetWidths derives the Hex/Dec field widths from the largest
// offset in either pane rather than from the current cursor, so the preview
// never slides sideways as the decimal count grows.
func adaptiveOffsetWidths(app *App) (int, int) {
	max := app.Panes[0].Len()
	if app.Panes[1].Len() > max {
		max = app.Panes[1].Len()
	}
	hex := len(fmt.Sprintf("%X", max))
	if hex < 8 {
		hex = 8
	}
	dec := len(fmt.Sprintf("%d", max))
	if dec < 1 {
		dec = 1
	}
	return hex, dec
}

// adaptiveHexCenter returns the horizontal middle of the hex columns of one
// adaptive data line.
func adaptiveHexCenter(bytesPerRow, dataCol int) int {
	groups := bytesPerRow / 16
	if groups < 1 {
		groups = 1
	}
	return dataCol + (33*groups-1)/2
}

// drawAdaptiveMiddle draws the two-line adaptive position band.
func drawAdaptiveMiddle(s *Screen, app *App, rect Rect) {
	if rect.W == 0 || rect.H == 0 {
		return
	}
	theme := app.Options.Theme
	normal := baseStyle(theme)
	cursorStyle := Style{FG: theme.cursorFG(), BG: theme.cursorBG(), Bold: true}

	hexWidth, decWidth := adaptiveOffsetWidths(app)
	dcol := hexWidth + 1
	center := adaptiveHexCenter(app.BytesPerRow, dcol)
	previewCol := center - previewBefore
	if previewCol < 0 {
		previewCol = 0
	}

	for index := 0; index < 2; index++ {
		if index >= rect.H {
			break
		}
		y := rect.Y + index
		active := app.Active == index
		style := normal
		if active {
			style = barStyle(theme)
			s.Restyle(Rect{rect.X, y, rect.W, 1}, style)
		}

		cursor := app.Panes[index].Cursor
		head := fmt.Sprintf("Hex %0*X  Dec %*d", hexWidth, cursor, decWidth, cursor)
		headLen := runeLen(head)
		start := previewCol
		if start < headLen+2 {
			start = headLen + 2
		}
		s.Put(rect.X, y, head, style)
		if start < rect.W {
			if start > headLen {
				s.Put(rect.X+headLen, y, strings.Repeat(" ", start-headLen), style)
			}
			room := rect.W - start
			if room > previewLen {
				room = previewLen
			}
			preview := []rune(previewText(app, index))
			for i := 0; i < room && i < len(preview); i++ {
				cellStyle := style
				if i == previewBefore {
					cellStyle = cursorStyle
				}
				s.Set(rect.X+start+i, y, preview[i], cellStyle)
			}
		}
	}
}

func drawPane(s *Screen, app *App, index int, rect Rect) {
	if rect.H == 0 || rect.W == 0 {
		return
	}
	theme := app.Options.Theme
	active := app.Active == index
	pane := app.Panes[index]
	cursor := pane.Cursor
	top := pane.Top
	name := pane.Name()
	length := pane.Len()
	dirty := pane.Dirty()
	readOnly := pane.File != nil && pane.File.ReadOnly()

	// The original puts the first file's title above its window and the second
	// file's title below its window.
	headerAtBottom := index == 1
	headerY := rect.Y
	if headerAtBottom {
		headerY = rect.Y + satSub(rect.H, 1)
	}
	header := Rect{rect.X, headerY, rect.W, 1}

	titleFG := theme.dim()
	titleBG := theme.bg()
	if active {
		titleFG = theme.bg()
		titleBG = theme.headerFG()
	}
	titleStyle := Style{FG: titleFG, BG: titleBG, Bold: active}
	s.Restyle(header, titleStyle)

	rightW := 38
	if rightW > rect.W {
		rightW = rect.W
	}
	titleText := fmt.Sprintf("%s  [%d bytes]", name, length)
	if dirty {
		titleText += " *"
	}
	if readOnly {
		titleText += " (read-only)"
	}
	s.PutLimit(header.X, header.Y, titleText, titleStyle, satSub(rect.W, rightW))

	flagChar := " "
	if active {
		flagChar = "A"
	}
	cmp := ""
	if app.CompareMode {
		cmp = " CMP"
	}
	flags := fmt.Sprintf("Hex %08X  Dec %d  %s%s", cursor, cursor, flagChar, cmp)
	flagX := header.X + satSub(rect.W, rightW)
	s.PutLimit(flagX, header.Y, flags, titleStyle, rightW)

	body := Rect{rect.X, rect.Y + 1, rect.W, satSub(rect.H, 1)}
	if headerAtBottom {
		body = Rect{rect.X, rect.Y, rect.W, satSub(rect.H, 1)}
	}
	if pane.File == nil {
		text := "--- empty ---"
		x := body.X + satSub(body.W, runeLen(text))/2
		s.Put(x, body.Y+body.H/2, text, Style{FG: theme.dim(), BG: theme.bg()})
		return
	}
	rows := body.H
	if rows == 0 {
		return
	}

	bpr := app.BytesPerRow
	if bpr < 16 {
		bpr = 16
	}
	bpr64 := uint64(bpr)
	for row := 0; row < rows; row++ {
		offset := top + uint64(row)*bpr64
		hexLine(s, app, index, offset, body.X, body.Y+row)
	}

	// The disassembly view is a vertical listing in the right-hand character
	// area.
	if app.Options.Disasm != nil {
		groups := bpr / 16
		if groups < 1 {
			groups = 1
		}
		prefix := 9 + (33*groups - 1) + 2
		if prefix < body.W {
			width := body.W - prefix
			window := disasmWindow(app, index, rows, width)
			for row, cells := range window {
				for i, cell := range cells {
					s.Set(body.X+prefix+i, body.Y+row, cell.Ch, cell.Style)
				}
			}
		}
	}
}

func hexLine(s *Screen, app *App, index int, offset uint64, x, y int) {
	theme := app.Options.Theme
	active := app.Active == index
	cursor := app.Panes[index].Cursor
	compare := app.CompareMode
	bpr := app.BytesPerRow
	if bpr < 16 {
		bpr = 16
	}

	normal := baseStyle(theme)
	dim := Style{FG: theme.dim(), BG: theme.bg()}
	cursorStyle := Style{FG: theme.cursorFG(), BG: theme.cursorBG(), Bold: true}
	diffStyle := Style{FG: theme.diffFG(), BG: theme.diffBG()}
	offsetStyle := Style{FG: theme.accent(), BG: theme.bg()}

	cx := x
	cx = s.Put(cx, y, fmt.Sprintf("%08X ", offset), offsetStyle)

	bytes := make([]byte, bpr)
	has := make([]bool, bpr)
	other := make([]byte, bpr)
	otherHas := make([]bool, bpr)
	for i := 0; i < bpr; i++ {
		bytes[i], has[i] = byteAt(app, index, offset+uint64(i))
		if compare {
			other[i], otherHas[i] = byteAt(app, 1-index, offset+uint64(i))
		}
	}

	styles := make([]Style, bpr)
	for i := 0; i < bpr; i++ {
		off := offset + uint64(i)
		isCursor := active && off == cursor
		differs := compare && (has[i] != otherHas[i] || (has[i] && bytes[i] != other[i]))
		switch {
		case isCursor:
			styles[i] = cursorStyle
		case differs:
			styles[i] = diffStyle
		default:
			styles[i] = normal
		}
	}

	for i := 0; i < bpr; i++ {
		if i > 0 && i%16 == 0 {
			s.Set(cx, y, ' ', normal)
			cx++
		}
		if has[i] {
			text := fmt.Sprintf("%02X", bytes[i])
			s.Put(cx, y, text, styles[i])
			cx += 2
		} else {
			s.Put(cx, y, "**", dim)
			cx += 2
		}
	}

	cx = s.Put(cx, y, "  ", normal)
	if app.Options.Disasm == nil {
		for i := 0; i < bpr; i++ {
			if i > 0 && i%16 == 0 {
				s.Set(cx, y, ' ', normal)
				cx++
			}
			if has[i] {
				s.Set(cx, y, displayByte(bytes[i], app.Options.Ebcdic, app.Options.Conceal), styles[i])
			} else {
				s.Set(cx, y, ' ', dim)
			}
			cx++
		}
	}
}

// disasmWindow builds a vertically centred instruction listing for one window.
// The instruction containing the cursor is placed on the middle row, earlier
// instructions above it and later ones below.  Each entry is one screen row of
// cells, padded to width.
func disasmWindow(app *App, index, rows, width int) [][]Cell {
	theme := app.Options.Theme
	normal := baseStyle(theme)
	currentStyle := normal.withBold(true)
	diffStyle := Style{FG: theme.diffFG(), BG: theme.diffBG()}

	out := make([][]Cell, rows)
	for row := range out {
		out[row] = make([]Cell, width)
		for i := range out[row] {
			out[row][i] = Cell{Ch: ' ', Style: normal}
		}
	}
	if app.Options.Disasm == nil || rows == 0 || width == 0 || app.Panes[index].File == nil {
		return out
	}

	compare := app.CompareMode
	cursor := app.Panes[index].Cursor
	center := rows / 2

	// Decode a generous window around the cursor.  Rounding the start to 256
	// bytes keeps instruction alignment stable while scrolling through a block.
	var anchor uint64
	if cursor > 512 {
		anchor = ((cursor - 512) / 256) * 256
	}
	end := cursor + 512
	if end > app.Panes[index].Len() {
		end = app.Panes[index].Len()
	}
	bytes := make([]byte, 0, end-anchor)
	for off := anchor; off < end; off++ {
		b, ok := byteAt(app, index, off)
		if !ok {
			break
		}
		bytes = append(bytes, b)
	}
	insns := disassemble(*app.Options.Disasm, bytes)
	if len(insns) == 0 {
		return out
	}

	rel := int(cursor - anchor)
	ci := 0
	for i := range insns {
		if insns[i].Offset <= rel {
			ci = i
		}
	}
	onInstruction := rel >= insns[ci].Offset && rel < insns[ci].Offset+insns[ci].Len

	for row := range out {
		target := row - center
		idx := ci + target
		if idx < 0 || idx >= len(insns) {
			continue
		}
		insn := insns[idx]
		abs := anchor + uint64(insn.Offset)
		isCursor := onInstruction && idx == ci
		differs := false
		if compare {
			for k := 0; k < insn.Len; k++ {
				off := abs + uint64(k)
				a, aok := byteAt(app, index, off)
				b, bok := byteAt(app, 1-index, off)
				if aok && bok {
					if a != b {
						differs = true
						break
					}
				} else if aok != bok {
					differs = true
					break
				}
			}
		}
		style := normal
		if isCursor {
			style = currentStyle
		} else if differs {
			style = diffStyle
		}
		text := insn.Text
		if width >= 24 {
			text = fmt.Sprintf("%08X  %s", abs, text)
		}
		r := []rune(text)
		for i := 0; i < width && i < len(r); i++ {
			out[row][i] = Cell{Ch: r[i], Style: style}
		}
	}
	return out
}

func byteAt(app *App, index int, offset uint64) (byte, bool) {
	if index < 0 || index >= len(app.Panes) {
		return 0, false
	}
	p := app.Panes[index]
	if p.File == nil {
		return 0, false
	}
	b, ok, err := p.File.ReadOffset(offset)
	if err != nil {
		return 0, false
	}
	return b, ok
}

// previewText builds the original 64-character preview: bytes cursor-16
// through cursor+47, with blanks outside the file.
func previewText(app *App, index int) string {
	cursor := app.Panes[index].Cursor
	start := int64(cursor) - previewBefore
	var b strings.Builder
	b.Grow(previewLen)
	for i := 0; i < previewLen; i++ {
		off := start + int64(i)
		if off < 0 {
			b.WriteRune(' ')
			continue
		}
		if v, ok := byteAt(app, index, uint64(off)); ok {
			b.WriteRune(displayByte(v, app.Options.Ebcdic, app.Options.Conceal))
		} else {
			b.WriteRune(' ')
		}
	}
	return b.String()
}

func drawFooter(s *Screen, app *App, area Rect) {
	theme := app.Options.Theme
	bg := baseStyle(theme)
	s.Fill(area, ' ', bg)

	rows := [4]Rect{}
	for i := range rows {
		rows[i] = Rect{area.X, area.Y + i, area.W, 1}
	}

	statusStyle := Style{FG: theme.accent(), BG: theme.bg()}
	status := app.statusLine() + "  (F7 / Ctrl+L toggles)"
	if app.HasMsg {
		statusStyle = Style{FG: theme.headerFG(), BG: theme.bg(), Bold: true}
		status = app.statusLine()
	}
	if 0 < area.H {
		s.PutLimit(rows[0].X, rows[0].Y, status, statusStyle, area.W)
	}
	if 1 < area.H {
		s.PutLimit(rows[1].X, rows[1].Y, app.fkeyLine(), Style{FG: theme.dim(), BG: theme.bg()}, area.W)
	}
	if 2 < area.H {
		drawMenuLine(s, app, rows[2].X, rows[2].Y)
	}
	if 3 < area.H {
		s.PutLimit(rows[3].X, rows[3].Y, app.menuDescription(), bg, area.W)
	}
}

// drawMenuLine draws CABLE: Compare Dir Edit ... with the currently selected
// command in reverse video and every accelerator letter bold.
func drawMenuLine(s *Screen, app *App, x, y int) {
	theme := app.Options.Theme
	cx := s.Put(x, y, "CABLE: ", baseStyle(theme))
	for i, item := range menuItems {
		if i > 0 {
			s.Set(cx, y, ' ', baseStyle(theme))
			cx++
		}
		base := baseStyle(theme)
		if i == app.Menu {
			base = Style{FG: theme.barFG(), BG: theme.barBG()}
		}
		r := []rune(item.Label)
		s.Set(cx, y, r[0], base.withBold(true))
		cx++
		for _, ch := range r[1:] {
			s.Set(cx, y, ch, base)
			cx++
		}
	}
}

func drawModal(s *Screen, m *Modal, options *Options, theme Theme, area Rect) {
	switch m.Kind {
	case modalConfirm:
		rect := centered(area, 76, 7)
		inner := drawBox(s, rect, m.Title, theme)
		lines := []string{""}
		lines = append(lines, wrapText(m.Text, inner.W)...)
		lines = append(lines, "")
		lines = append(lines, "Y = yes    N = no    Esc = cancel")
		y := inner.Y
		for i, line := range lines {
			if i == 1 {
				s.PutLimit(inner.X, y, line, Style{FG: theme.headerFG(), BG: theme.bg(), Bold: true}, inner.W)
			} else if i == len(lines)-1 {
				s.PutLimit(inner.X, y, line, Style{FG: theme.dim(), BG: theme.bg()}, inner.W)
			}
			y++
			if y >= inner.Y+inner.H {
				break
			}
		}
	case modalInput:
		rect := centered(area, 78, 8)
		inner := drawBox(s, rect, m.Title, theme)
		var hint string
		switch m.Input {
		case inputAdd, inputSub, inputGoto:
			hint = "Decimal, or HEX with a leading H or 0x"
		case inputSearch:
			hint = "Enter = search    Esc = cancel"
		default:
			hint = "Enter = open    Esc = cancel"
		}
		s.PutLimit(inner.X, inner.Y, m.Prompt, Style{FG: theme.accent(), BG: theme.bg()}, inner.W)
		s.Put(inner.X, inner.Y+2, "> ", Style{FG: theme.accent(), BG: theme.bg()})
		valueX := inner.X + 2
		s.PutLimit(valueX, inner.Y+2, m.Value, Style{FG: theme.cursorFG(), BG: theme.cursorBG()}, maxInt(inner.W-3, 0))
		valueX += runeLen(m.Value)
		if valueX < inner.X+inner.W {
			s.Set(valueX, inner.Y+2, '_', baseStyle(theme))
		}
		s.PutLimit(inner.X, inner.Y+4, hint, Style{FG: theme.dim(), BG: theme.bg()}, inner.W)
	case modalOptions:
		rect := centered(area, 64, 15)
		inner := drawBox(s, rect, "OPTIONS", theme)
		labels := []struct {
			name    string
			current bool
		}{
			{"ASCII", !options.Ebcdic},
			{"Back", !options.Forward},
			{"Conceal", options.Conceal},
			{"EBCDIC", options.Ebcdic},
			{"Fore", options.Forward},
			{"Help", false},
			{"Ignore", !options.RespectCase},
			{"Quit", false},
			{"Respect", options.RespectCase},
			{"Show", !options.Conceal},
			{"heX/char", options.HexInput},
		}
		for i, label := range labels {
			if i >= inner.H {
				break
			}
			selected := i == m.Index
			st := baseStyle(theme)
			if selected {
				st = Style{FG: theme.bg(), BG: theme.cursorBG(), Bold: true}
			}
			mark := "   "
			if label.current {
				mark = "(*)"
			}
			s.PutLimit(inner.X, inner.Y+i, fmt.Sprintf("  %-24s %s", label.name, mark), st, inner.W)
		}
		if len(labels) < inner.H {
			s.PutLimit(inner.X, inner.Y+len(labels), "Enter/Space = select   Esc = close",
				Style{FG: theme.dim(), BG: theme.bg()}, inner.W)
		}
	case modalModes:
		rect := centered(area, 44, 9)
		inner := drawBox(s, rect, "MODE", theme)
		labels := []string{"Attributes", "Color", "Help", "Mono", "Quit"}
		for i, label := range labels {
			if i >= inner.H {
				break
			}
			st := baseStyle(theme)
			if i == m.Index {
				st = Style{FG: theme.bg(), BG: theme.cursorBG(), Bold: true}
			}
			s.PutLimit(inner.X, inner.Y+i, fmt.Sprintf("  %-24s", label), st, inner.W)
		}
	case modalAttributes:
		rect := centered(area, 46, 8)
		inner := drawBox(s, rect, "ATTRIBUTES", theme)
		themes := []Theme{ThemeClassic, ThemeAmber, ThemeMono}
		for i, t := range themes {
			if i >= inner.H {
				break
			}
			text := t.Name()
			if t == theme {
				text += "  (current)"
			}
			st := baseStyle(theme)
			if i == m.Index {
				st = Style{FG: theme.bg(), BG: theme.cursorBG(), Bold: true}
			}
			s.PutLimit(inner.X, inner.Y+i, fmt.Sprintf("  %-24s", text), st, inner.W)
		}
		if len(themes) < inner.H {
			s.PutLimit(inner.X, inner.Y+len(themes), "Enter = choose   Esc = close",
				Style{FG: theme.dim(), BG: theme.bg()}, inner.W)
		}
	case modalHelp:
		rect := centered(area, 88, satSub(area.H, 2))
		inner := drawBox(s, rect, helpTitle, theme)
		visible := inner.H
		max := len(helpLines) - 1
		start := m.Scroll
		if start > max {
			start = max
		}
		if start < 0 {
			start = 0
		}
		end := start + visible
		if end > len(helpLines) {
			end = len(helpLines)
		}
		for i, line := range helpLines[start:end] {
			s.PutLimit(inner.X, inner.Y+i, line, baseStyle(theme), inner.W)
		}
		if inner.H > 0 {
			footer := fmt.Sprintf("Line %d of %d   Up/Down, PgUp/PgDn, Home/End scroll   Esc close",
				start+1, len(helpLines))
			s.PutLimit(inner.X, inner.Y+inner.H-1, footer, Style{FG: theme.dim(), BG: theme.bg()}, inner.W)
		}
	case modalDir:
		drawDir(s, m.Dir, theme, area)
	}
}

func drawDir(s *Screen, b *DirBrowser, theme Theme, area Rect) {
	rect := centered(area, satSub(area.W, 4), satSub(area.H, 2))
	inner := drawBox(s, rect, "Dir: "+b.Dir, theme)
	if inner.H < 4 || inner.W == 0 {
		return
	}

	status := "DIR: Enter [drive:][path\\][filespec] (press <key> for current directory)"
	s.PutLimit(inner.X, inner.Y, status, Style{FG: theme.accent(), BG: theme.bg()}, inner.W)

	const footerRowCount = 3
	listH := satSub(inner.H, 1+footerRowCount)
	b.EnsureVisible(maxInt(listH, 1))
	listRect := Rect{inner.X, inner.Y + 1, inner.W, listH}
	for i := 0; i < listH; i++ {
		idx := b.Scroll + i
		if idx >= len(b.Entries) {
			break
		}
		e := b.Entries[idx]
		size := formatSize(e.Size)
		if e.IsDir {
			size = "<DIR>"
		}
		mark := " "
		if e.IsDir {
			mark = "/"
		}
		line := fmt.Sprintf("%s%-36s %9s  %s", mark, e.Name, size, formatTime(e.Modified, e.HasTime))
		st := baseStyle(theme)
		if idx == b.Selected {
			st = Style{FG: theme.bg(), BG: theme.cursorBG(), Bold: true}
		}
		s.PutLimit(listRect.X, listRect.Y+i, line, st, listRect.W)
	}

	footer := []struct {
		text  string
		style Style
	}{}
	switch {
	case b.PathInput != nil:
		footer = append(footer, struct {
			text  string
			style Style
		}{"Path: " + *b.PathInput + "_", Style{FG: theme.headerFG(), BG: theme.bg()}})
	case b.SortPrompt:
		footer = append(footer, struct {
			text  string
			style Style
		}{"Sort by N(ame), E(xtension), D(ate) or S(ize)?", Style{FG: theme.headerFG(), BG: theme.bg(), Bold: true}})
	default:
		footer = append(footer,
			struct {
				text  string
				style Style
			}{"Scroll using <keys>, PgUp, PgDn, Home, End  -  Esc returns to CABLE", Style{FG: theme.dim(), BG: theme.bg()}},
			struct {
				text  string
				style Style
			}{fmt.Sprintf("%s   %d of %d   Enter = open   Ctrl+Enter = view in other window   Tab = target   S = sort   / = path",
				b.Sort.label(), selectedDisplay(b), len(b.Entries)), Style{FG: theme.dim(), BG: theme.bg()}},
		)
	}
	if b.Error != "" {
		footer = append(footer, struct {
			text  string
			style Style
		}{b.Error, Style{FG: theme.diffBG(), BG: theme.bg()}})
	}
	footerY := inner.Y + inner.H - len(footer)
	if footerY < inner.Y {
		footerY = inner.Y
	}
	for i, line := range footer {
		if inner.Y+inner.H-1 < footerY+i {
			break
		}
		s.PutLimit(inner.X, footerY+i, line.text, line.style, inner.W)
	}
}

func selectedDisplay(b *DirBrowser) int {
	if len(b.Entries) == 0 {
		return 0
	}
	return b.Selected + 1
}

// drawBox fills a rectangle, draws a single-line border and returns the inner
// area.
func drawBox(s *Screen, r Rect, title string, theme Theme) Rect {
	fill := baseStyle(theme)
	border := Style{FG: theme.border(), BG: theme.bg()}
	s.Fill(r, ' ', fill)
	if r.W < 2 || r.H < 2 {
		return Rect{r.X, r.Y, maxInt(r.W-2, 0), maxInt(r.H-2, 0)}
	}
	top := r.Y
	bottom := r.Y + r.H - 1
	left := r.X
	right := r.X + r.W - 1
	for x := left + 1; x < right; x++ {
		s.Set(x, top, '─', border)
		s.Set(x, bottom, '─', border)
	}
	for y := top + 1; y < bottom; y++ {
		s.Set(left, y, '│', border)
		s.Set(right, y, '│', border)
	}
	s.Set(left, top, '┌', border)
	s.Set(right, top, '┐', border)
	s.Set(left, bottom, '└', border)
	s.Set(right, bottom, '┘', border)
	if title != "" {
		s.PutLimit(left+1, top, title, fill, maxInt(r.W-2, 0))
	}
	return Rect{r.X + 1, r.Y + 1, maxInt(r.W-2, 0), maxInt(r.H-2, 0)}
}

func centered(area Rect, width, height int) Rect {
	w := width
	if w > area.W {
		w = area.W
	}
	if w < 3 {
		w = 3
	}
	h := height
	if h > area.H {
		h = area.H
	}
	if h < 3 {
		h = 3
	}
	return Rect{
		area.X + satSub(area.W, w)/2,
		area.Y + satSub(area.H, h)/2,
		w, h,
	}
}

func satSub(a, b int) int {
	if a < b {
		return 0
	}
	return a - b
}
