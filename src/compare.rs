//! Search and compare operations, shared by the interactive application.

use std::io;

use crate::buffer::PagedFile;
use crate::codec::fold_byte;

const CHUNK: usize = 64 * 1024;

fn folded(data: &mut [u8], ebcdic: bool, respect_case: bool) {
    if !respect_case {
        for b in data {
            *b = fold_byte(*b, ebcdic);
        }
    }
}

/// Find `needle` in `file`, starting at `start` (inclusive).
///
/// `forward` selects the search direction.  `respect_case` and `ebcdic`
/// control how bytes are folded before comparison.
pub fn find(
    file: &mut PagedFile,
    needle: &[u8],
    start: u64,
    forward: bool,
    respect_case: bool,
    ebcdic: bool,
) -> io::Result<Option<u64>> {
    let n = needle.len();
    if n == 0 {
        return Ok(None);
    }
    let len = file.len();
    if len < n as u64 {
        return Ok(None);
    }
    let mut needle = needle.to_vec();
    folded(&mut needle, ebcdic, respect_case);

    if forward {
        if start >= len {
            return Ok(None);
        }
        let mut pos = start;
        while pos + n as u64 <= len {
            let want = CHUNK + n - 1;
            let mut buf = file.read_range(pos, want)?;
            if buf.len() < n {
                break;
            }
            folded(&mut buf, ebcdic, respect_case);
            let last = buf.len() - n;
            for i in 0..=last {
                if &buf[i..i + n] == needle.as_slice() {
                    return Ok(Some(pos + i as u64));
                }
            }
            if pos + CHUNK as u64 >= len {
                break;
            }
            pos += CHUNK as u64;
        }
        Ok(None)
    } else {
        // The last possible match start is start, but it must still fit in
        // the file.
        let max_start = start.min(len - n as u64);
        let mut high = max_start;
        loop {
            let low = high.saturating_sub(CHUNK as u64);
            let span_end = (high + n as u64).min(len);
            let mut buf = file.read_range(low, (span_end - low) as usize)?;
            if buf.len() >= n {
                folded(&mut buf, ebcdic, respect_case);
                let max_i = (high - low) as usize;
                let last = buf.len() - n;
                let mut i = max_i.min(last);
                loop {
                    if &buf[i..i + n] == needle.as_slice() {
                        return Ok(Some(low + i as u64));
                    }
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                }
            }
            if low == 0 {
                break;
            }
            high = low.saturating_sub(1);
        }
        Ok(None)
    }
}

/// Find the next (or previous) byte that differs between two files.
///
/// A difference in length is reported at the first offset past the shorter
/// file, unless that offset is before `start`.
pub fn find_difference(
    a: &mut PagedFile,
    b: &mut PagedFile,
    start: u64,
    forward: bool,
) -> io::Result<Option<u64>> {
    let min_len = a.len().min(b.len());

    if forward {
        let mut pos = start;
        while pos < min_len {
            let want = ((min_len - pos) as usize).min(CHUNK);
            let left = a.read_range(pos, want)?;
            let right = b.read_range(pos, want)?;
            for i in 0..left.len().min(right.len()) {
                if left[i] != right[i] {
                    return Ok(Some(pos + i as u64));
                }
            }
            pos += want as u64;
        }
        if a.len() != b.len() && start <= min_len {
            return Ok(Some(min_len));
        }
        Ok(None)
    } else {
        let limit = start.min(min_len.saturating_sub(1));
        let mut pos = 0;
        let mut last = None;
        while pos <= limit {
            let want = ((limit - pos + 1) as usize).min(CHUNK);
            let left = a.read_range(pos, want)?;
            let right = b.read_range(pos, want)?;
            for i in 0..left.len().min(right.len()) {
                if left[i] != right[i] {
                    last = Some(pos + i as u64);
                }
            }
            pos += want as u64;
            if want == 0 {
                break;
            }
        }
        if a.len() != b.len() && min_len <= start {
            last = Some(min_len);
        }
        Ok(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = dir.join(format!("cable-rs-{pid}-{nanos}-{name}"));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn search_forward_and_backward() {
        let path = temp_file("search.bin", b"one two one two");
        let mut f = PagedFile::open(&path).unwrap();
        let needle = b"one";
        assert_eq!(find(&mut f, needle, 0, true, true, false).unwrap(), Some(0));
        assert_eq!(find(&mut f, needle, 1, true, true, false).unwrap(), Some(8));
        assert_eq!(
            find(&mut f, needle, 15, false, true, false).unwrap(),
            Some(8)
        );
        assert_eq!(
            find(&mut f, needle, 7, false, true, false).unwrap(),
            Some(0)
        );
        assert_eq!(find(&mut f, b"zzz", 0, true, true, false).unwrap(), None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn case_insensitive_search() {
        let path = temp_file("case.bin", b"Alpha BETA");
        let mut f = PagedFile::open(&path).unwrap();
        assert_eq!(
            find(&mut f, b"alpha", 0, true, false, false).unwrap(),
            Some(0)
        );
        assert_eq!(find(&mut f, b"alpha", 0, true, true, false).unwrap(), None);
        assert_eq!(
            find(&mut f, b"beta", 0, true, false, false).unwrap(),
            Some(6)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn compare_finds_first_and_last_difference() {
        let a = temp_file("cmp-a.bin", b"abcXefgh");
        let b = temp_file("cmp-b.bin", b"abcYefgZ");
        let mut fa = PagedFile::open(&a).unwrap();
        let mut fb = PagedFile::open(&b).unwrap();
        assert_eq!(find_difference(&mut fa, &mut fb, 0, true).unwrap(), Some(3));
        assert_eq!(find_difference(&mut fa, &mut fb, 4, true).unwrap(), Some(7));
        assert_eq!(
            find_difference(&mut fa, &mut fb, 7, false).unwrap(),
            Some(7)
        );
        assert_eq!(
            find_difference(&mut fa, &mut fb, 6, false).unwrap(),
            Some(3)
        );
        fs::remove_file(a).unwrap();
        fs::remove_file(b).unwrap();
    }

    #[test]
    fn compare_reports_length_difference() {
        let a = temp_file("len-a.bin", b"abc");
        let b = temp_file("len-b.bin", b"abcd");
        let mut fa = PagedFile::open(&a).unwrap();
        let mut fb = PagedFile::open(&b).unwrap();
        assert_eq!(find_difference(&mut fa, &mut fb, 0, true).unwrap(), Some(3));
        // Stepping past the length difference must not report it again.
        assert_eq!(find_difference(&mut fa, &mut fb, 4, true).unwrap(), None);
        fs::remove_file(a).unwrap();
        fs::remove_file(b).unwrap();
    }
}
