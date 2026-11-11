package main

import "testing"

func TestSearchForwardAndBackward(t *testing.T) {
	path := tempFile(t, "search.bin", []byte("one two one two"))
	f, err := openPagedFile(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.file.Close()

	needle := []byte("one")
	cases := []struct {
		start   uint64
		forward bool
		want    uint64
		found   bool
	}{
		{0, true, 0, true},
		{1, true, 8, true},
		{15, false, 8, true},
		{7, false, 0, true},
	}
	for _, c := range cases {
		got, found, err := Find(f, needle, c.start, c.forward, true, false)
		if err != nil || found != c.found || got != c.want {
			t.Fatalf("find(%d, forward=%v) = %d, %v, %v; want %d", c.start, c.forward, got, found, err, c.want)
		}
	}
	if _, found, err := Find(f, []byte("zzz"), 0, true, true, false); err != nil || found {
		t.Fatalf("unexpected match: %v, %v", found, err)
	}
}

func TestCaseInsensitiveSearch(t *testing.T) {
	path := tempFile(t, "case.bin", []byte("Alpha BETA"))
	f, err := openPagedFile(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.file.Close()

	if got, found, _ := Find(f, []byte("alpha"), 0, true, false, false); !found || got != 0 {
		t.Fatalf("ignore-case alpha = %d, %v", got, found)
	}
	if _, found, _ := Find(f, []byte("alpha"), 0, true, true, false); found {
		t.Fatal("respect-case alpha should not match")
	}
	if got, found, _ := Find(f, []byte("beta"), 0, true, false, false); !found || got != 6 {
		t.Fatalf("ignore-case beta = %d, %v", got, found)
	}
}

func TestCompareFindsFirstAndLastDifference(t *testing.T) {
	a, err := openPagedFile(tempFile(t, "cmp-a.bin", []byte("abcXefgh")))
	if err != nil {
		t.Fatal(err)
	}
	defer a.file.Close()
	b, err := openPagedFile(tempFile(t, "cmp-b.bin", []byte("abcYefgZ")))
	if err != nil {
		t.Fatal(err)
	}
	defer b.file.Close()

	cases := []struct {
		start   uint64
		forward bool
		want    uint64
	}{
		{0, true, 3},
		{4, true, 7},
		{7, false, 7},
		{6, false, 3},
	}
	for _, c := range cases {
		got, found, err := FindDifference(a, b, c.start, c.forward)
		if err != nil || !found || got != c.want {
			t.Fatalf("difference(%d, forward=%v) = %d, %v, %v; want %d", c.start, c.forward, got, found, err, c.want)
		}
	}
}

func TestCompareReportsLengthDifference(t *testing.T) {
	a, err := openPagedFile(tempFile(t, "len-a.bin", []byte("abc")))
	if err != nil {
		t.Fatal(err)
	}
	defer a.file.Close()
	b, err := openPagedFile(tempFile(t, "len-b.bin", []byte("abcd")))
	if err != nil {
		t.Fatal(err)
	}
	defer b.file.Close()

	if got, found, err := FindDifference(a, b, 0, true); err != nil || !found || got != 3 {
		t.Fatalf("length difference = %d, %v, %v; want 3", got, found, err)
	}
	if _, found, err := FindDifference(a, b, 4, true); err != nil || found {
		t.Fatalf("stepping past length difference = %v, %v", found, err)
	}
}
