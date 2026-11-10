//! Paged, editable file storage.
//!
//! CABLE handled files through a small number of 16 KiB buffers.  The clone
//! keeps the same "work on a large file without loading it all" property by
//! reading 64 KiB pages on demand and keeping edits in an overlay.  Only the
//! edited pages and the overlay are kept in memory.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const PAGE_SIZE: u64 = 64 * 1024;
const IO_CHUNK: usize = 64 * 1024;

#[derive(Debug)]
pub struct PagedFile {
    path: PathBuf,
    file: File,
    len: u64,
    pages: HashMap<u64, Vec<u8>>,
    edits: BTreeMap<u64, u8>,
    read_only: bool,
    dirty: bool,
}

impl PagedFile {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let meta = file.metadata()?;
        let read_only = meta.permissions().readonly();
        Ok(Self {
            path,
            file,
            len: meta.len(),
            pages: HashMap::new(),
            edits: BTreeMap::new(),
            read_only,
            dirty: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn page(&mut self, page: u64) -> io::Result<&[u8]> {
        if !self.pages.contains_key(&page) {
            let start = page * PAGE_SIZE;
            if start >= self.len {
                self.pages.insert(page, Vec::new());
            } else {
                let want = (self.len - start).min(PAGE_SIZE) as usize;
                let mut buf = vec![0u8; want];
                self.file.seek(SeekFrom::Start(start))?;
                self.file.read_exact(&mut buf)?;
                self.pages.insert(page, buf);
            }
        }
        Ok(self.pages.get(&page).map(Vec::as_slice).unwrap_or(&[]))
    }

    /// Read one byte without applying the edit overlay.
    fn raw_byte(&mut self, offset: u64) -> io::Result<u8> {
        let page = offset / PAGE_SIZE;
        let within = (offset % PAGE_SIZE) as usize;
        let data = self.page(page)?;
        data.get(within)
            .copied()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "offset past end of file"))
    }

    /// Read one byte, applying the edit overlay.
    pub fn read_byte(&mut self, offset: u64) -> io::Result<Option<u8>> {
        if offset >= self.len {
            return Ok(None);
        }
        if let Some(&b) = self.edits.get(&offset) {
            return Ok(Some(b));
        }
        Ok(Some(self.raw_byte(offset)?))
    }

    /// Read up to `len` bytes, applying the edit overlay.  The returned vector
    /// is shortened at end of file.
    pub fn read_range(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        if offset >= self.len || len == 0 {
            return Ok(Vec::new());
        }
        let want = ((self.len - offset) as usize).min(len);
        let mut out = Vec::with_capacity(want);
        let mut pos = offset;
        let end = offset + want as u64;
        while pos < end {
            let page_no = pos / PAGE_SIZE;
            let within = (pos % PAGE_SIZE) as usize;
            let data = self.page(page_no)?;
            let available = data.len().saturating_sub(within);
            if available == 0 {
                break;
            }
            let take = available.min((end - pos) as usize);
            out.extend_from_slice(&data[within..within + take]);
            pos += take as u64;
        }
        // Overlay edits that fall inside the requested range.
        for (&off, &val) in self.edits.range(offset..end) {
            let i = (off - offset) as usize;
            if i < out.len() {
                out[i] = val;
            }
        }
        Ok(out)
    }

    /// Write one byte into the edit overlay.  Writing at `len` appends.
    pub fn write_byte(&mut self, offset: u64, value: u8) -> io::Result<()> {
        if offset > self.len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "offset past end of file",
            ));
        }
        if offset == self.len {
            self.edits.insert(offset, value);
            self.len += 1;
        } else {
            let original = self.raw_byte(offset)?;
            if value == original {
                self.edits.remove(&offset);
            } else {
                self.edits.insert(offset, value);
            }
        }
        self.dirty = !self.edits.is_empty();
        Ok(())
    }

    /// Save the file, applying the edit overlay to a temporary copy and then
    /// renaming it over the original.  This keeps the original intact if an
    /// error occurs part way through.
    pub fn save(&mut self) -> io::Result<u64> {
        if !self.dirty {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No changes to write",
            ));
        }
        if self.read_only {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "File is read-only - can't write",
            ));
        }

        let tmp = self.temp_path();
        let result = self.write_copy(&tmp);
        if let Err(e) = result {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }

        #[cfg(windows)]
        {
            // std::fs::rename does not replace an existing file on Windows.
            if let Err(e) = fs::remove_file(&self.path) {
                if e.kind() != io::ErrorKind::NotFound {
                    let _ = fs::remove_file(&tmp);
                    return Err(e);
                }
            }
        }
        if let Err(e) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }

        // The open handle still refers to the replaced (now unlinked) file on
        // Unix, so reopen the path before serving any further reads.
        self.file = File::open(&self.path)?;
        let meta = self.file.metadata()?;
        self.len = meta.len();
        self.read_only = meta.permissions().readonly();
        self.pages.clear();
        self.edits.clear();
        self.dirty = false;
        Ok(self.len)
    }

    fn temp_path(&self) -> PathBuf {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "cable.tmp".to_string());
        let dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        dir.join(format!(".{name}.cable-rs-{pid}-{nanos}.tmp"))
    }

    fn write_copy(&mut self, tmp: &Path) -> io::Result<()> {
        let mut src = File::open(&self.path)?;
        let mut dst = OpenOptions::new().write(true).create_new(true).open(tmp)?;

        let mut buf = vec![0u8; IO_CHUNK];
        let mut pos = 0u64;
        let mut written = 0u64;
        while pos < self.len {
            src.seek(SeekFrom::Start(pos))?;
            let want = ((self.len - pos) as usize).min(IO_CHUNK);
            let n = src.read(&mut buf[..want])?;
            if n == 0 {
                break;
            }
            let mut out = buf[..n].to_vec();
            for (&off, &val) in self.edits.range(pos..pos + n as u64) {
                out[(off - pos) as usize] = val;
            }
            dst.write_all(&out)?;
            written += n as u64;
            pos += n as u64;
        }
        // Appended bytes, if any.  `written` is now the length of the
        // original file, so edits beyond it are the append overlay.
        for (&off, &val) in self.edits.range(written..) {
            if off == written {
                dst.write_all(&[val])?;
                written += 1;
            } else {
                break;
            }
        }
        dst.flush()?;
        dst.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn reads_and_edits() {
        let path = temp_file("edit.bin", b"abcdef");
        let mut f = PagedFile::open(&path).unwrap();
        assert_eq!(f.len(), 6);
        assert_eq!(f.read_byte(0).unwrap(), Some(b'a'));
        f.write_byte(1, b'Z').unwrap();
        assert_eq!(f.read_byte(1).unwrap(), Some(b'Z'));
        assert!(f.is_dirty());
        assert_eq!(f.read_range(0, 3).unwrap(), b"aZc");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn append_and_save() {
        let path = temp_file("append.bin", b"ab");
        let mut f = PagedFile::open(&path).unwrap();
        f.write_byte(2, b'c').unwrap();
        f.write_byte(3, b'd').unwrap();
        assert_eq!(f.len(), 4);
        assert_eq!(f.save().unwrap(), 4);
        assert!(!f.is_dirty());
        assert_eq!(fs::read(&path).unwrap(), b"abcd");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn save_without_changes_fails() {
        let path = temp_file("nochange.bin", b"x");
        let mut f = PagedFile::open(&path).unwrap();
        assert!(f.save().is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reads_after_save_see_new_contents() {
        let path = temp_file("reopen.bin", b"abcdef");
        let mut f = PagedFile::open(&path).unwrap();
        f.write_byte(1, b'Z').unwrap();
        f.save().unwrap();
        assert_eq!(f.read_byte(0).unwrap(), Some(b'a'));
        assert_eq!(f.read_byte(1).unwrap(), Some(b'Z'));
        assert_eq!(f.read_range(0, 3).unwrap(), b"aZc");
        assert!(!f.is_dirty());
        fs::remove_file(path).unwrap();
    }
}
