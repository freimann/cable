package main

import (
	"testing"
	"time"
)

func entry(name string, size uint64, modified uint64, isDir bool) DirEntry {
	return DirEntry{
		Name:     name,
		Size:     size,
		Modified: time.Unix(int64(modified), 0),
		HasTime:  true,
		IsDir:    isDir,
	}
}

func names(entries []DirEntry) []string {
	out := make([]string, len(entries))
	for i, e := range entries {
		out[i] = e.Name
	}
	return out
}

func equalNames(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestSortByNameAndExtension(t *testing.T) {
	v := []DirEntry{
		entry("b.txt", 10, 10, false),
		entry("a.bin", 30, 30, false),
		entry("dir", 0, 20, true),
	}
	sortEntries(v, SortName)
	if want := []string{"dir", "a.bin", "b.txt"}; !equalNames(names(v), want) {
		t.Fatalf("by name = %v, want %v", names(v), want)
	}
	sortEntries(v, SortExt)
	if want := []string{"dir", "a.bin", "b.txt"}; !equalNames(names(v), want) {
		t.Fatalf("by extension = %v, want %v", names(v), want)
	}
}

func TestSortByDateAndSize(t *testing.T) {
	v := []DirEntry{
		entry("old", 100, 10, false),
		entry("new", 5, 30, false),
		entry("mid", 50, 20, false),
	}
	sortEntries(v, SortDate)
	if want := []string{"new", "mid", "old"}; !equalNames(names(v), want) {
		t.Fatalf("by date = %v, want %v", names(v), want)
	}
	sortEntries(v, SortSize)
	if want := []string{"old", "mid", "new"}; !equalNames(names(v), want) {
		t.Fatalf("by size = %v, want %v", names(v), want)
	}
}

func TestDateFormatting(t *testing.T) {
	if got := formatTime(time.Unix(0, 0), true); got != "1970-01-01 00:00" {
		t.Fatalf("formatTime = %q", got)
	}
}
