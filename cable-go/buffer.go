// Paged, editable file storage.
//
// CABLE handled files through a small number of 16 KiB buffers.  The clone
// keeps the same "work on a large file without loading it all" property by
// reading 64 KiB pages on demand and keeping edits in an overlay.  Only the
// edited pages and the overlay are kept in memory.

package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"time"
)

const pageSize uint64 = 64 * 1024
const ioChunk = 64 * 1024

type PagedFile struct {
	path     string
	file     *os.File
	length   uint64
	pages    map[uint64][]byte
	edits    map[uint64]byte
	readOnly bool
	dirty    bool
}

func openPagedFile(path string) (*PagedFile, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	info, err := f.Stat()
	if err != nil {
		f.Close()
		return nil, err
	}
	return &PagedFile{
		path:     path,
		file:     f,
		length:   uint64(info.Size()),
		pages:    make(map[uint64][]byte),
		edits:    make(map[uint64]byte),
		readOnly: isReadOnly(info.Mode()),
	}, nil
}

// isReadOnly reports whether the file permissions deny writing.  On Unix this
// matches libstdc++'s readonly() (no write bits); on Windows Go maps the DOS
// read-only attribute to a 0444 permission mask.
func isReadOnly(mode os.FileMode) bool {
	return mode.Perm()&0222 == 0
}

func (f *PagedFile) Path() string   { return f.path }
func (f *PagedFile) Len() uint64    { return f.length }
func (f *PagedFile) ReadOnly() bool { return f.readOnly }
func (f *PagedFile) Dirty() bool    { return f.dirty }

func (f *PagedFile) page(page uint64) ([]byte, error) {
	if data, ok := f.pages[page]; ok {
		return data, nil
	}
	start := page * pageSize
	if start >= f.length {
		f.pages[page] = []byte{}
		return f.pages[page], nil
	}
	want := f.length - start
	if want > pageSize {
		want = pageSize
	}
	buf := make([]byte, want)
	if _, err := f.file.ReadAt(buf, int64(start)); err != nil {
		return nil, err
	}
	f.pages[page] = buf
	return buf, nil
}

// rawByte reads one byte without applying the edit overlay.
func (f *PagedFile) rawByte(offset uint64) (byte, error) {
	page := offset / pageSize
	within := offset % pageSize
	data, err := f.page(page)
	if err != nil {
		return 0, err
	}
	if within >= uint64(len(data)) {
		return 0, io.ErrUnexpectedEOF
	}
	return data[within], nil
}

// ReadOffset reads one byte, applying the edit overlay.  The bool reports
// whether the offset lies inside the file.
func (f *PagedFile) ReadOffset(offset uint64) (byte, bool, error) {
	if offset >= f.length {
		return 0, false, nil
	}
	if b, ok := f.edits[offset]; ok {
		return b, true, nil
	}
	b, err := f.rawByte(offset)
	if err != nil {
		return 0, false, err
	}
	return b, true, nil
}

// ReadRange reads up to n bytes, applying the edit overlay.  The returned
// slice is shortened at end of file.
func (f *PagedFile) ReadRange(offset uint64, n int) ([]byte, error) {
	if offset >= f.length || n <= 0 {
		return []byte{}, nil
	}
	want := f.length - offset
	if uint64(n) < want {
		want = uint64(n)
	}
	out := make([]byte, 0, want)
	pos := offset
	end := offset + want
	for pos < end {
		pageNo := pos / pageSize
		within := pos % pageSize
		data, err := f.page(pageNo)
		if err != nil {
			return nil, err
		}
		available := uint64(len(data))
		if within >= available {
			break
		}
		available -= within
		take := end - pos
		if available < take {
			take = available
		}
		out = append(out, data[within:within+take]...)
		pos += take
	}
	for off, val := range f.edits {
		if off >= offset && off < end {
			i := off - offset
			if i < uint64(len(out)) {
				out[i] = val
			}
		}
	}
	return out, nil
}

// WriteOffset writes one byte into the edit overlay.  Writing at length appends.
func (f *PagedFile) WriteOffset(offset uint64, value byte) error {
	if offset > f.length {
		return errors.New("offset past end of file")
	}
	if offset == f.length {
		f.edits[offset] = value
		f.length++
	} else {
		original, err := f.rawByte(offset)
		if err != nil {
			return err
		}
		if value == original {
			delete(f.edits, offset)
		} else {
			f.edits[offset] = value
		}
	}
	f.dirty = len(f.edits) > 0
	return nil
}

// Save writes the edit overlay to a temporary copy and then renames it over
// the original, keeping the original intact if an error occurs part way
// through.
func (f *PagedFile) Save() (uint64, error) {
	if !f.dirty {
		return 0, errors.New("No changes to write")
	}
	if f.readOnly {
		return 0, errors.New("File is read-only - can't write")
	}

	tmp := f.tempPath()
	if err := f.writeCopy(tmp); err != nil {
		os.Remove(tmp)
		return 0, err
	}

	if runtime.GOOS == "windows" {
		// os.Rename does not replace an existing file on Windows.
		if err := os.Remove(f.path); err != nil && !os.IsNotExist(err) {
			os.Remove(tmp)
			return 0, err
		}
	}
	if err := os.Rename(tmp, f.path); err != nil {
		os.Remove(tmp)
		return 0, err
	}

	// The open handle still refers to the replaced file on Unix, so reopen the
	// path before serving any further reads.
	file, err := os.Open(f.path)
	if err != nil {
		return 0, err
	}
	info, err := file.Stat()
	if err != nil {
		file.Close()
		return 0, err
	}
	f.file.Close()
	f.file = file
	f.length = uint64(info.Size())
	f.readOnly = isReadOnly(info.Mode())
	f.pages = make(map[uint64][]byte)
	f.edits = make(map[uint64]byte)
	f.dirty = false
	return f.length, nil
}

func (f *PagedFile) tempPath() string {
	name := filepath.Base(f.path)
	dir := filepath.Dir(f.path)
	return filepath.Join(dir, fmt.Sprintf(".%s.cable-go-%d-%d.tmp", name, os.Getpid(), time.Now().UnixNano()))
}

func (f *PagedFile) writeCopy(tmp string) error {
	src, err := os.Open(f.path)
	if err != nil {
		return err
	}
	defer src.Close()
	dst, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0666)
	if err != nil {
		return err
	}
	defer dst.Close()

	buf := make([]byte, ioChunk)
	pos := uint64(0)
	written := uint64(0)
	for pos < f.length {
		want := f.length - pos
		if want > ioChunk {
			want = ioChunk
		}
		n, err := src.ReadAt(buf[:want], int64(pos))
		if n > 0 {
			out := append([]byte(nil), buf[:n]...)
			for off, val := range f.edits {
				if off >= pos && off < pos+uint64(n) {
					out[off-pos] = val
				}
			}
			if _, err := dst.Write(out); err != nil {
				return err
			}
			written += uint64(n)
			pos += uint64(n)
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return err
		}
		if n == 0 {
			break
		}
	}
	// Appended bytes, if any.  written is now the length of the original file,
	// so edits beyond it are the append overlay.
	for {
		val, ok := f.edits[written]
		if !ok {
			break
		}
		if _, err := dst.Write([]byte{val}); err != nil {
			return err
		}
		written++
	}
	if err := dst.Sync(); err != nil {
		return err
	}
	return nil
}
