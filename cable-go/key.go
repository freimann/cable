// Keyboard events and terminal key decoding.

package main

import (
	"os"
	"time"
	"unicode/utf8"
)

type keyCode int

const (
	keyRune keyCode = iota
	keyUp
	keyDown
	keyLeft
	keyRight
	keyHome
	keyEnd
	keyPageUp
	keyPageDown
	keyInsert
	keyDelete
	keyEnter
	keyEsc
	keyBackspace
	keyTab
	keyBackTab
	keyF1
	keyF2
	keyF3
	keyF4
	keyF5
	keyF6
	keyF7
	keyF8
	keyF9
	keyF10
	keyF11
	keyF12
	keyScrollLock
)

// Key is one decoded keyboard event.
type Key struct {
	Code  keyCode
	Rune  rune
	Ctrl  bool
	Alt   bool
	Shift bool
}

func (k Key) isF(n int) bool {
	return int(k.Code) == int(keyF1)+n-1
}

func runeKey(r rune, ctrl bool) Key {
	return Key{Code: keyRune, Rune: r, Ctrl: ctrl}
}

type KeyReader struct {
	ch chan byte
}

func newKeyReader(f *os.File) *KeyReader {
	ch := make(chan byte, 4096)
	go func() {
		buf := make([]byte, 256)
		for {
			n, err := f.Read(buf)
			for i := 0; i < n; i++ {
				ch <- buf[i]
			}
			if err != nil {
				close(ch)
				return
			}
		}
	}()
	return &KeyReader{ch: ch}
}

func (r *KeyReader) next(d time.Duration) (byte, bool) {
	if d <= 0 {
		b, ok := <-r.ch
		return b, ok
	}
	select {
	case b, ok := <-r.ch:
		return b, ok
	case <-time.After(d):
		return 0, false
	}
}

func (r *KeyReader) ReadKey() (Key, bool) {
	b, ok := r.next(0)
	if !ok {
		return Key{}, false
	}
	if b == 0x1b {
		c, ok := r.next(30 * time.Millisecond)
		if !ok {
			return Key{Code: keyEsc}, true
		}
		switch c {
		case '[':
			return r.readCSI(), true
		case 'O':
			return r.readSS3(), true
		default:
			// Alt+key.  The clone has no Alt bindings, so deliver the key
			// without the modifier.
			return decodeByte(c, r), true
		}
	}
	return decodeByte(b, r), true
}

func decodeByte(b byte, r *KeyReader) Key {
	switch b {
	case 9:
		return Key{Code: keyTab}
	case 13:
		return Key{Code: keyEnter}
	case 8, 127:
		return Key{Code: keyBackspace}
	case 12:
		// Ctrl+L may arrive as a bare form feed.  The application accepts it
		// as the Scroll Lock alias.
		return runeKey('\f', false)
	}
	if b > 0 && b < 27 {
		return runeKey(rune('a'+b-1), true)
	}
	if b >= 0x20 && b < 0x80 {
		return runeKey(rune(b), false)
	}
	if b >= 0x80 {
		buf := []byte{b}
		for !utf8.FullRune(buf) && len(buf) < 4 {
			nb, ok := r.next(20 * time.Millisecond)
			if !ok {
				break
			}
			buf = append(buf, nb)
		}
		if ch, _ := utf8.DecodeRune(buf); ch != utf8.RuneError {
			return runeKey(ch, false)
		}
	}
	return Key{}
}

func (r *KeyReader) readCSI() Key {
	params := []byte{}
	for {
		b, ok := r.next(30 * time.Millisecond)
		if !ok {
			return Key{Code: keyEsc}
		}
		if b >= 0x40 && b <= 0x7e {
			return csiKey(string(params), b)
		}
		if len(params) < 16 {
			params = append(params, b)
		}
	}
}

func csiKey(params string, final byte) Key {
	switch final {
	case 'A':
		return Key{Code: keyUp}
	case 'B':
		return Key{Code: keyDown}
	case 'C':
		return Key{Code: keyRight}
	case 'D':
		return Key{Code: keyLeft}
	case 'H':
		return Key{Code: keyHome}
	case 'F':
		return Key{Code: keyEnd}
	case 'Z':
		return Key{Code: keyBackTab}
	case 'P':
		return Key{Code: keyF1}
	case 'Q':
		return Key{Code: keyF2}
	case 'R':
		return Key{Code: keyF3}
	case 'S':
		return Key{Code: keyF4}
	case '~':
		n := 0
		for _, ch := range params {
			if ch < '0' || ch > '9' {
				break
			}
			n = n*10 + int(ch-'0')
		}
		switch n {
		case 1, 7:
			return Key{Code: keyHome}
		case 2:
			return Key{Code: keyInsert}
		case 3:
			return Key{Code: keyDelete}
		case 4, 8:
			return Key{Code: keyEnd}
		case 5:
			return Key{Code: keyPageUp}
		case 6:
			return Key{Code: keyPageDown}
		case 11:
			return Key{Code: keyF1}
		case 12:
			return Key{Code: keyF2}
		case 13:
			return Key{Code: keyF3}
		case 14:
			return Key{Code: keyF4}
		case 15:
			return Key{Code: keyF5}
		case 17:
			return Key{Code: keyF6}
		case 18:
			return Key{Code: keyF7}
		case 19:
			return Key{Code: keyF8}
		case 20:
			return Key{Code: keyF9}
		case 21:
			return Key{Code: keyF10}
		case 23:
			return Key{Code: keyF11}
		case 24:
			return Key{Code: keyF12}
		}
	}
	return Key{Code: keyEsc}
}

func (r *KeyReader) readSS3() Key {
	b, ok := r.next(30 * time.Millisecond)
	if !ok {
		return Key{Code: keyEsc}
	}
	switch b {
	case 'A':
		return Key{Code: keyUp}
	case 'B':
		return Key{Code: keyDown}
	case 'C':
		return Key{Code: keyRight}
	case 'D':
		return Key{Code: keyLeft}
	case 'H':
		return Key{Code: keyHome}
	case 'F':
		return Key{Code: keyEnd}
	case 'P':
		return Key{Code: keyF1}
	case 'Q':
		return Key{Code: keyF2}
	case 'R':
		return Key{Code: keyF3}
	case 'S':
		return Key{Code: keyF4}
	}
	return Key{Code: keyEsc}
}
