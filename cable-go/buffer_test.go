package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

func tempFile(t *testing.T, name string, contents []byte) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(path, contents, 0666); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestReadsAndEdits(t *testing.T) {
	path := tempFile(t, "edit.bin", []byte("abcdef"))
	f, err := openPagedFile(path)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { f.file.Close() }()

	if got := f.Len(); got != 6 {
		t.Fatalf("len = %d, want 6", got)
	}
	if b, ok, err := f.ReadOffset(0); err != nil || !ok || b != 'a' {
		t.Fatalf("read byte 0 = %q, %v, %v", b, ok, err)
	}
	if err := f.WriteOffset(1, 'Z'); err != nil {
		t.Fatal(err)
	}
	if b, ok, err := f.ReadOffset(1); err != nil || !ok || b != 'Z' {
		t.Fatalf("read edited byte = %q, %v, %v", b, ok, err)
	}
	if !f.Dirty() {
		t.Fatal("file should be dirty")
	}
	if got, err := f.ReadRange(0, 3); err != nil || !bytes.Equal(got, []byte("aZc")) {
		t.Fatalf("range = %q, %v", got, err)
	}
}

func TestAppendAndSave(t *testing.T) {
	path := tempFile(t, "append.bin", []byte("ab"))
	f, err := openPagedFile(path)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { f.file.Close() }()

	if err := f.WriteOffset(2, 'c'); err != nil {
		t.Fatal(err)
	}
	if err := f.WriteOffset(3, 'd'); err != nil {
		t.Fatal(err)
	}
	if got := f.Len(); got != 4 {
		t.Fatalf("len = %d, want 4", got)
	}
	if n, err := f.Save(); err != nil || n != 4 {
		t.Fatalf("save = %d, %v", n, err)
	}
	if f.Dirty() {
		t.Fatal("saved file should not be dirty")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(data, []byte("abcd")) {
		t.Fatalf("saved contents = %q", data)
	}
}

func TestSaveWithoutChangesFails(t *testing.T) {
	path := tempFile(t, "nochange.bin", []byte("x"))
	f, err := openPagedFile(path)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { f.file.Close() }()
	if _, err := f.Save(); err == nil {
		t.Fatal("save without changes should fail")
	}
}

func TestReadsAfterSaveSeeNewContents(t *testing.T) {
	path := tempFile(t, "reopen.bin", []byte("abcdef"))
	f, err := openPagedFile(path)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { f.file.Close() }()

	if err := f.WriteOffset(1, 'Z'); err != nil {
		t.Fatal(err)
	}
	if _, err := f.Save(); err != nil {
		t.Fatal(err)
	}
	if b, ok, err := f.ReadOffset(0); err != nil || !ok || b != 'a' {
		t.Fatalf("byte 0 = %q, %v, %v", b, ok, err)
	}
	if b, ok, err := f.ReadOffset(1); err != nil || !ok || b != 'Z' {
		t.Fatalf("byte 1 = %q, %v, %v", b, ok, err)
	}
	if got, err := f.ReadRange(0, 3); err != nil || !bytes.Equal(got, []byte("aZc")) {
		t.Fatalf("range = %q, %v", got, err)
	}
	if f.Dirty() {
		t.Fatal("file should not be dirty after save")
	}
}
