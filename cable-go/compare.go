// Search and compare operations, shared by the interactive application.

package main

const compareChunk = 64 * 1024

func folded(data []byte, ebcdic, respectCase bool) {
	if respectCase {
		return
	}
	for i, b := range data {
		data[i] = foldByte(b, ebcdic)
	}
}

// Find locates needle in file, starting at start (inclusive).  forward selects
// the search direction.  respectCase and ebcdic control how bytes are folded
// before comparison.
func Find(file *PagedFile, needle []byte, start uint64, forward, respectCase, ebcdic bool) (uint64, bool, error) {
	n := len(needle)
	if n == 0 {
		return 0, false, nil
	}
	length := file.Len()
	if length < uint64(n) {
		return 0, false, nil
	}
	needle = append([]byte(nil), needle...)
	folded(needle, ebcdic, respectCase)

	if forward {
		if start >= length {
			return 0, false, nil
		}
		pos := start
		for pos+uint64(n) <= length {
			want := compareChunk + n - 1
			buf, err := file.ReadRange(pos, want)
			if err != nil {
				return 0, false, err
			}
			if len(buf) < n {
				break
			}
			folded(buf, ebcdic, respectCase)
			last := len(buf) - n
			for i := 0; i <= last; i++ {
				if bytesEqual(buf[i:i+n], needle) {
					return pos + uint64(i), true, nil
				}
			}
			if pos+compareChunk >= length {
				break
			}
			pos += compareChunk
		}
		return 0, false, nil
	}

	// The last possible match start is start, but it must still fit in the
	// file.
	maxStart := start
	if maxStart > length-uint64(n) {
		maxStart = length - uint64(n)
	}
	high := maxStart
	for {
		var low uint64
		if high > compareChunk {
			low = high - compareChunk
		}
		spanEnd := high + uint64(n)
		if spanEnd > length {
			spanEnd = length
		}
		buf, err := file.ReadRange(low, int(spanEnd-low))
		if err != nil {
			return 0, false, err
		}
		if len(buf) >= n {
			folded(buf, ebcdic, respectCase)
			maxI := int(high - low)
			last := len(buf) - n
			if maxI > last {
				maxI = last
			}
			for i := maxI; ; i-- {
				if bytesEqual(buf[i:i+n], needle) {
					return low + uint64(i), true, nil
				}
				if i == 0 {
					break
				}
			}
		}
		if low == 0 {
			break
		}
		high = low - 1
	}
	return 0, false, nil
}

func bytesEqual(a, b []byte) bool {
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

// FindDifference finds the next (or previous) byte that differs between two
// files.  A difference in length is reported at the first offset past the
// shorter file, unless that offset is before start.
func FindDifference(a, b *PagedFile, start uint64, forward bool) (uint64, bool, error) {
	minLen := a.Len()
	if b.Len() < minLen {
		minLen = b.Len()
	}

	if forward {
		pos := start
		for pos < minLen {
			want := minLen - pos
			if want > compareChunk {
				want = compareChunk
			}
			left, err := a.ReadRange(pos, int(want))
			if err != nil {
				return 0, false, err
			}
			right, err := b.ReadRange(pos, int(want))
			if err != nil {
				return 0, false, err
			}
			n := len(left)
			if len(right) < n {
				n = len(right)
			}
			for i := 0; i < n; i++ {
				if left[i] != right[i] {
					return pos + uint64(i), true, nil
				}
			}
			pos += want
		}
		if a.Len() != b.Len() && start <= minLen {
			return minLen, true, nil
		}
		return 0, false, nil
	}

	limit := start
	if minLen == 0 {
		limit = 0
	} else if limit > minLen-1 {
		limit = minLen - 1
	}
	pos := uint64(0)
	var last uint64
	found := false
	for pos <= limit {
		want := limit - pos + 1
		if want > compareChunk {
			want = compareChunk
		}
		left, err := a.ReadRange(pos, int(want))
		if err != nil {
			return 0, false, err
		}
		right, err := b.ReadRange(pos, int(want))
		if err != nil {
			return 0, false, err
		}
		n := len(left)
		if len(right) < n {
			n = len(right)
		}
		for i := 0; i < n; i++ {
			if left[i] != right[i] {
				last = pos + uint64(i)
				found = true
			}
		}
		pos += want
		if want == 0 {
			break
		}
	}
	if a.Len() != b.Len() && minLen <= start {
		last = minLen
		found = true
	}
	return last, found, nil
}
