package main

import (
	"bytes"
	"testing"
)

func TestCP037RoundTripForPrintableASCII(t *testing.T) {
	for b := 0x20; b <= 0x7e; b++ {
		c := cp037[b]
		if enc, ok := encodeCP037(c); ok {
			if cp037[enc] != c {
				t.Fatalf("round trip failed for byte %02X", b)
			}
		}
	}
}

func TestEBCDICTextRoundTrip(t *testing.T) {
	needle, err := encodeText("Hello, World!", true)
	if err != nil {
		t.Fatal(err)
	}
	if needle[0] != 0xC8 {
		t.Fatalf("H encoded as %02X", needle[0])
	}
	if needle[5] != 0x6B {
		t.Fatalf(", encoded as %02X", needle[5])
	}
	if needle[6] != 0x40 {
		t.Fatalf("space encoded as %02X", needle[6])
	}
	if cp037[needle[7]] != 'W' {
		t.Fatalf("W decoded as %q", cp037[needle[7]])
	}
}

func TestParseHexIgnoresBlanks(t *testing.T) {
	got, err := parseHex("de ad   BE EF")
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, []byte{0xde, 0xad, 0xbe, 0xef}) {
		t.Fatalf("parse = % X", got)
	}
	if _, err := parseHex("abc"); err == nil {
		t.Fatal("odd number of digits should fail")
	}
	if _, err := parseHex("zz"); err == nil {
		t.Fatal("invalid digits should fail")
	}
}

func TestParseOffsets(t *testing.T) {
	cases := []struct {
		in   string
		want uint64
	}{
		{"123", 123},
		{"H1f", 31},
		{"0x20", 32},
		{" H 10 ", 16},
	}
	for _, c := range cases {
		got, err := parseOffset(c.in)
		if err != nil || got != c.want {
			t.Fatalf("parseOffset(%q) = %d, %v; want %d", c.in, got, err, c.want)
		}
	}
	if _, err := parseOffset(""); err == nil {
		t.Fatal("empty offset should fail")
	}
	if _, err := parseOffset("nope"); err == nil {
		t.Fatal("invalid offset should fail")
	}
}

func TestConcealReplacesControls(t *testing.T) {
	cases := []struct {
		b       byte
		ebcdic  bool
		conceal bool
		want    rune
	}{
		{0x00, false, true, '.'},
		{'A', false, true, 'A'},
		{0x00, false, false, ' '},
		{0x40, true, true, ' '},
	}
	for _, c := range cases {
		if got := displayByte(c.b, c.ebcdic, c.conceal); got != c.want {
			t.Fatalf("displayByte(%02X, %v, %v) = %q, want %q", c.b, c.ebcdic, c.conceal, got, c.want)
		}
	}
}

func TestASCIIUsesCodePage437Glyphs(t *testing.T) {
	cases := []struct {
		b       byte
		conceal bool
		want    rune
	}{
		{0xE9, true, '\u0398'},
		{0xFA, true, '\u00B7'},
		{0xFF, true, '\u00A0'},
		{0x5C, true, '\\'},
		{0x18, true, '.'},
		{0x18, false, '\u2191'},
	}
	for _, c := range cases {
		if got := displayByte(c.b, false, c.conceal); got != c.want {
			t.Fatalf("displayByte(%02X, conceal=%v) = %q, want %q", c.b, c.conceal, got, c.want)
		}
	}
}
