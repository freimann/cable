// Non-interactive I/O benchmark shared with the Rust and C++ ports.

package main

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"
)

const fnvOffset uint64 = 14695981039346656037
const fnvPrime uint64 = 1099511628211

func fnvUpdate(value uint64, data []byte) uint64 {
	for _, b := range data {
		value ^= uint64(b)
		value *= fnvPrime
	}
	return value
}

func benchEmit(phase string, start time.Time, bytes, checksum uint64) {
	ms := float64(time.Since(start).Nanoseconds()) / 1e6
	fmt.Printf("%s,%.3f,%d,%d\n", phase, ms, bytes, checksum)
}

func locateNeedle(file *PagedFile) ([]byte, bool, error) {
	const marker = "CABLE-NEEDLE-1234"
	length := file.Len()
	start := uint64(0)
	if length > 4096 {
		start = length - 4096
	}
	tail, err := file.ReadRange(start, int(length-start))
	if err != nil {
		return nil, false, err
	}
	for i := 0; i+len(marker) <= len(tail); i++ {
		if string(tail[i:i+len(marker)]) == marker {
			out := make([]byte, len(marker))
			copy(out, marker)
			return out, true, nil
		}
	}
	return nil, false, nil
}

func runBench(paths []string) error {
	if len(paths) == 0 || len(paths) > 2 {
		return fmt.Errorf("usage: cable --bench FILE1 [FILE2]")
	}

	fmt.Println("phase,milliseconds,bytes,checksum")

	openStart := time.Now()
	file1, err := openPagedFile(paths[0])
	if err != nil {
		return fmt.Errorf("Error opening file %s: %v", paths[0], err)
	}
	defer file1.file.Close()
	length1 := file1.Len()
	benchEmit("open", openStart, length1, length1)

	scanStart := time.Now()
	checksum := fnvOffset
	var scanned uint64
	pos := uint64(0)
	for pos < length1 {
		want := length1 - pos
		if want > 64*1024 {
			want = 64 * 1024
		}
		data, err := file1.ReadRange(pos, int(want))
		if err != nil {
			return err
		}
		if len(data) == 0 {
			break
		}
		checksum = fnvUpdate(checksum, data)
		scanned += uint64(len(data))
		pos += uint64(len(data))
	}
	benchEmit("scan", scanStart, scanned, checksum)

	searchStart := time.Now()
	needle, haveNeedle, err := locateNeedle(file1)
	if err != nil {
		return err
	}
	var found uint64 = ^uint64(0)
	if haveNeedle {
		offset, ok, err := Find(file1, needle, 0, true, true, false)
		if err != nil {
			return err
		}
		if ok {
			found = offset
		}
	}
	benchEmit("search", searchStart, length1, found)

	if len(paths) >= 2 {
		file2, err := openPagedFile(paths[1])
		if err != nil {
			return fmt.Errorf("Error opening file %s: %v", paths[1], err)
		}
		defer file2.file.Close()
		compareBytes := file1.Len()
		if file2.Len() < compareBytes {
			compareBytes = file2.Len()
		}
		compareStart := time.Now()
		offset, ok, err := FindDifference(file1, file2, 0, true)
		if err != nil {
			return err
		}
		checksum := ^uint64(0)
		if ok {
			checksum = offset
		}
		benchEmit("compare", compareStart, compareBytes, checksum)
	} else {
		benchEmit("compare", time.Now(), 0, ^uint64(0))
	}

	temp := filepath.Join(filepath.Dir(paths[0]),
		fmt.Sprintf(".cable-bench-%d-%d.tmp", os.Getpid(), time.Now().UnixNano()))
	if err := copyFile(paths[0], temp); err != nil {
		return fmt.Errorf("could not create benchmark copy: %v", err)
	}
	saveFile, err := openPagedFile(temp)
	if err != nil {
		os.Remove(temp)
		return err
	}
	saveBytes := saveFile.Len()
	original, _, err := saveFile.ReadOffset(0)
	if err != nil {
		saveFile.file.Close()
		os.Remove(temp)
		return err
	}
	saveStart := time.Now()
	if err := saveFile.WriteOffset(0, original^0xff); err != nil {
		saveFile.file.Close()
		os.Remove(temp)
		return err
	}
	saved, err := saveFile.Save()
	if err != nil {
		saveFile.file.Close()
		os.Remove(temp)
		return err
	}
	benchEmit("save", saveStart, saveBytes, saved)
	saveFile.file.Close()
	os.Remove(temp)
	return nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0666)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		os.Remove(dst)
		return err
	}
	if err := out.Sync(); err != nil {
		out.Close()
		os.Remove(dst)
		return err
	}
	return out.Close()
}
