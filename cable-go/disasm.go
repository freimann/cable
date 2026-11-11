// Passive instruction disassembly for the right-hand character column.
//
// The original CABLE only knows two character interpretations, ASCII and
// EBCDIC.  The clone can additionally show the bytes of a row as machine
// instructions.  This is a display aid only: editing stays byte-oriented and
// is disabled while a disassembly view is active.
//
// Decoding always starts at the first byte of the supplied slice, which is
// one displayed row.  An instruction that crosses a row boundary is therefore
// decoded from its first byte in the next row again; the original layout has
// no notion of instruction boundaries, so this keeps the display stable while
// scrolling.

package main

import (
	"fmt"
	"regexp"
	"strings"

	"github.com/loov/disasm/thumbasm"
	"golang.org/x/arch/arm/armasm"
	"golang.org/x/arch/arm64/arm64asm"
	"golang.org/x/arch/x86/x86asm"
)

var (
	reArmCond    = regexp.MustCompile(`\.(al|eq|ne|hs|cs|lo|cc|mi|pl|vs|vc|hi|ls|ge|lt|gt|le)\b`)
	reArmAlias   = regexp.MustCompile(`\br(9|11|12)\b`)
	reArmBranch  = regexp.MustCompile(`\bpc([+-])0x([0-9a-f]+)`)
	reBracketed  = regexp.MustCompile(`\[[^\]]*\]`)
	reBracketNum = regexp.MustCompile(`(,\s*)(-?\d+)\b`)
	reBracketHex = regexp.MustCompile(`(,\s*#?)(-?\d+)\b`)
	reSubReg     = regexp.MustCompile(`,\s*\+r`)
	reArmShift   = regexp.MustCompile(`([^,\s])\s+(lsl|lsr|asr|ror|rrx)\s+(\d+)`)
	reSpace      = regexp.MustCompile(`\s+`)
	reRetX30     = regexp.MustCompile(`^ret x30$`)
	reDotRel     = regexp.MustCompile(`\.([+-])`)
	reThumbLit   = regexp.MustCompile(`^(ldr\w*)\s+(r\d+),\s+(0x[0-9a-f]+)$`)
	reThumbNum   = regexp.MustCompile(`(,\s*)(-?\d+)\b`)
	reThumbBr    = regexp.MustCompile(`^(b|bl|blx|b[a-z]{2})\s+(0x[0-9a-f]+)$`)
)

// normalizeText applies the small amount of formatting that the reference
// decoder (yaxpeax) and golang.org/x/arch disagree on.  Instruction selection
// and operand text can still differ for uncommon encodings.
func normalizeText(cpu Cpu, text string) string {
	switch cpu {
	case CpuX86, CpuX64:
		return normalizeX86(text)
	case CpuArm:
		return normalizeArm(text)
	case CpuThumb:
		return normalizeThumb(text)
	default:
		return normalizeArm64(text)
	}
}

func normalizeX86(text string) string {
	text = strings.ReplaceAll(text, " ptr", "")
	// x86asm writes the current instruction as "." while yaxpeax uses "$".
	text = reDotRel.ReplaceAllString(text, "$$$1")
	return text
}

func collapseSpace(text string) string {
	return strings.TrimSpace(reSpace.ReplaceAllString(text, " "))
}

func normalizeArm(text string) string {
	text = strings.ToLower(strings.TrimSpace(text))
	text = strings.ReplaceAll(text, "#", "")
	text = strings.ReplaceAll(text, ",", ", ")
	text = reArmCond.ReplaceAllStringFunc(text, func(m string) string {
		cond := m[1:]
		switch cond {
		case "al":
			return ""
		case "cs":
			return "hs"
		case "cc":
			return "lo"
		}
		return cond
	})
	text = reArmAlias.ReplaceAllStringFunc(text, func(m string) string {
		switch m {
		case "r9":
			return "sb"
		case "r11":
			return "fp"
		default:
			return "ip"
		}
	})
	text = reSubReg.ReplaceAllString(text, ", r")
	text = reArmShift.ReplaceAllStringFunc(text, func(m string) string {
		loc := reArmShift.FindStringSubmatch(m)
		if loc[2] == "rrx" {
			return loc[1] + ", rrx"
		}
		return loc[1] + ", " + loc[2] + " " + loc[3]
	})
	text = reBracketed.ReplaceAllStringFunc(text, func(part string) string {
		return reBracketNum.ReplaceAllStringFunc(part, func(m string) string {
			loc := reBracketNum.FindStringSubmatch(m)
			var v int64
			fmt.Sscanf(loc[2], "%d", &v)
			return loc[1] + fmt.Sprintf("0x%x", v)
		})
	})
	text = reArmBranch.ReplaceAllStringFunc(text, func(m string) string {
		sub := reArmBranch.FindStringSubmatch(m)
		var v int64
		fmt.Sscanf(sub[2], "%x", &v)
		if sub[1] == "-" {
			v = -v
		}
		v += 8
		if v < 0 {
			return fmt.Sprintf("$-0x%x", -v)
		}
		return fmt.Sprintf("$+0x%x", v)
	})
	return collapseSpace(text)
}

func normalizeThumb(text string) string {
	text = strings.ToLower(strings.TrimSpace(text))
	text = strings.ReplaceAll(text, "#", "")
	text = strings.ReplaceAll(text, ",", ", ")
	text = reThumbNum.ReplaceAllStringFunc(text, func(m string) string {
		loc := reThumbNum.FindStringSubmatch(m)
		var v int64
		fmt.Sscanf(loc[2], "%d", &v)
		if v < 0 {
			return loc[1] + fmt.Sprintf("-0x%x", -v)
		}
		return loc[1] + fmt.Sprintf("0x%x", v)
	})
	if sub := reThumbLit.FindStringSubmatch(text); sub != nil {
		var target uint64
		fmt.Sscanf(sub[3], "0x%x", &target)
		off := target - 4
		if off == 0 {
			text = sub[1] + " " + sub[2] + ", [pc]"
		} else {
			text = fmt.Sprintf("%s %s, [pc, 0x%x]", sub[1], sub[2], off)
		}
		return text
	}
	if sub := reThumbBr.FindStringSubmatch(text); sub != nil {
		var raw uint64
		fmt.Sscanf(sub[2], "0x%x", &raw)
		target := int64(int32(uint32(raw)))
		mnemonic := sub[1]
		switch mnemonic {
		case "bl", "blx":
			mnemonic += ".w"
			target -= 4
		case "b":
			target -= 4
		default:
			// beq/bne/... use a two-byte PC bias in the reference display.
			target -= 2
		}
		if target < 0 {
			return fmt.Sprintf("%s $-0x%x", mnemonic, -target)
		}
		return fmt.Sprintf("%s $+0x%x", mnemonic, target)
	}
	return collapseSpace(text)
}

func normalizeArm64(text string) string {
	text = strings.ToLower(strings.TrimSpace(text))
	text = strings.ReplaceAll(text, ",", ", ")
	text = reRetX30.ReplaceAllString(text, "ret")
	text = reDotRel.ReplaceAllString(text, "$$$1")
	text = reBracketed.ReplaceAllStringFunc(text, func(part string) string {
		return reBracketHex.ReplaceAllStringFunc(part, func(m string) string {
			loc := reBracketHex.FindStringSubmatch(m)
			var v int64
			fmt.Sscanf(loc[2], "%d", &v)
			if v < 0 {
				return loc[1] + fmt.Sprintf("-0x%x", -v)
			}
			return loc[1] + fmt.Sprintf("0x%x", v)
		})
	})
	return collapseSpace(text)
}

// Cpu is an instruction set offered by the side view, in cycling order.
type Cpu int

const (
	CpuX86 Cpu = iota
	CpuX64
	CpuArm
	CpuThumb
	CpuArm64
)

// Label is the short name shown in the status and function-key lines.
func (c Cpu) Label() string {
	switch c {
	case CpuX86:
		return "x86"
	case CpuX64:
		return "x64"
	case CpuArm:
		return "ARM"
	case CpuThumb:
		return "Thumb"
	default:
		return "ARM64"
	}
}

// nextView returns the next view in the cycle, or false to return to the
// character view.
func (c Cpu) nextView() (Cpu, bool) {
	switch c {
	case CpuX86:
		return CpuX64, true
	case CpuX64:
		return CpuArm, true
	case CpuArm:
		return CpuThumb, true
	case CpuThumb:
		return CpuArm64, true
	default:
		return 0, false
	}
}

// parseCpu parses a command-line/side-view name.
func parseCpu(name string) (Cpu, bool) {
	switch strings.ToLower(name) {
	case "x86", "i386", "32":
		return CpuX86, true
	case "x64", "x86-64", "amd64", "64":
		return CpuX64, true
	case "arm", "armv7":
		return CpuArm, true
	case "thumb", "thumb2":
		return CpuThumb, true
	case "arm64", "aarch64", "armv8":
		return CpuArm64, true
	}
	return 0, false
}

// Insn is one decoded instruction, with offset and len relative to the start
// of the decoded slice.
type Insn struct {
	Offset int
	Len    int
	Text   string
}

// disassemble decodes as many complete instructions as bytes allows.
// Undecodable bytes are emitted as an assembler directive of one native
// instruction slot so the decoder stays aligned.
func disassemble(cpu Cpu, bytes []byte) []Insn {
	switch cpu {
	case CpuX86:
		return disasmX86(bytes, false)
	case CpuX64:
		return disasmX86(bytes, true)
	case CpuArm:
		return disasmArm(bytes)
	case CpuThumb:
		return disasmThumb(bytes)
	default:
		return disasmArm64(bytes)
	}
}

func disasmX86(bytes []byte, longMode bool) []Insn {
	mode := 32
	if longMode {
		mode = 64
	}
	out := []Insn{}
	pos := 0
	for pos < len(bytes) {
		slice := bytes[pos:]
		inst, err := x86asm.Decode(slice, mode)
		text := ""
		if err == nil && inst.Len > 0 && inst.Len <= len(slice) {
			text = x86asm.IntelSyntax(inst, 0, nil)
			// x86asm returns pseudo-instructions such as "prefix(0xf)" for
			// byte sequences yaxpeax rejects; show those as directives.
			if strings.HasPrefix(text, "prefix(") || strings.Contains(text, "(bad)") {
				text = ""
			}
		}
		length := 1
		if text == "" {
			text = fmt.Sprintf(".byte %02X", slice[0])
		} else {
			text = normalizeText(CpuX86, text)
			length = inst.Len
		}
		out = append(out, Insn{Offset: pos, Len: length, Text: text})
		pos += length
	}
	return out
}

func disasmArm(bytes []byte) []Insn {
	out := []Insn{}
	pos := 0
	for pos < len(bytes) {
		rest := bytes[pos:]
		if len(rest) < 4 {
			insn := rawWord(rest, false)
			insn.Offset = pos
			out = append(out, insn)
			pos += insn.Len
			continue
		}
		inst, err := armasm.Decode(rest[:4], armasm.ModeARM)
		if err != nil || inst.Len <= 0 {
			insn := rawWord(rest, false)
			insn.Offset = pos
			out = append(out, insn)
			pos += insn.Len
			continue
		}
		text := normalizeText(CpuArm, inst.String())
		out = append(out, Insn{Offset: pos, Len: inst.Len, Text: text})
		pos += inst.Len
	}
	return out
}

func disasmThumb(bytes []byte) []Insn {
	decoder := &thumbasm.Decoder{}
	out := []Insn{}
	pos := 0
	for pos < len(bytes) {
		rest := bytes[pos:]
		inst, err := decoder.Decode(rest, 0)
		if err != nil || inst.Len <= 0 || inst.Len > len(rest) {
			insn := rawWord(rest, true)
			insn.Offset = pos
			out = append(out, insn)
			pos += insn.Len
			continue
		}
		text := normalizeText(CpuThumb, inst.Text)
		out = append(out, Insn{Offset: pos, Len: inst.Len, Text: text})
		pos += inst.Len
	}
	return out
}

func disasmArm64(bytes []byte) []Insn {
	out := []Insn{}
	pos := 0
	for pos < len(bytes) {
		rest := bytes[pos:]
		if len(rest) < 4 {
			insn := rawWord(rest, false)
			insn.Offset = pos
			out = append(out, insn)
			pos += insn.Len
			continue
		}
		inst, err := arm64asm.Decode(rest[:4])
		if err != nil {
			if text, ok := lenientArm64(rest[:4]); ok {
				out = append(out, Insn{Offset: pos, Len: 4, Text: text})
				pos += 4
				continue
			}
			insn := rawWord(rest, false)
			insn.Offset = pos
			out = append(out, insn)
			pos += insn.Len
			continue
		}
		text := normalizeText(CpuArm64, inst.String())
		out = append(out, Insn{Offset: pos, Len: 4, Text: text})
		pos += 4
	}
	return out
}

// lenientArm64 handles a couple of encodings that the reference decoder
// accepts but golang.org/x/arch rejects: the UDF space and shifted-register
// ORR with an out-of-range shift count.
func lenientArm64(wordBytes []byte) (string, bool) {
	word := uint32(wordBytes[0]) | uint32(wordBytes[1])<<8 | uint32(wordBytes[2])<<16 | uint32(wordBytes[3])<<24
	if word&0xFFFF0000 == 0 {
		return fmt.Sprintf("udf #0x%x", word&0xFFFF), true
	}
	if word&0x7F200000 == 0x2A000000 {
		rd := word & 31
		rn := (word >> 5) & 31
		rm := (word >> 16) & 31
		imm6 := (word >> 10) & 63
		shift := [...]string{"lsl", "lsr", "asr", "ror"}[(word>>22)&3]
		return fmt.Sprintf("orr w%d, w%d, w%d, %s #%d", rd, rn, rm, shift, imm6), true
	}
	return "", false
}

// rawWord is a directive for bytes that do not decode.  ARM instructions are
// aligned to four bytes and Thumb to two, so one whole slot is consumed where
// possible.
func rawWord(rest []byte, thumb bool) Insn {
	if thumb && len(rest) >= 2 {
		word := uint16(rest[0]) | uint16(rest[1])<<8
		return Insn{Len: 2, Text: fmt.Sprintf(".hword 0x%04X", word)}
	}
	if !thumb && len(rest) >= 4 {
		word := uint32(rest[0]) | uint32(rest[1])<<8 | uint32(rest[2])<<16 | uint32(rest[3])<<24
		return Insn{Len: 4, Text: fmt.Sprintf(".word 0x%08X", word)}
	}
	text := ".byte"
	used := 0
	for _, b := range rest {
		if used == 4 {
			break
		}
		text += fmt.Sprintf(" %02X", b)
		used++
	}
	length := len(rest)
	if length > 4 {
		length = 4
	}
	if length < 1 {
		length = 1
	}
	return Insn{Len: length, Text: text}
}
