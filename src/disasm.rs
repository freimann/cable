//! Passive instruction disassembly for the right-hand character column.
//!
//! The original CABLE only knows two character interpretations, ASCII and
//! EBCDIC.  The clone can additionally show the bytes of a row as machine
//! instructions.  This is a display aid only: editing stays byte-oriented and
//! is disabled while a disassembly view is active.
//!
//! Decoding always starts at the first byte of the supplied slice, which is
//! one displayed row.  An instruction that crosses a row boundary is therefore
//! decoded from its first byte in the next row again; the original layout has
//! no notion of instruction boundaries, so this keeps the display stable while
//! scrolling.

use std::fmt::Write as _;

use yaxpeax_arch::{Decoder, LengthedInstruction, Reader, U8Reader};

/// Instruction sets offered by the side view, in cycling order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cpu {
    X86,
    X64,
    Arm,
    Thumb,
    Arm64,
}

impl Cpu {
    /// Short name shown in the status and function-key lines.
    pub fn label(self) -> &'static str {
        match self {
            Cpu::X86 => "x86",
            Cpu::X64 => "x64",
            Cpu::Arm => "ARM",
            Cpu::Thumb => "Thumb",
            Cpu::Arm64 => "ARM64",
        }
    }

    /// Next view in the cycle, or `None` to return to the character view.
    pub fn next(self) -> Option<Self> {
        match self {
            Cpu::X86 => Some(Cpu::X64),
            Cpu::X64 => Some(Cpu::Arm),
            Cpu::Arm => Some(Cpu::Thumb),
            Cpu::Thumb => Some(Cpu::Arm64),
            Cpu::Arm64 => None,
        }
    }

    /// Parse a command-line/side-view name.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "x86" | "i386" | "32" => Some(Cpu::X86),
            "x64" | "x86-64" | "amd64" | "64" => Some(Cpu::X64),
            "arm" | "armv7" => Some(Cpu::Arm),
            "thumb" | "thumb2" => Some(Cpu::Thumb),
            "arm64" | "aarch64" | "armv8" => Some(Cpu::Arm64),
            _ => None,
        }
    }
}

/// One decoded instruction, with `offset` and `len` relative to the start of
/// the decoded slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Insn {
    pub offset: usize,
    pub len: usize,
    pub text: String,
}

/// Decode as many complete instructions as `bytes` allows.  Undecodable bytes
/// are emitted as an assembler directive of one native instruction slot so the
/// decoder stays aligned.
pub fn disassemble(cpu: Cpu, bytes: &[u8]) -> Vec<Insn> {
    match cpu {
        Cpu::X86 => x86(bytes, false),
        Cpu::X64 => x86(bytes, true),
        Cpu::Arm => arm(bytes, false),
        Cpu::Thumb => arm(bytes, true),
        Cpu::Arm64 => arm64(bytes),
    }
}

fn x86(bytes: &[u8], long_mode: bool) -> Vec<Insn> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let long = yaxpeax_x86::long_mode::InstDecoder::default();
    let protected = yaxpeax_x86::protected_mode::InstDecoder::default();
    while pos < bytes.len() {
        let slice = &bytes[pos..];
        let decoded = if long_mode {
            long.decode_slice(slice)
                .ok()
                .map(|inst| (inst.to_string(), inst.len().to_const() as usize))
        } else {
            protected
                .decode_slice(slice)
                .ok()
                .map(|inst| (inst.to_string(), inst.len().to_const() as usize))
        };
        let (text, len) = match decoded {
            Some((text, len)) if len > 0 && len <= slice.len() => (text, len),
            _ => (format!(".byte {:02X}", slice[0]), 1),
        };
        out.push(Insn {
            offset: pos,
            len,
            text,
        });
        pos += len;
    }
    out
}

fn arm(bytes: &[u8], thumb: bool) -> Vec<Insn> {
    let mut decoder = yaxpeax_arm::armv7::InstDecoder::default();
    decoder.set_thumb_mode(thumb);
    decode_arch::<yaxpeax_arm::armv7::ARMv7>(&decoder, bytes, thumb)
}

fn arm64(bytes: &[u8]) -> Vec<Insn> {
    let decoder = yaxpeax_arm::armv8::a64::InstDecoder::default();
    decode_arch::<yaxpeax_arm::armv8::a64::ARMv8>(&decoder, bytes, false)
}

/// ARM and AArch64 expose their instruction stream through the generic
/// `yaxpeax_arch::Decoder` trait.  A failed decode is skipped by one native
/// instruction slot so the rest of the row remains aligned.
fn decode_arch<A>(decoder: &A::Decoder, bytes: &[u8], thumb: bool) -> Vec<Insn>
where
    A: yaxpeax_arch::Arch,
    A::Instruction: std::fmt::Display,
    A::Address: Into<u64>,
    for<'a> U8Reader<'a>: Reader<A::Address, A::Word>,
{
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let mut reader = U8Reader::new(&bytes[pos..]);
        match decoder.decode(&mut reader) {
            Ok(inst) => {
                let len: usize = reader.total_offset().into() as usize;
                if len == 0 || len > bytes.len() - pos {
                    out.push(raw_word(bytes, pos, thumb));
                    pos += out.last().map(|i| i.len).unwrap_or(1);
                } else {
                    out.push(Insn {
                        offset: pos,
                        len,
                        text: inst.to_string(),
                    });
                    pos += len;
                }
            }
            Err(_) => {
                out.push(raw_word(bytes, pos, thumb));
                pos += out.last().map(|i| i.len).unwrap_or(1);
            }
        }
    }
    out
}

/// A directive for bytes that do not decode.  ARM instructions are aligned to
/// four bytes and Thumb to two, so one whole slot is consumed where possible.
fn raw_word(bytes: &[u8], pos: usize, thumb: bool) -> Insn {
    let rest = &bytes[pos..];
    if thumb && rest.len() >= 2 {
        let word = u16::from_le_bytes([rest[0], rest[1]]);
        Insn {
            offset: pos,
            len: 2,
            text: format!(".hword 0x{word:04X}"),
        }
    } else if !thumb && rest.len() >= 4 {
        let word = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
        Insn {
            offset: pos,
            len: 4,
            text: format!(".word 0x{word:08X}"),
        }
    } else {
        let mut text = String::from(".byte");
        let mut used = 0;
        for b in rest {
            if used == 4 || write!(text, " {b:02X}").is_err() {
                break;
            }
            used += 1;
        }
        Insn {
            offset: pos,
            len: rest.len().min(4).max(1),
            text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(cpu: Cpu, bytes: &[u8]) -> Vec<String> {
        disassemble(cpu, bytes)
            .into_iter()
            .map(|i| i.text)
            .collect()
    }

    #[test]
    fn x64_decodes_a_known_instruction() {
        let inst = disassemble(Cpu::X64, &[0x48, 0x31, 0xC0]);
        assert_eq!(inst.len(), 1);
        assert_eq!(inst[0].offset, 0);
        assert_eq!(inst[0].len, 3);
        assert_eq!(inst[0].text, "xor rax, rax");
    }

    #[test]
    fn x86_decodes_a_known_instruction() {
        let inst = disassemble(Cpu::X86, &[0x33, 0xC0]);
        assert_eq!(inst.len(), 1);
        assert_eq!(inst[0].len, 2);
        assert_eq!(inst[0].text, "xor eax, eax");
    }

    #[test]
    fn invalid_x86_byte_becomes_a_directive() {
        let inst = disassemble(Cpu::X64, &[0x0F]);
        assert_eq!(inst.len(), 1);
        assert_eq!(inst[0].len, 1);
        assert_eq!(inst[0].text, ".byte 0F");
    }

    #[test]
    fn arm_and_thumb_decode_nops() {
        let arm = disassemble(Cpu::Arm, &[0x00, 0x00, 0xA0, 0xE1]);
        assert_eq!(arm.len(), 1);
        assert_eq!(arm[0].len, 4);
        assert!(arm[0].text.contains("mov"));

        let thumb = disassemble(Cpu::Thumb, &[0xC0, 0x46]);
        assert_eq!(thumb.len(), 1);
        assert_eq!(thumb[0].len, 2);
        assert!(thumb[0].text.contains("nop") || thumb[0].text.contains("mov"));
    }

    #[test]
    fn arm64_decodes_a_nop() {
        let inst = disassemble(Cpu::Arm64, &[0x1F, 0x20, 0x03, 0xD5]);
        assert_eq!(inst.len(), 1);
        assert_eq!(inst[0].len, 4);
        assert_eq!(inst[0].text, "nop");
    }

    #[test]
    fn partial_bytes_are_kept() {
        let inst = disassemble(Cpu::Arm, &[0x00]);
        assert_eq!(inst.len(), 1);
        assert_eq!(inst[0].len, 1);
        assert!(inst[0].text.starts_with(".byte"));
    }

    #[test]
    fn cpu_cycle_returns_to_the_character_view() {
        assert_eq!(Cpu::X86.next(), Some(Cpu::X64));
        assert_eq!(Cpu::Arm64.next(), None);
        assert_eq!(Cpu::parse("AARCH64"), Some(Cpu::Arm64));
        assert_eq!(Cpu::parse("nope"), None);
        assert_eq!(texts(Cpu::X64, &[]), Vec::<String>::new());
    }
}
