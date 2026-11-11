// Character encodings and byte/string conversion helpers.
//
// CABLE could treat data as ASCII or EBCDIC.  The EBCDIC code page used here
// is IBM code page 037, which is the international EBCDIC variant that was
// current on the IBM systems CABLE was written for.

package main

import (
	"fmt"
	"strconv"
	"strings"
	"unicode"
)

// encodeCP037 returns the byte encoding of one Unicode scalar, if the table
// contains it.
func encodeCP037(c rune) (byte, bool) {
	for i, r := range cp037 {
		if r == c {
			return byte(i), true
		}
	}
	return 0, false
}

// displayByte returns a safe, printable representation of a byte.  conceal
// mirrors CABLE's Conceal/Show state: control bytes become '.' while
// concealing and render as the code page's graphic glyphs when showing.
func displayByte(b byte, ebcdic, conceal bool) rune {
	if ebcdic {
		c := cp037[b]
		u := uint32(c)
		if u < 0x20 || u == 0x7f {
			if conceal {
				return '.'
			}
			return cp437[u&0xff]
		}
		return c
	}
	if b < 0x20 || b == 0x7f {
		if conceal {
			return '.'
		}
		return cp437[b]
	}
	return cp437[b]
}

// foldByte folds one data byte for case-insensitive comparison.
func foldByte(b byte, ebcdic bool) byte {
	if ebcdic {
		c := cp037[b]
		upper := unicode.ToUpper(c)
		if v, ok := encodeCP037(upper); ok {
			return v
		}
		return b
	}
	if b >= 'a' && b <= 'z' {
		return b - 0x20
	}
	return b
}

// encodeText converts typed text into the byte representation used for
// searching.
func encodeText(s string, ebcdic bool) ([]byte, error) {
	out := make([]byte, 0, len(s))
	for _, c := range s {
		if ebcdic {
			b, ok := encodeCP037(c)
			if !ok {
				return nil, fmt.Errorf("Character %q cannot be encoded as EBCDIC", c)
			}
			out = append(out, b)
		} else {
			if c > 0xff {
				return nil, fmt.Errorf("Character %q cannot be encoded as ASCII", c)
			}
			out = append(out, byte(c))
		}
	}
	return out, nil
}

// parseHex parses a hexadecimal string, ignoring spaces and tabs.
func parseHex(s string) ([]byte, error) {
	digits := make([]byte, 0, len(s))
	for _, c := range s {
		if unicode.IsSpace(c) {
			continue
		}
		v := hexDigit(c)
		if v < 0 {
			return nil, fmt.Errorf("Invalid HEX digit %q", c)
		}
		digits = append(digits, byte(v))
	}
	if len(digits)%2 != 0 {
		return nil, fmt.Errorf("HEX string must contain an even number of digits")
	}
	out := make([]byte, len(digits)/2)
	for i := 0; i < len(out); i++ {
		out[i] = digits[2*i]<<4 | digits[2*i+1]
	}
	return out, nil
}

func hexDigit(c rune) int {
	switch {
	case c >= '0' && c <= '9':
		return int(c - '0')
	case c >= 'a' && c <= 'f':
		return int(c-'a') + 10
	case c >= 'A' && c <= 'F':
		return int(c-'A') + 10
	}
	return -1
}

// parseOffset parses an offset typed at the +, - or = prompt.  A leading H or
// 0x means hexadecimal, as in the original CABLE prompt ("if HEX start with
// H").  Everything else is decimal.
func parseOffset(s string) (uint64, error) {
	t := strings.TrimSpace(s)
	if t == "" {
		return 0, fmt.Errorf("No value entered")
	}
	radix := 10
	digits := t
	if strings.HasPrefix(t, "H") || strings.HasPrefix(t, "h") {
		radix = 16
		digits = t[1:]
	} else if strings.HasPrefix(t, "0x") || strings.HasPrefix(t, "0X") {
		radix = 16
		digits = t[2:]
	}
	var b strings.Builder
	for _, c := range digits {
		if !unicode.IsSpace(c) {
			b.WriteRune(c)
		}
	}
	digits = b.String()
	if digits == "" {
		return 0, fmt.Errorf("No value entered")
	}
	v, err := strconv.ParseUint(digits, radix, 64)
	if err != nil {
		return 0, fmt.Errorf("Invalid number %q", t)
	}
	return v, nil
}
