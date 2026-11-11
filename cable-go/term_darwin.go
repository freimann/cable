//go:build darwin

package main

import (
	"syscall"
	"unsafe"
)

func getTermios(fd uintptr) (*syscall.Termios, error) {
	var t syscall.Termios
	if err := ioctl(fd, syscall.TIOCGETA, unsafe.Pointer(&t)); err != nil {
		return nil, err
	}
	return &t, nil
}

func setTermios(fd uintptr, t *syscall.Termios) error {
	return ioctl(fd, syscall.TIOCSETA, unsafe.Pointer(t))
}
