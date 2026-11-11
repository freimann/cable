// Minimal directory browser, modelled on CABLE's Dir command.

package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

type SortKey int

const (
	SortUnordered SortKey = iota
	SortName
	SortExt
	SortDate
	SortSize
)

func (s SortKey) label() string {
	switch s {
	case SortUnordered:
		return "Unordered"
	case SortName:
		return "by Name"
	case SortExt:
		return "by Ext"
	case SortDate:
		return "by Date"
	default:
		return "by Size"
	}
}

type DirEntry struct {
	Name     string
	Size     uint64
	Modified time.Time
	HasTime  bool
	IsDir    bool
}

func (e DirEntry) pathIn(dir string) string {
	return filepath.Join(dir, e.Name)
}

type DirBrowser struct {
	Dir        string
	Entries    []DirEntry
	Selected   int
	Scroll     int
	Sort       SortKey
	TargetPane int
	SortPrompt bool
	PathInput  *string
	Error      string
}

func newDirBrowser(dir string, targetPane int) *DirBrowser {
	b := &DirBrowser{
		Dir:        absolutePath(dir),
		TargetPane: targetPane,
		Sort:       SortUnordered,
	}
	b.Refresh()
	return b
}

func (b *DirBrowser) Refresh() {
	b.Error = ""
	entries := []DirEntry{}
	dir, err := os.Open(b.Dir)
	if err != nil {
		b.Error = fmt.Sprintf("Error opening file: %v", err)
	} else {
		items, readErr := dir.ReadDir(-1)
		dir.Close()
		if readErr != nil {
			b.Error = fmt.Sprintf("Error opening file: %v", readErr)
		}
		// ReadDir(-1) keeps the operating system's directory order, which is
		// what the original fs::read_dir listing shows in Unordered mode.
		for _, item := range items {
			name := item.Name()
			if name == "." || name == ".." {
				continue
			}
			entry := DirEntry{Name: name}
			if info, err := item.Info(); err == nil {
				entry.Size = uint64(info.Size())
				entry.Modified = info.ModTime()
				entry.HasTime = true
				entry.IsDir = info.IsDir()
			}
			entries = append(entries, entry)
		}
	}
	sortEntries(entries, b.Sort)
	b.Entries = entries
	if b.Selected >= len(b.Entries) {
		b.Selected = len(b.Entries) - 1
		if b.Selected < 0 {
			b.Selected = 0
		}
	}
	if b.Scroll > b.Selected {
		b.Scroll = b.Selected
	}
}

func (b *DirBrowser) current() *DirEntry {
	if b.Selected < 0 || b.Selected >= len(b.Entries) {
		return nil
	}
	return &b.Entries[b.Selected]
}

func (b *DirBrowser) CurrentPath() string {
	e := b.current()
	if e == nil {
		return ""
	}
	return e.pathIn(b.Dir)
}

func (b *DirBrowser) SetSort(key SortKey) {
	b.Sort = key
	sortEntries(b.Entries, key)
	if len(b.Entries) == 0 {
		b.Selected = 0
	} else if b.Selected >= len(b.Entries) {
		b.Selected = len(b.Entries) - 1
	}
	b.SortPrompt = false
}

func (b *DirBrowser) MoveBy(delta int) {
	if len(b.Entries) == 0 {
		b.Selected = 0
		return
	}
	max := len(b.Entries) - 1
	b.Selected += delta
	if b.Selected < 0 {
		b.Selected = 0
	}
	if b.Selected > max {
		b.Selected = max
	}
}

func (b *DirBrowser) Home() { b.Selected = 0 }
func (b *DirBrowser) End() {
	if len(b.Entries) == 0 {
		b.Selected = 0
	} else {
		b.Selected = len(b.Entries) - 1
	}
}

func (b *DirBrowser) EnsureVisible(page int) {
	if page < 1 {
		page = 1
	}
	if b.Selected < b.Scroll {
		b.Scroll = b.Selected
	} else if b.Selected >= b.Scroll+page {
		b.Scroll = b.Selected + 1 - page
	}
}

// Activate opens the selected directory, or returns the selected file path.
func (b *DirBrowser) Activate() (string, bool) {
	e := b.current()
	if e == nil {
		return "", false
	}
	path := e.pathIn(b.Dir)
	if e.IsDir {
		b.Dir = path
		b.Selected = 0
		b.Scroll = 0
		b.Refresh()
		return "", false
	}
	return path, true
}

func (b *DirBrowser) Parent() {
	parent := filepath.Dir(b.Dir)
	if parent != b.Dir {
		b.Dir = parent
		b.Selected = 0
		b.Scroll = 0
		b.Refresh()
	}
}

func (b *DirBrowser) ToggleTarget() { b.TargetPane = 1 - b.TargetPane }

func (b *DirBrowser) BeginPathInput() {
	s := b.Dir
	if !strings.HasSuffix(s, string(os.PathSeparator)) {
		s += string(os.PathSeparator)
	}
	b.PathInput = &s
	b.Error = ""
}

// ApplyPathInput navigates to a typed filespec.  A directory becomes the
// current directory; a file is selected in its parent directory.
func (b *DirBrowser) ApplyPathInput() {
	if b.PathInput == nil {
		return
	}
	raw := *b.PathInput
	b.PathInput = nil
	spec := expandHome(strings.TrimSpace(raw))
	if spec == "" {
		return
	}
	var candidate string
	if filepath.IsAbs(spec) {
		candidate = spec
	} else {
		candidate = filepath.Join(b.Dir, spec)
	}
	if info, err := os.Stat(candidate); err == nil && info.IsDir() {
		b.Dir = candidate
		b.Selected = 0
		b.Scroll = 0
		b.Refresh()
	} else if info, err := os.Stat(candidate); err == nil && info.Mode().IsRegular() {
		parent := filepath.Dir(candidate)
		name := filepath.Base(candidate)
		if parent != b.Dir {
			b.Dir = parent
			b.Selected = 0
			b.Scroll = 0
			b.Refresh()
		}
		found := false
		for i, e := range b.Entries {
			if e.Name == name {
				b.Selected = i
				found = true
				break
			}
		}
		if !found {
			b.Error = fmt.Sprintf("No matching files found in directory: %s", name)
		}
	} else {
		b.Error = fmt.Sprintf("Error opening file: %s", candidate)
	}
}

func absolutePath(path string) string {
	if filepath.IsAbs(path) {
		return path
	}
	cwd, err := os.Getwd()
	if err != nil {
		return path
	}
	return filepath.Join(cwd, path)
}

func expandHome(path string) string {
	if path == "~" || strings.HasPrefix(path, "~/") || strings.HasPrefix(path, `~\`) {
		if home := os.Getenv("HOME"); home != "" {
			return filepath.Join(home, path[1:])
		}
		if home, err := os.UserHomeDir(); err == nil {
			return filepath.Join(home, path[1:])
		}
	}
	return path
}

func sortEntries(entries []DirEntry, key SortKey) {
	switch key {
	case SortUnordered:
		return
	case SortName:
		sort.SliceStable(entries, func(i, j int) bool {
			a, b := entries[i], entries[j]
			if a.IsDir != b.IsDir {
				return a.IsDir
			}
			an, bn := strings.ToLower(a.Name), strings.ToLower(b.Name)
			if an != bn {
				return an < bn
			}
			return a.Name < b.Name
		})
	case SortExt:
		sort.SliceStable(entries, func(i, j int) bool {
			a, b := entries[i], entries[j]
			if a.IsDir != b.IsDir {
				return a.IsDir
			}
			ea, eb := extension(a.Name), extension(b.Name)
			if ea != eb {
				return ea < eb
			}
			return strings.ToLower(a.Name) < strings.ToLower(b.Name)
		})
	case SortDate:
		sort.SliceStable(entries, func(i, j int) bool {
			a, b := entries[i], entries[j]
			if a.IsDir != b.IsDir {
				return a.IsDir
			}
			if !a.Modified.Equal(b.Modified) {
				return a.Modified.After(b.Modified)
			}
			return strings.ToLower(a.Name) < strings.ToLower(b.Name)
		})
	case SortSize:
		sort.SliceStable(entries, func(i, j int) bool {
			a, b := entries[i], entries[j]
			if a.IsDir != b.IsDir {
				return a.IsDir
			}
			if a.Size != b.Size {
				return a.Size > b.Size
			}
			return strings.ToLower(a.Name) < strings.ToLower(b.Name)
		})
	}
}

func extension(name string) string {
	ext := filepath.Ext(name)
	return strings.ToLower(strings.TrimPrefix(ext, "."))
}

func formatSize(n uint64) string {
	switch {
	case n < 1024:
		return fmt.Sprintf("%d", n)
	case n < 1024*1024:
		return fmt.Sprintf("%.1fK", float64(n)/1024)
	case n < 1024*1024*1024:
		return fmt.Sprintf("%.1fM", float64(n)/(1024*1024))
	default:
		return fmt.Sprintf("%.1fG", float64(n)/(1024*1024*1024))
	}
}

// formatTime formats a timestamp as YYYY-MM-DD HH:MM in UTC.
func formatTime(t time.Time, ok bool) string {
	if !ok {
		return ""
	}
	return t.UTC().Format("2006-01-02 15:04")
}
