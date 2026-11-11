//go:build windows

package main

import (
	"syscall"
	"unsafe"
)

var (
	kernel32                       = syscall.NewLazyDLL("kernel32.dll")
	procSetConsoleMode             = kernel32.NewProc("SetConsoleMode")
	procGetConsoleScreenBufferInfo = kernel32.NewProc("GetConsoleScreenBufferInfo")
	procGetConsoleOutputCP         = kernel32.NewProc("GetConsoleOutputCP")
	procSetConsoleOutputCP         = kernel32.NewProc("SetConsoleOutputCP")
)

type coord struct {
	X int16
	Y int16
}

type smallRect struct {
	Left   int16
	Top    int16
	Right  int16
	Bottom int16
}

type consoleScreenBufferInfo struct {
	Size              coord
	CursorPosition    coord
	Attributes        uint16
	Window            smallRect
	MaximumWindowSize coord
}

const (
	enableProcessedInput            = 0x0001
	enableLineInput                 = 0x0002
	enableEchoInput                 = 0x0004
	enableWindowInput               = 0x0008
	enableProcessedOutput           = 0x0001
	enableVirtualTerminalInput      = 0x0200
	enableVirtualTerminalProcessing = 0x0004
	enableExtendedFlags             = 0x0080
)

func setConsoleMode(handle syscall.Handle, mode uint32) error {
	r1, _, err := procSetConsoleMode.Call(uintptr(handle), uintptr(mode))
	if r1 == 0 {
		return err
	}
	return nil
}

func makeRaw() (func(), error) {
	in, err := syscall.GetStdHandle(syscall.STD_INPUT_HANDLE)
	if err != nil {
		return nil, err
	}
	out, err := syscall.GetStdHandle(syscall.STD_OUTPUT_HANDLE)
	if err != nil {
		return nil, err
	}
	var inMode, outMode uint32
	if err := syscall.GetConsoleMode(in, &inMode); err != nil {
		return nil, err
	}
	if err := syscall.GetConsoleMode(out, &outMode); err != nil {
		return nil, err
	}

	newIn := inMode &^ (enableProcessedInput | enableLineInput | enableEchoInput | enableWindowInput)
	newIn |= enableVirtualTerminalInput | enableExtendedFlags
	newOut := outMode | enableProcessedOutput | enableVirtualTerminalProcessing
	if err := setConsoleMode(in, newIn); err != nil {
		return nil, err
	}
	if err := setConsoleMode(out, newOut); err != nil {
		setConsoleMode(in, inMode)
		return nil, err
	}

	oldCP, _, _ := procGetConsoleOutputCP.Call()
	procSetConsoleOutputCP.Call(65001)

	return func() {
		if oldCP != 0 {
			procSetConsoleOutputCP.Call(oldCP)
		}
		setConsoleMode(in, inMode)
		setConsoleMode(out, outMode)
	}, nil
}

func terminalSize() (int, int, error) {
	out, err := syscall.GetStdHandle(syscall.STD_OUTPUT_HANDLE)
	if err != nil {
		return 80, 25, nil
	}
	var info consoleScreenBufferInfo
	r1, _, _ := procGetConsoleScreenBufferInfo.Call(uintptr(out), uintptr(unsafe.Pointer(&info)))
	if r1 == 0 {
		return 80, 25, nil
	}
	w := int(info.Window.Right-info.Window.Left) + 1
	h := int(info.Window.Bottom-info.Window.Top) + 1
	if w < 1 || h < 1 {
		return 80, 25, nil
	}
	return w, h, nil
}
