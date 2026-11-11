//! Non-interactive I/O benchmark shared with the Go and C++ ports.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};

use crate::buffer::PagedFile;
use crate::compare::{find, find_difference};

const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
const FNV_PRIME: u64 = 1_099_511_628_211;

fn fnv_update(mut value: u64, data: &[u8]) -> u64 {
    for &b in data {
        value ^= u64::from(b);
        value = value.wrapping_mul(FNV_PRIME);
    }
    value
}

fn emit(phase: &str, start: Instant, bytes: u64, checksum: u64) {
    println!(
        "{phase},{:.3},{bytes},{checksum}",
        start.elapsed().as_secs_f64() * 1000.0
    );
}

fn tail(file: &mut PagedFile, want: usize) -> Result<Vec<u8>> {
    let len = file.len();
    let start = len.saturating_sub(want as u64);
    Ok(file.read_range(start, (len - start) as usize)?)
}

fn locate_needle(file: &mut PagedFile) -> Result<Option<Vec<u8>>> {
    const MARKER: &[u8] = b"CABLE-NEEDLE-1234";
    let tail = tail(file, 4096)?;
    Ok(tail
        .windows(MARKER.len())
        .position(|w| w == MARKER)
        .map(|i| tail[i..i + MARKER.len()].to_vec()))
}

pub fn run(paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() || paths.len() > 2 {
        bail!("usage: cable-rs --bench FILE1 [FILE2]");
    }

    println!("phase,milliseconds,bytes,checksum");

    let open_start = Instant::now();
    let mut file1 = match PagedFile::open(&paths[0]) {
        Ok(file) => file,
        Err(e) => bail!("Error opening file {}: {e}", paths[0].display()),
    };
    let len1 = file1.len();
    emit("open", open_start, len1, len1);

    let scan_start = Instant::now();
    let mut checksum = FNV_OFFSET;
    let mut scanned = 0u64;
    let mut pos = 0u64;
    while pos < len1 {
        let want = ((len1 - pos) as usize).min(64 * 1024);
        let data = file1.read_range(pos, want)?;
        if data.is_empty() {
            break;
        }
        checksum = fnv_update(checksum, &data);
        scanned += data.len() as u64;
        pos += data.len() as u64;
    }
    emit("scan", scan_start, scanned, checksum);

    let search_start = Instant::now();
    let needle = locate_needle(&mut file1)?;
    let found = match &needle {
        Some(needle) => find(&mut file1, needle, 0, true, true, false)?,
        None => None,
    };
    emit("search", search_start, len1, found.unwrap_or(u64::MAX));

    if let Some(path2) = paths.get(1) {
        let mut file2 = match PagedFile::open(path2) {
            Ok(file) => file,
            Err(e) => bail!("Error opening file {}: {e}", path2.display()),
        };
        let compare_bytes = file1.len().min(file2.len());
        let compare_start = Instant::now();
        let difference = find_difference(&mut file1, &mut file2, 0, true)?;
        emit(
            "compare",
            compare_start,
            compare_bytes,
            difference.unwrap_or(u64::MAX),
        );
    } else {
        emit("compare", Instant::now(), 0, u64::MAX);
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let parent = paths[0].parent().unwrap_or_else(|| Path::new("."));
    let temp = parent.join(format!(".cable-bench-{nanos}.tmp"));
    fs::copy(&paths[0], &temp)?;
    let mut save_file = PagedFile::open(&temp)?;
    let save_bytes = save_file.len();
    let original = save_file.read_byte(0)?.unwrap_or(0);
    let save_start = Instant::now();
    save_file.write_byte(0, original ^ 0xFF)?;
    let saved = save_file.save()?;
    emit("save", save_start, save_bytes, saved);
    drop(save_file);
    let _ = fs::remove_file(temp);

    Ok(())
}
