package main

import "testing"

func texts(cpu Cpu, bytes []byte) []string {
	insns := disassemble(cpu, bytes)
	out := make([]string, len(insns))
	for i, insn := range insns {
		out[i] = insn.Text
	}
	return out
}

func TestX64DecodesAKnownInstruction(t *testing.T) {
	inst := disassemble(CpuX64, []byte{0x48, 0x31, 0xC0})
	if len(inst) != 1 {
		t.Fatalf("got %d instructions", len(inst))
	}
	if inst[0].Offset != 0 || inst[0].Len != 3 {
		t.Fatalf("offset/len = %d/%d", inst[0].Offset, inst[0].Len)
	}
	if inst[0].Text != "xor rax, rax" {
		t.Fatalf("text = %q", inst[0].Text)
	}
}

func TestX86DecodesAKnownInstruction(t *testing.T) {
	inst := disassemble(CpuX86, []byte{0x33, 0xC0})
	if len(inst) != 1 || inst[0].Len != 2 || inst[0].Text != "xor eax, eax" {
		t.Fatalf("instructions = %+v", inst)
	}
}

func TestInvalidX86ByteBecomesADirective(t *testing.T) {
	inst := disassemble(CpuX64, []byte{0x0F})
	if len(inst) != 1 {
		t.Fatalf("got %d instructions", len(inst))
	}
	if inst[0].Len != 1 || inst[0].Text != ".byte 0F" {
		t.Fatalf("instruction = %+v", inst[0])
	}
}

func TestARMAndThumbDecodeNOPs(t *testing.T) {
	arm := disassemble(CpuArm, []byte{0x00, 0x00, 0xA0, 0xE1})
	if len(arm) != 1 || arm[0].Len != 4 {
		t.Fatalf("ARM instructions = %+v", arm)
	}
	if want := "mov r0, r0"; arm[0].Text != want {
		t.Fatalf("ARM text = %q, want %q", arm[0].Text, want)
	}

	thumb := disassemble(CpuThumb, []byte{0xC0, 0x46})
	if len(thumb) != 1 || thumb[0].Len != 2 {
		t.Fatalf("Thumb instructions = %+v", thumb)
	}
	if thumb[0].Text != "mov r8, r8" && thumb[0].Text != "nop" {
		t.Fatalf("Thumb text = %q", thumb[0].Text)
	}
}

func TestARM64DecodesANOP(t *testing.T) {
	inst := disassemble(CpuArm64, []byte{0x1F, 0x20, 0x03, 0xD5})
	if len(inst) != 1 || inst[0].Len != 4 || inst[0].Text != "nop" {
		t.Fatalf("instructions = %+v", inst)
	}
}

func TestPartialBytesAreKept(t *testing.T) {
	inst := disassemble(CpuArm, []byte{0x00})
	if len(inst) != 1 || inst[0].Len != 1 {
		t.Fatalf("instructions = %+v", inst)
	}
	if len(inst[0].Text) < 5 || inst[0].Text[:5] != ".byte" {
		t.Fatalf("text = %q", inst[0].Text)
	}
}

func TestCPUCycleReturnsToTheCharacterView(t *testing.T) {
	if got, ok := CpuX86.nextView(); !ok || got != CpuX64 {
		t.Fatalf("x86.next = %v, %v", got, ok)
	}
	if _, ok := CpuArm64.nextView(); ok {
		t.Fatal("ARM64 should not have a next view")
	}
	if got, ok := parseCpu("AARCH64"); !ok || got != CpuArm64 {
		t.Fatalf("parseCpu(AARCH64) = %v, %v", got, ok)
	}
	if _, ok := parseCpu("nope"); ok {
		t.Fatal("parseCpu(nope) should fail")
	}
	if got := texts(CpuX64, nil); len(got) != 0 {
		t.Fatalf("empty decode = %v", got)
	}
}
