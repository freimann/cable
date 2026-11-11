//go:build linux

package main

import (
	"syscall"
	"unsafe"
)

func getTermios(fd uintptr) (*syscall.Termios, error) {
	var t syscall.Termios
	if err := ioctl(fd, syscall.TCGETS, unsafe.Pointer(&t)); err != nil {
		return nil, err
	}
	return &t, nil
}

func setTermios(fd uintptr, t *syscall.Termios) error {
	return ioctl(fd, syscall.TCSETS, unsafe.Pointer(t))
}
