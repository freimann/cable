// CABLE-GO: a modern, cross-platform reimplementation of the DOS program
// `CABLE - Compare And Browse with Limited Editing`.

package main

import (
	"fmt"
	"os"
	"strings"
	"time"
)

const version = "0.2.4"

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "cable: %v\n", err)
		os.Exit(1)
	}
}

func run() error {
	if len(os.Args) > 1 && os.Args[1] == "--bench" {
		return runBench(os.Args[2:])
	}
	options := defaultOptions()
	paths := []string{}

	for _, arg := range os.Args[1:] {
		switch {
		case arg == "-h" || arg == "--help":
			usage()
			return nil
		case arg == "-V" || arg == "--version":
			fmt.Printf("cable %s (Go)\n", version)
			return nil
		case arg == "--mono":
			options.Theme = ThemeMono
		case arg == "--colour" || arg == "--color":
			options.Theme = ThemeClassic
		case arg == "--amber":
			options.Theme = ThemeAmber
		case arg == "--ebcdic":
			options.Ebcdic = true
		case arg == "--ascii":
			options.Ebcdic = false
		case arg == "--show":
			options.Conceal = false
		case arg == "--conceal":
			options.Conceal = true
		case arg == "--ignore-case":
			options.RespectCase = false
		case arg == "--respect-case":
			options.RespectCase = true
		case arg == "--hex":
			options.HexInput = true
		case arg == "--character":
			options.HexInput = false
		case arg == "--backward":
			options.Forward = false
		case arg == "--forward":
			options.Forward = true
		case arg == "--both":
			options.MoveBoth = true
		case arg == "--active":
			options.MoveBoth = false
		case arg == "--x86":
			c := CpuX86
			options.Disasm = &c
		case arg == "--x64" || arg == "--amd64":
			c := CpuX64
			options.Disasm = &c
		case arg == "--arm":
			c := CpuArm
			options.Disasm = &c
		case arg == "--thumb":
			c := CpuThumb
			options.Disasm = &c
		case arg == "--arm64" || arg == "--aarch64":
			c := CpuArm64
			options.Disasm = &c
		case arg == "--text" || arg == "--ascii-view":
			options.Disasm = nil
		case strings.HasPrefix(arg, "--disasm="):
			name := strings.TrimPrefix(arg, "--disasm=")
			c, ok := parseCpu(name)
			if !ok {
				usage()
				return fmt.Errorf("unknown disassembly target %s", name)
			}
			options.Disasm = &c
		case strings.HasPrefix(arg, "-"):
			usage()
			return fmt.Errorf("unknown option %s", arg)
		default:
			paths = append(paths, arg)
		}
	}

	if len(paths) > 2 {
		usage()
		return fmt.Errorf("at most two files can be opened")
	}

	app, err := newApp(paths, options)
	if err != nil {
		return err
	}

	term, err := startTerminal()
	if err != nil {
		return err
	}
	defer term.Close()

	reader := newKeyReader(os.Stdin)
	keys := make(chan Key)
	go func() {
		for {
			k, ok := reader.ReadKey()
			if !ok {
				close(keys)
				return
			}
			keys <- k
		}
	}()

	ticker := time.NewTicker(200 * time.Millisecond)
	defer ticker.Stop()

	lastW, lastH := 0, 0
	dirty := true
	for !app.Quit {
		if dirty {
			w, h, _ := terminalSize()
			if err := redraw(app); err != nil {
				return err
			}
			lastW, lastH = w, h
			dirty = false
		}
		select {
		case k, ok := <-keys:
			if !ok {
				return nil
			}
			app.handleKey(k)
			dirty = true
		case <-ticker.C:
			if w, h, _ := terminalSize(); w != lastW || h != lastH {
				dirty = true
			}
		}
	}
	return nil
}

func usage() {
	const body = ` - Compare And Browse with Limited Editing

Usage: cable [OPTIONS] [FILE1 [FILE2]]

With no files, a directory browser is opened.

Options:
  --mono            start in mono
  --colour, --color start in colour (default)
  --amber           start in amber
  --ascii           treat data as ASCII (default)
  --ebcdic          treat data as EBCDIC (CP037)
  --conceal         show unprintables as '.' (default)
  --show            show unprintables as control pictures
  --respect-case    case-sensitive search/compare (default)
  --ignore-case     case-insensitive search/compare
  --hex             HEX search/edit input
  --character       character search/edit input (default)
  --forward         compare/search forwards (default)
  --backward        compare/search backwards
  --active          scroll only the active window (default)
  --both            scroll both windows together
  --text            show ASCII/EBCDIC in the right-hand column (default)
  --x86, --x64      disassemble the right-hand column as x86 / x86-64
  --arm, --thumb    disassemble as ARMv7 / Thumb
  --arm64           disassemble as AArch64
  --disasm=CPU      same as the flags above (CPU: x86, x64, arm, thumb, arm64)
  --bench           run the non-interactive I/O benchmark
  -h, --help        show this help
  -V, --version     show version`
	fmt.Printf("cable %s (Go port of cable-rs)%s\n", version, body)
}
