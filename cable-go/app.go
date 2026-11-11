// Application state and key handling.

package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"unicode/utf8"
)

var menuItems = []struct {
	Label string
	Key   rune
}{
	{"Compare", 'C'},
	{"Dir", 'D'},
	{"Edit", 'E'},
	{"Help", 'H'},
	{"Mode", 'M'},
	{"Next", 'N'},
	{"Options", 'O'},
	{"+", '+'},
	{"-", '-'},
	{"=", '='},
	{"Quit", 'Q'},
	{"Search", 'S'},
	{"View", 'V'},
	{"Write", 'W'},
}

// isScrollLockToggle matches the original Scroll Lock state plus the aliases
// used on keyboards and terminals that cannot report it.
func isScrollLockToggle(k Key) bool {
	switch k.Code {
	case keyScrollLock, keyF7:
		return true
	}
	if k.Code == keyRune {
		if k.Rune == '\f' {
			return true
		}
		if k.Ctrl {
			switch k.Rune {
			case 'l', 'L', 'b', 'B':
				return true
			}
		}
	}
	return false
}

type Theme int

const (
	ThemeClassic Theme = iota
	ThemeAmber
	ThemeMono
)

func (t Theme) Name() string {
	switch t {
	case ThemeClassic:
		return "Colour (blue/yellow)"
	case ThemeAmber:
		return "Amber"
	default:
		return "Mono"
	}
}

func (t Theme) bg() Color {
	if t == ThemeClassic {
		return bg256(4)
	}
	return bg256(0)
}

func (t Theme) fg() Color {
	switch t {
	case ThemeClassic:
		return fg256(11)
	case ThemeAmber:
		return fgRGB(255, 191, 0)
	default:
		return fg256(15)
	}
}

func (t Theme) dim() Color {
	switch t {
	case ThemeClassic:
		return fg256(7)
	case ThemeAmber:
		return fgRGB(150, 100, 0)
	default:
		return fg256(8)
	}
}

// barBG is the background of the original selection bars (0x78 in colour
// mode, i.e. light grey) and of the highlighted menu command.
func (t Theme) barBG() Color {
	switch t {
	case ThemeClassic:
		return bg256(7)
	case ThemeAmber:
		return bgRGB(150, 100, 0)
	default:
		return bg256(8)
	}
}

// barFG is the text drawn on a selection bar (0x78 = dark grey on light grey).
func (t Theme) barFG() Color {
	switch t {
	case ThemeClassic:
		return fg256(8)
	default:
		return fg256(0)
	}
}

func (t Theme) accent() Color {
	switch t {
	case ThemeClassic:
		return fg256(6)
	case ThemeAmber:
		return fgRGB(255, 220, 120)
	default:
		return fg256(15)
	}
}

func (t Theme) headerFG() Color {
	switch t {
	case ThemeClassic:
		return fg256(15)
	case ThemeAmber:
		return fgRGB(255, 220, 120)
	default:
		return fg256(15)
	}
}

func (t Theme) cursorFG() Color {
	if t == ThemeClassic {
		return fg256(4)
	}
	return fg256(0)
}

func (t Theme) cursorBG() Color {
	switch t {
	case ThemeClassic:
		return bg256(15)
	case ThemeAmber:
		return bgRGB(255, 191, 0)
	default:
		return bg256(15)
	}
}

func (t Theme) diffFG() Color {
	if t == ThemeClassic {
		return fg256(15)
	}
	return fg256(0)
}

func (t Theme) diffBG() Color {
	switch t {
	case ThemeClassic:
		return bg256(1)
	case ThemeAmber:
		return bgRGB(255, 191, 0)
	default:
		return bg256(15)
	}
}

func (t Theme) border() Color {
	switch t {
	case ThemeClassic:
		return fg256(6)
	case ThemeAmber:
		return fgRGB(255, 191, 0)
	default:
		return fg256(15)
	}
}

type Options struct {
	Ebcdic      bool
	Conceal     bool
	RespectCase bool
	Forward     bool
	HexInput    bool
	// MoveBoth is the Scroll Lock state: false moves the active window only
	// (SL:Active), true moves both windows together (SL:Both).
	MoveBoth bool
	Theme    Theme
	// Disasm selects passive disassembly for the right-hand column.  nil shows
	// ASCII/EBCDIC.
	Disasm *Cpu
}

func defaultOptions() Options {
	return Options{
		Conceal:     true,
		RespectCase: true,
		Forward:     true,
		Theme:       ThemeClassic,
	}
}

func (o *Options) charset() string {
	if o.Ebcdic {
		return "EBCDIC"
	}
	return "ASCII"
}

func (o *Options) direction() string {
	if o.Forward {
		return "Forward"
	}
	return "Backward"
}

func (o *Options) caseLabel() string {
	if o.RespectCase {
		return "Respect"
	}
	return "Ignore"
}

func (o *Options) show() string {
	if o.Conceal {
		return "Conceal"
	}
	return "Show"
}

func (o *Options) input() string {
	if o.HexInput {
		return "HEX"
	}
	return "Character"
}

func (o *Options) scrollLock() string {
	if o.MoveBoth {
		return "Both"
	}
	return "Active"
}

// sideView names the right-hand character view.  Without a disassembly CPU it
// is the active character set, so the status line is unchanged in text mode.
func (o *Options) sideView() string {
	if o.Disasm != nil {
		return o.Disasm.Label()
	}
	return o.charset()
}

// cycleSideView advances the side view through every CPU and back to
// characters.
func (o *Options) cycleSideView() {
	if o.Disasm == nil {
		c := CpuX86
		o.Disasm = &c
		return
	}
	if next, ok := o.Disasm.nextView(); ok {
		c := next
		o.Disasm = &c
		return
	}
	o.Disasm = nil
}

type Pane struct {
	File   *PagedFile
	Cursor uint64
	Top    uint64
}

func emptyPane() *Pane { return &Pane{} }

func openPane(path string) (*Pane, error) {
	f, err := openPagedFile(path)
	if err != nil {
		return nil, err
	}
	return &Pane{File: f}, nil
}

func (p *Pane) Len() uint64 {
	if p.File == nil {
		return 0
	}
	return p.File.Len()
}

func (p *Pane) Name() string {
	if p.File == nil {
		return "--- empty ---"
	}
	full := p.File.Path()
	if runeLen(full) <= 34 {
		return full
	}
	return filepath.Base(full)
}

func (p *Pane) Dirty() bool {
	return p.File != nil && p.File.Dirty()
}

func (p *Pane) cursorMax(allowAppend bool) uint64 {
	length := p.Len()
	if length == 0 {
		return 0
	}
	if allowAppend {
		return length
	}
	return length - 1
}

func (p *Pane) clampCursor(allowAppend bool) {
	max := p.cursorMax(allowAppend)
	if p.Cursor > max {
		p.Cursor = max
	}
}

func (p *Pane) ensureVisible(rows, bytesPerRow int) {
	if rows < 1 {
		rows = 1
	}
	if bytesPerRow < 16 {
		bytesPerRow = 16
	}
	bpr := uint64(bytesPerRow)
	p.Top -= p.Top % bpr
	page := uint64(rows) * bpr
	lineStart := (p.Cursor / bpr) * bpr
	if lineStart < p.Top {
		p.Top = lineStart
	} else if p.Cursor >= p.Top+page {
		lastRow := p.Cursor / bpr
		if lastRow >= uint64(rows)-1 {
			lastRow -= uint64(rows) - 1
		} else {
			lastRow = 0
		}
		p.Top = lastRow * bpr
	}
	// Keep the window inside the file when possible.
	length := p.Len()
	if length > 0 && p.Top >= length {
		p.Top = ((length - 1) / bpr) * bpr
	}
}

type confirmAction int

const (
	confirmQuit confirmAction = iota
	confirmWrite
	confirmLoad
)

type inputAction int

const (
	inputAdd inputAction = iota
	inputSub
	inputGoto
	inputSearch
	inputView
)

type modalKind int

const (
	modalConfirm modalKind = iota
	modalInput
	modalOptions
	modalModes
	modalAttributes
	modalHelp
	modalDir
)

type Modal struct {
	Kind modalKind

	Title string
	Text  string

	Prompt string
	Value  string
	Input  inputAction

	Confirm confirmAction
	Path    string
	Pane    int

	Index  int
	Scroll int

	Dir *DirBrowser
}

type editState struct {
	nibble byte
	has    bool
}

type lastActionKind int

const (
	lastSearch lastActionKind = iota
	lastCompare
)

type lastAction struct {
	Kind   lastActionKind
	Needle []byte
}

type App struct {
	Panes  [2]*Pane
	Active int

	Options Options
	Modal   *Modal
	Menu    int
	Quit    bool
	Message string
	HasMsg  bool

	RowsPerPane int
	BytesPerRow int

	CompareMode bool
	Last        *lastAction
	Edit        *editState
}

func newApp(paths []string, options Options) (*App, error) {
	a := &App{
		Panes:       [2]*Pane{emptyPane(), emptyPane()},
		Options:     options,
		Menu:        3, // the original starts with Help highlighted
		RowsPerPane: 10,
		BytesPerRow: 16,
	}
	for i, path := range paths {
		if i >= 2 {
			break
		}
		pane, err := openPane(path)
		if err != nil {
			return nil, fmt.Errorf("Error opening file %s: %v", path, err)
		}
		a.Panes[i] = pane
	}
	if len(paths) == 0 {
		cwd, err := os.Getwd()
		if err != nil {
			cwd = "."
		}
		a.Modal = &Modal{Kind: modalDir, Dir: newDirBrowser(cwd, 0)}
	}
	return a, nil
}

func (a *App) anyDirty() bool {
	return a.Panes[0].Dirty() || a.Panes[1].Dirty()
}

func (a *App) setMessage(text string) {
	a.Message = text
	a.HasMsg = true
}

func (a *App) clearMessage() {
	a.Message = ""
	a.HasMsg = false
}

// syncGeometry adopts the geometry chosen by the renderer and keeps both
// panes inside their windows.  It is called on every draw so a terminal resize
// is handled without a keypress.
func (a *App) syncGeometry(rows, bytesPerRow int) {
	if rows < 1 {
		rows = 1
	}
	if bytesPerRow < 16 {
		bytesPerRow = 16
	}
	a.RowsPerPane = rows
	a.BytesPerRow = bytesPerRow
	for _, p := range a.Panes {
		if p.File != nil {
			p.ensureVisible(rows, bytesPerRow)
		}
	}
}

func (a *App) statusLine() string {
	if a.HasMsg {
		return a.Message
	}
	s := fmt.Sprintf("%s  %s  %s  %s  %s  SL:%s",
		a.Options.sideView(), a.Options.direction(), a.Options.caseLabel(),
		a.Options.show(), a.Options.input(), a.Options.scrollLock())
	if a.CompareMode {
		s += "  [COMPARE]"
	}
	if a.Edit != nil {
		s += "  [EDIT]"
	}
	return s
}

func (a *App) fkeyLine() string {
	return fmt.Sprintf("F1:Help  F2:%s  F3:%s  F4:%s  F5:%s  F6:%s  F8:%s  Tab:Active",
		a.Options.caseLabel(), a.Options.direction(), a.Options.input(),
		a.Options.show(), a.Options.charset(), a.Options.sideView())
}

// menuDescription is the message line for the highlighted menu command.  The
// original prints it whenever the highlight moves; Help's entry is the generic
// hint, which is why the initial screen shows the hint.  The Enter keycap is
// rendered as the original CP437 glyphs.
func (a *App) menuDescription() string {
	switch a.Menu {
	case 0:
		return "Compare files starting at current positions"
	case 1:
		return "Display directory"
	case 2:
		return "Edit (in ASCII) file in active window"
	case 3:
		return "Type first letter of command or press ◄─┘ to select hilited command"
	case 4:
		return "Switch between Colour and Mono monitors"
	case 5:
		return "Repeat Compare or Search"
	case 6:
		return "Change case, direction, data entry and show options"
	case 7:
		return "Move current position forwards"
	case 8:
		return "Move current position backwards"
	case 9:
		return "Set current position"
	case 10:
		return "Quit CABLE, return to DOS"
	case 11:
		return "Search for ASCII string"
	case 12:
		return "View file in active window"
	case 13:
		return "Write changes to active file back to disk"
	default:
		return ""
	}
}

func (a *App) handleKey(k Key) {
	// Scroll Lock is absent from most Mac keyboards, so F7 and Ctrl+L/Ctrl+B
	// are available as equivalents.
	if isScrollLockToggle(k) {
		a.Options.MoveBoth = !a.Options.MoveBoth
		return
	}
	if a.Modal != nil {
		a.handleModalKey(k)
		return
	}
	if a.Edit != nil {
		a.handleEditKey(k)
		return
	}

	if k.Ctrl && k.Code == keyRune {
		switch k.Rune {
		case 'q', 'Q':
			a.tryQuit()
		case 's', 'S':
			a.writeConfirm()
		}
		return
	}

	if k.Code == keyRune {
		switch k.Rune {
		case ' ':
			a.Menu = (a.Menu + 1) % len(menuItems)
			a.clearMessage()
			return
		}
		if !k.Ctrl {
			a.handleCommandChar(k.Rune)
			return
		}
	}

	switch {
	case k.isF(1):
		a.openHelp()
	case k.isF(2):
		a.Options.RespectCase = !a.Options.RespectCase
		a.setMessage("Case: " + a.Options.caseLabel())
	case k.isF(3):
		a.Options.Forward = !a.Options.Forward
		a.setMessage("Direction: " + a.Options.direction())
	case k.isF(4):
		a.Options.HexInput = !a.Options.HexInput
		a.setMessage("Input: " + a.Options.input())
	case k.isF(5):
		a.Options.Conceal = !a.Options.Conceal
		a.setMessage("Unprintables: " + a.Options.show())
	case k.isF(6):
		a.Options.Ebcdic = !a.Options.Ebcdic
		a.setMessage("Character set: " + a.Options.charset())
	case k.isF(8):
		a.Options.cycleSideView()
		a.setMessage("Side view: " + a.Options.sideView())
	case k.isF(9):
		a.Menu = (a.Menu + len(menuItems) - 1) % len(menuItems)
		a.clearMessage()
	case k.isF(10):
		a.Menu = (a.Menu + 1) % len(menuItems)
		a.clearMessage()
	case k.Code == keyTab, k.Code == keyBackTab:
		a.switchPane()
	case k.Code == keyEnter:
		a.executeMenu(a.Menu)
	case k.Code == keyEsc:
		if a.CompareMode {
			a.CompareMode = false
			a.setMessage("Compare display off")
		} else {
			a.clearMessage()
		}
	case k.Code == keyLeft:
		a.moveCursor(-1)
	case k.Code == keyRight:
		a.moveCursor(1)
	case k.Code == keyUp:
		a.moveCursor(-int64(a.BytesPerRow))
	case k.Code == keyDown:
		a.moveCursor(int64(a.BytesPerRow))
	case k.Code == keyPageUp:
		a.moveCursor(-int64(maxInt(a.RowsPerPane, 1) * maxInt(a.BytesPerRow, 16)))
	case k.Code == keyPageDown:
		a.moveCursor(int64(maxInt(a.RowsPerPane, 1) * maxInt(a.BytesPerRow, 16)))
	case k.Code == keyHome:
		a.moveHome()
	case k.Code == keyEnd:
		a.moveEnd()
	}
}

func (a *App) activePane() *Pane {
	return a.Panes[a.Active]
}

func (a *App) switchPane() {
	a.Active = 1 - a.Active
	a.Panes[a.Active].clampCursor(a.Edit != nil)
	a.Panes[a.Active].ensureVisible(a.RowsPerPane, a.BytesPerRow)
}

func (a *App) moveCursor(delta int64) {
	allowAppend := a.Edit != nil
	rows := a.RowsPerPane
	bpr := a.BytesPerRow
	active := a.Active
	nudgePane(a.Panes[active], delta, allowAppend, rows, bpr)
	if a.bothPanes() {
		nudgePane(a.Panes[1-active], delta, allowAppend, rows, bpr)
	}
}

// bothPanes is true when Scroll Lock is on and both windows hold a file.
func (a *App) bothPanes() bool {
	return a.Options.MoveBoth && a.Panes[0].File != nil && a.Panes[1].File != nil
}

func nudgePane(p *Pane, delta int64, allowAppend bool, rows, bpr int) {
	if p.File == nil {
		return
	}
	max := int64(p.cursorMax(allowAppend))
	next := int64(p.Cursor) + delta
	if next < 0 {
		next = 0
	}
	if next > max {
		next = max
	}
	if next < 0 {
		next = 0
	}
	p.Cursor = uint64(next)
	p.ensureVisible(rows, bpr)
}

func (a *App) moveHome() {
	active := a.Active
	homePane(a.Panes[active])
	if a.bothPanes() {
		homePane(a.Panes[1-active])
	}
}

func homePane(p *Pane) {
	if p.File == nil {
		return
	}
	p.Cursor = 0
	p.Top = 0
}

func (a *App) moveEnd() {
	allowAppend := a.Edit != nil
	rows := a.RowsPerPane
	bpr := a.BytesPerRow
	active := a.Active
	endPane(a.Panes[active], allowAppend, rows, bpr)
	if a.bothPanes() {
		endPane(a.Panes[1-active], allowAppend, rows, bpr)
	}
}

func endPane(p *Pane, allowAppend bool, rows, bpr int) {
	if p.File == nil {
		return
	}
	p.Cursor = p.cursorMax(allowAppend)
	p.ensureVisible(rows, bpr)
}

func (a *App) handleCommandChar(c rune) {
	switch c {
	case 'c', 'C':
		a.startCompare()
	case 'd', 'D':
		a.openDir()
	case 'e', 'E':
		a.startEdit()
	case 'h', 'H':
		a.openHelp()
	case 'm', 'M':
		a.openModes()
	case 'n', 'N':
		a.repeatLast()
	case 'o', 'O':
		a.openOptions()
	case '+':
		a.openOffsetInput(inputAdd)
	case '-':
		a.openOffsetInput(inputSub)
	case '=':
		a.openOffsetInput(inputGoto)
	case 'q', 'Q':
		a.tryQuit()
	case 's', 'S':
		a.openSearch()
	case 'v', 'V':
		a.openView()
	case 'w', 'W':
		a.writeConfirm()
	}
}

func (a *App) executeMenu(index int) {
	if index >= len(menuItems) {
		index = len(menuItems) - 1
	}
	a.Menu = index
	switch index {
	case 0:
		a.startCompare()
	case 1:
		a.openDir()
	case 2:
		a.startEdit()
	case 3:
		a.openHelp()
	case 4:
		a.openModes()
	case 5:
		a.repeatLast()
	case 6:
		a.openOptions()
	case 7:
		a.openOffsetInput(inputAdd)
	case 8:
		a.openOffsetInput(inputSub)
	case 9:
		a.openOffsetInput(inputGoto)
	case 10:
		a.tryQuit()
	case 11:
		a.openSearch()
	case 12:
		a.openView()
	case 13:
		a.writeConfirm()
	}
}

func (a *App) openHelp() {
	a.Modal = &Modal{Kind: modalHelp}
}

func (a *App) openOptions() {
	a.Modal = &Modal{Kind: modalOptions, Index: 0}
}

func (a *App) openModes() {
	a.Modal = &Modal{Kind: modalModes, Index: 0}
}

func (a *App) openDir() {
	cwd, err := os.Getwd()
	if err != nil {
		cwd = "."
	}
	a.Modal = &Modal{Kind: modalDir, Dir: newDirBrowser(cwd, a.Active)}
}

func (a *App) openInput(title, prompt string, action inputAction, value string) {
	a.Modal = &Modal{Kind: modalInput, Title: title, Prompt: prompt, Value: value, Input: action}
}

func (a *App) openOffsetInput(action inputAction) {
	if a.activePane().File == nil {
		a.setMessage("No file to edit")
		return
	}
	var title, prompt string
	switch action {
	case inputAdd:
		title, prompt = "+", "+: Enter value to add to current position (if HEX start with H)"
	case inputSub:
		title, prompt = "-", "-: Enter value to subtract from current position (if HEX start with H)"
	case inputGoto:
		title, prompt = "=", "=: Enter new current position (if HEX start with H)"
	}
	a.openInput(title, prompt, action, "")
}

func (a *App) openSearch() {
	if a.activePane().File == nil {
		a.setMessage("No file to search")
		return
	}
	var prompt string
	if a.Options.HexInput {
		prompt = "SEARCH: Enter HEX search string (blanks ignored)"
	} else if a.Options.Ebcdic {
		prompt = "SEARCH: Enter EBCDIC search string"
	} else {
		prompt = "SEARCH: Enter ASCII search string"
	}
	a.openInput("SEARCH", prompt, inputSearch, "")
}

func (a *App) openView() {
	a.openInput("VIEW", "VIEW: Enter [drive:][path\\]filespec", inputView, "")
}

func (a *App) editPrompt() string {
	if a.Options.HexInput {
		return "EDIT: Enter HEX digits (Esc to quit)"
	}
	if a.Options.Ebcdic {
		return "EDIT: Enter EBCDIC characters (Esc to quit)"
	}
	return "EDIT: Enter ASCII characters (Esc to quit)"
}

func (a *App) startEdit() {
	if a.activePane().File == nil {
		a.setMessage("No file to edit")
		return
	}
	if a.Options.Disasm != nil {
		a.setMessage("Edit unavailable in disassembly view")
		return
	}
	a.Edit = &editState{}
	a.Message = a.editPrompt()
	a.HasMsg = true
}

func (a *App) handleEditKey(k Key) {
	rows := a.RowsPerPane
	bpr := a.BytesPerRow
	switch {
	case k.Code == keyEsc:
		a.Edit = nil
		a.clearMessage()
	case k.Code == keyScrollLock, k.isF(7):
		a.Options.MoveBoth = !a.Options.MoveBoth
	case k.Code == keyTab, k.Code == keyBackTab:
		other := 1 - a.Active
		if a.Panes[other].File == nil {
			a.setMessage("No file to edit")
		} else {
			a.Active = other
			a.Panes[a.Active].clampCursor(true)
			a.Panes[a.Active].ensureVisible(rows, bpr)
			a.Message = a.editPrompt()
			a.HasMsg = true
		}
	case k.Code == keyLeft:
		a.moveCursor(-1)
	case k.Code == keyRight:
		a.moveCursor(1)
	case k.Code == keyUp:
		a.moveCursor(-int64(bpr))
	case k.Code == keyDown:
		a.moveCursor(int64(bpr))
	case k.Code == keyPageUp:
		a.moveCursor(-int64(maxInt(rows, 1) * maxInt(bpr, 16)))
	case k.Code == keyPageDown:
		a.moveCursor(int64(maxInt(rows, 1) * maxInt(bpr, 16)))
	case k.Code == keyHome:
		a.moveHome()
	case k.Code == keyEnd:
		a.moveEnd()
	case k.Code == keyBackspace:
		p := a.activePane()
		if p.Cursor > 0 {
			p.Cursor--
		}
		p.ensureVisible(rows, bpr)
	case k.Code == keyEnter:
		if !a.Options.HexInput {
			a.editByte(0x0d)
		}
	case k.Code == keyRune && !k.Ctrl:
		if a.Options.HexInput {
			v := hexDigit(k.Rune)
			if v >= 0 {
				if a.Edit.has {
					hi := a.Edit.nibble
					a.Edit.has = false
					a.editByte(hi<<4 | byte(v))
				} else {
					a.Edit.nibble = byte(v)
					a.Edit.has = true
				}
			}
		} else {
			bytes, err := encodeText(string(k.Rune), a.Options.Ebcdic)
			if err != nil {
				a.setMessage(err.Error())
			} else if len(bytes) > 0 {
				a.editByte(bytes[0])
			}
		}
	}
}

func (a *App) editByte(value byte) {
	idx := a.Active
	rows := a.RowsPerPane
	bpr := a.BytesPerRow
	if a.Panes[idx].File == nil {
		a.setMessage("No file to edit")
		return
	}
	p := a.Panes[idx]
	if err := p.File.WriteOffset(p.Cursor, value); err != nil {
		a.setMessage("Error writing file: " + err.Error())
		return
	}
	p.Cursor++
	p.ensureVisible(rows, bpr)
}

func (a *App) startCompare() {
	if a.Panes[0].File == nil || a.Panes[1].File == nil {
		a.setMessage("No file to compare")
		return
	}
	a.CompareMode = true
	a.Last = &lastAction{Kind: lastCompare}
	a.doCompare(false)
}

// doCompare finds the next difference.  advance is false for a fresh Compare
// command (start at the cursor) and true for Next (step past the difference
// already displayed).
func (a *App) doCompare(advance bool) {
	if a.Panes[0].File == nil || a.Panes[1].File == nil {
		a.setMessage("No file to compare")
		return
	}
	cursor := a.activePane().Cursor
	forward := a.Options.Forward
	var start uint64
	if !advance {
		start = cursor
	} else if forward {
		start = cursor + 1
	} else if cursor == 0 {
		a.setMessage("No difference found before end of file")
		return
	} else {
		start = cursor - 1
	}
	active := a.Active
	var offset uint64
	var found bool
	var err error
	if active == 0 {
		offset, found, err = FindDifference(a.Panes[0].File, a.Panes[1].File, start, forward)
	} else {
		offset, found, err = FindDifference(a.Panes[1].File, a.Panes[0].File, start, forward)
	}
	if err != nil {
		a.setMessage("Error reading file: " + err.Error())
		return
	}
	if !found {
		a.setMessage("No difference found before end of file")
		return
	}
	a.CompareMode = true
	bpr := uint64(maxInt(a.BytesPerRow, 16))
	for _, pane := range a.Panes {
		// A length difference is reported at the first byte past the shorter
		// file, so allow the cursor to sit at len.
		pane.Cursor = offset
		if pane.Len() < pane.Cursor {
			pane.Cursor = pane.Len()
		}
		if pane.File != nil {
			pane.Top = (offset / bpr) * bpr
			pane.ensureVisible(a.RowsPerPane, int(bpr))
		}
	}
	a.clearMessage()
}

func (a *App) repeatLast() {
	if a.Last == nil {
		a.setMessage("No compare/search to repeat")
		return
	}
	if a.Last.Kind == lastSearch {
		a.performSearch(a.Last.Needle, true)
	} else {
		a.doCompare(true)
	}
}

func (a *App) performSearch(needle []byte, advance bool) {
	if len(needle) == 0 {
		a.setMessage("Search requires a string")
		return
	}
	forward := a.Options.Forward
	cursor := a.activePane().Cursor
	var start uint64
	if !advance {
		start = cursor
	} else if forward {
		start = cursor + 1
	} else if cursor == 0 {
		a.setMessage("No match found before end of file")
		return
	} else {
		start = cursor - 1
	}
	file := a.activePane().File
	if file == nil {
		a.setMessage("No file to search")
		return
	}
	offset, found, err := Find(file, needle, start, forward, a.Options.RespectCase, a.Options.Ebcdic)
	if err != nil {
		a.setMessage("Error reading file: " + err.Error())
		return
	}
	if !found {
		a.setMessage("No match found before end of file")
		return
	}
	p := a.activePane()
	p.Cursor = offset
	p.ensureVisible(a.RowsPerPane, a.BytesPerRow)
	a.clearMessage()
}

func (a *App) writeConfirm() {
	p := a.activePane()
	if p.File == nil {
		a.setMessage("No file to edit")
		return
	}
	if !p.File.Dirty() {
		a.setMessage("No changes to write")
		return
	}
	if p.File.ReadOnly() {
		a.setMessage("File is read-only - can't write")
		return
	}
	a.Modal = &Modal{
		Kind:    modalConfirm,
		Title:   "WRITE",
		Text:    fmt.Sprintf("Do you want to write changes to %s (Y/N)?", p.File.Path()),
		Confirm: confirmWrite,
	}
}

func (a *App) doWrite() {
	p := a.activePane()
	if p.File == nil {
		a.setMessage("No file to edit")
		return
	}
	length, err := p.File.Save()
	if err != nil {
		a.setMessage(err.Error())
		return
	}
	a.setMessage(fmt.Sprintf("%d bytes written", length))
}

func (a *App) tryQuit() {
	if a.anyDirty() {
		a.Modal = &Modal{
			Kind:    modalConfirm,
			Title:   "QUIT",
			Text:    "Changes lost.  Quit without writing changes? (Y/N)",
			Confirm: confirmQuit,
		}
	} else {
		a.Quit = true
	}
}

func (a *App) loadInto(path string, pane int) {
	if a.Panes[pane].Dirty() {
		a.Modal = &Modal{
			Kind:    modalConfirm,
			Title:   "REPLACE",
			Text:    fmt.Sprintf("Changes to %s not written.  Replace it? (Y/N)", a.Panes[pane].Name()),
			Confirm: confirmLoad,
			Path:    path,
			Pane:    pane,
		}
		return
	}
	a.loadIntoForce(path, pane)
}

func (a *App) loadIntoForce(path string, pane int) {
	p, err := openPane(path)
	if err != nil {
		a.setMessage("Error opening file: " + err.Error())
		return
	}
	a.Panes[pane] = p
	a.CompareMode = false
	a.clearMessage()
}

func (a *App) submitInput(action inputAction, value string) {
	switch action {
	case inputAdd, inputSub, inputGoto:
		amount, err := parseOffset(value)
		if err != nil {
			a.setMessage(err.Error())
			return
		}
		p := a.activePane()
		if p.File == nil {
			a.setMessage("No file to edit")
			return
		}
		length := p.Len()
		var max uint64
		if a.Edit != nil {
			max = length
		} else if length > 0 {
			max = length - 1
		}
		switch action {
		case inputAdd:
			p.Cursor += amount
			if p.Cursor > max {
				p.Cursor = max
			}
		case inputSub:
			if p.Cursor > amount {
				p.Cursor -= amount
			} else {
				p.Cursor = 0
			}
		case inputGoto:
			p.Cursor = amount
			if p.Cursor > max {
				p.Cursor = max
			}
		}
		p.ensureVisible(a.RowsPerPane, a.BytesPerRow)
		a.clearMessage()
	case inputSearch:
		if strings.TrimSpace(value) == "" {
			a.setMessage("Search requires a string")
			return
		}
		var needle []byte
		var err error
		if a.Options.HexInput {
			needle, err = parseHex(value)
		} else {
			needle, err = encodeText(value, a.Options.Ebcdic)
		}
		if err != nil {
			a.setMessage(err.Error())
			return
		}
		if len(needle) == 0 {
			a.setMessage("Search requires a string")
			return
		}
		a.Last = &lastAction{Kind: lastSearch, Needle: append([]byte(nil), needle...)}
		a.performSearch(needle, false)
	case inputView:
		if strings.TrimSpace(value) == "" {
			a.setMessage("View requires a filespec")
			return
		}
		path := strings.TrimSpace(value)
		if !filepath.IsAbs(path) {
			if cwd, err := os.Getwd(); err == nil {
				path = filepath.Join(cwd, path)
			}
		}
		if info, err := os.Stat(path); err == nil && info.IsDir() {
			a.setMessage("Error opening file: is a directory")
		} else {
			a.loadInto(path, a.Active)
		}
	}
}

func (a *App) handleModalKey(k Key) {
	m := a.Modal
	if m == nil {
		return
	}
	a.Modal = nil
	switch m.Kind {
	case modalConfirm:
		switch {
		case k.Code == keyRune && (k.Rune == 'y' || k.Rune == 'Y'), k.Code == keyEnter:
			switch m.Confirm {
			case confirmQuit:
				a.Quit = true
			case confirmWrite:
				a.doWrite()
			case confirmLoad:
				a.loadIntoForce(m.Path, m.Pane)
			}
		case k.Code == keyRune && (k.Rune == 'n' || k.Rune == 'N'), k.Code == keyEsc:
			// leave the modal closed
		default:
			a.Modal = m
		}
	case modalInput:
		switch {
		case k.Code == keyEsc:
			// closed
		case k.Code == keyEnter:
			a.submitInput(m.Input, m.Value)
		case k.Code == keyBackspace:
			m.Value = trimLastRune(m.Value)
			a.Modal = m
		case k.Code == keyRune && k.Ctrl && (k.Rune == 'u' || k.Rune == 'U'):
			m.Value = ""
			a.Modal = m
		case k.Code == keyRune && !k.Ctrl:
			m.Value += string(k.Rune)
			a.Modal = m
		default:
			a.Modal = m
		}
	case modalOptions:
		const n = 11
		switch {
		case k.Code == keyEsc:
			// closed
		case k.Code == keyUp:
			m.Index = (m.Index + n - 1) % n
			a.Modal = m
		case k.Code == keyDown, k.Code == keyTab:
			m.Index = (m.Index + 1) % n
			a.Modal = m
		case k.Code == keyEnter, k.Code == keyRune && k.Rune == ' ':
			a.activateOption(m.Index)
		case k.Code == keyRune:
			a.optionLetter(k.Rune)
		default:
			a.Modal = m
		}
	case modalModes:
		const n = 5
		switch {
		case k.Code == keyEsc:
			// closed
		case k.Code == keyUp:
			m.Index = (m.Index + n - 1) % n
			a.Modal = m
		case k.Code == keyDown, k.Code == keyTab:
			m.Index = (m.Index + 1) % n
			a.Modal = m
		case k.Code == keyEnter, k.Code == keyRune && k.Rune == ' ':
			a.activateMode(m.Index)
		case k.Code == keyRune:
			switch k.Rune {
			case 'a', 'A':
				a.activateMode(0)
			case 'c', 'C':
				a.activateMode(1)
			case 'h', 'H':
				a.activateMode(2)
			case 'm', 'M':
				a.activateMode(3)
			case 'q', 'Q':
				a.activateMode(4)
			default:
				a.Modal = m
			}
		default:
			a.Modal = m
		}
	case modalAttributes:
		const n = 3
		switch {
		case k.Code == keyEsc:
			// closed
		case k.Code == keyUp:
			m.Index = (m.Index + n - 1) % n
			a.Modal = m
		case k.Code == keyDown, k.Code == keyTab:
			m.Index = (m.Index + 1) % n
			a.Modal = m
		case k.Code == keyEnter, k.Code == keyRune && k.Rune == ' ':
			switch m.Index {
			case 0:
				a.Options.Theme = ThemeClassic
			case 1:
				a.Options.Theme = ThemeAmber
			default:
				a.Options.Theme = ThemeMono
			}
		default:
			a.Modal = m
		}
	case modalHelp:
		max := len(helpLines) - 1
		switch {
		case k.Code == keyEsc, k.Code == keyRune && (k.Rune == 'q' || k.Rune == 'Q'), k.isF(1):
			// closed
		case k.Code == keyUp:
			if m.Scroll > 0 {
				m.Scroll--
			}
			a.Modal = m
		case k.Code == keyDown:
			m.Scroll++
			if m.Scroll > max {
				m.Scroll = max
			}
			a.Modal = m
		case k.Code == keyPageUp:
			m.Scroll -= 20
			if m.Scroll < 0 {
				m.Scroll = 0
			}
			a.Modal = m
		case k.Code == keyPageDown:
			m.Scroll += 20
			if m.Scroll > max {
				m.Scroll = max
			}
			a.Modal = m
		case k.Code == keyHome:
			m.Scroll = 0
			a.Modal = m
		case k.Code == keyEnd:
			m.Scroll = max
			a.Modal = m
		default:
			a.Modal = m
		}
	case modalDir:
		a.handleDirKey(m, k)
	}
}

func (a *App) handleDirKey(m *Modal, k Key) {
	b := m.Dir
	const page = 15
	if b.PathInput != nil {
		switch k.Code {
		case keyEsc:
			b.PathInput = nil
		case keyEnter:
			b.ApplyPathInput()
		case keyBackspace:
			s := *b.PathInput
			s = trimLastRune(s)
			b.PathInput = &s
		case keyRune:
			if !k.Ctrl {
				s := *b.PathInput + string(k.Rune)
				b.PathInput = &s
			}
		}
		a.Modal = m
		return
	}

	if b.SortPrompt {
		switch {
		case k.Code == keyEsc:
			b.SortPrompt = false
		case k.Code == keyRune:
			switch k.Rune {
			case 'n', 'N':
				b.SetSort(SortName)
			case 'e', 'E':
				b.SetSort(SortExt)
			case 'd', 'D':
				b.SetSort(SortDate)
			case 's', 'S':
				b.SetSort(SortSize)
			}
		}
		a.Modal = m
		return
	}

	switch {
	case k.Code == keyEsc:
		// closed
	case k.Code == keyUp:
		b.MoveBy(-1)
		b.EnsureVisible(page)
		a.Modal = m
	case k.Code == keyDown:
		b.MoveBy(1)
		b.EnsureVisible(page)
		a.Modal = m
	case k.Code == keyPageUp:
		b.MoveBy(-page)
		b.EnsureVisible(page)
		a.Modal = m
	case k.Code == keyPageDown:
		b.MoveBy(page)
		b.EnsureVisible(page)
		a.Modal = m
	case k.Code == keyHome:
		b.Home()
		b.EnsureVisible(page)
		a.Modal = m
	case k.Code == keyEnd:
		b.End()
		b.EnsureVisible(page)
		a.Modal = m
	case k.Code == keyBackspace:
		b.Parent()
		a.Modal = m
	case k.Code == keyTab:
		b.ToggleTarget()
		a.Modal = m
	case k.Code == keyRune && (k.Rune == 's' || k.Rune == 'S'):
		b.SortPrompt = true
		a.Modal = m
	case k.Code == keyRune && k.Rune == '/':
		b.BeginPathInput()
		a.Modal = m
	case k.Code == keyEnter:
		target := b.TargetPane
		view := k.Ctrl
		if view {
			if path := b.CurrentPath(); path != "" {
				a.loadInto(path, 1-target)
			} else {
				a.Modal = m
			}
		} else {
			if path, ok := b.Activate(); ok {
				a.loadInto(path, target)
			} else {
				a.Modal = m
			}
		}
	default:
		a.Modal = m
	}
}

func (a *App) activateOption(index int) {
	switch index {
	case 0:
		a.Options.Ebcdic = false
	case 1:
		a.Options.Forward = false
	case 2:
		a.Options.Conceal = true
	case 3:
		a.Options.Ebcdic = true
	case 4:
		a.Options.Forward = true
	case 5:
		a.openHelp()
	case 6:
		a.Options.RespectCase = false
	case 7:
		return // Quit to main menu
	case 8:
		a.Options.RespectCase = true
	case 9:
		a.Options.Conceal = false
	case 10:
		a.Options.HexInput = !a.Options.HexInput
	}
	if a.Modal == nil {
		a.Modal = &Modal{Kind: modalOptions, Index: index}
	}
}

func (a *App) optionLetter(c rune) {
	switch c {
	case 'a', 'A':
		a.Options.Ebcdic = false
	case 'e', 'E':
		a.Options.Ebcdic = true
	case 'b', 'B':
		a.Options.Forward = false
	case 'f', 'F':
		a.Options.Forward = true
	case 'c', 'C':
		a.Options.Conceal = true
	case 's', 'S':
		a.Options.Conceal = false
	case 'i', 'I':
		a.Options.RespectCase = false
	case 'r', 'R':
		a.Options.RespectCase = true
	case 'x', 'X':
		a.Options.HexInput = !a.Options.HexInput
	case 'h', 'H':
		a.openHelp()
	case 'q', 'Q':
		return
	}
	if a.Modal == nil {
		a.Modal = &Modal{Kind: modalOptions, Index: 0}
	}
}

func (a *App) activateMode(index int) {
	switch index {
	case 0:
		a.Modal = &Modal{Kind: modalAttributes, Index: 0}
	case 1:
		a.Options.Theme = ThemeClassic
		a.Modal = &Modal{Kind: modalModes, Index: 1}
	case 2:
		a.openHelp()
	case 3:
		a.Options.Theme = ThemeMono
		a.Modal = &Modal{Kind: modalModes, Index: 3}
	case 4:
		a.Modal = nil // Quit closes the Mode menu
	}
}

func trimLastRune(s string) string {
	_, size := utf8.DecodeLastRuneInString(s)
	if size <= 0 {
		return s
	}
	return s[:len(s)-size]
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}
