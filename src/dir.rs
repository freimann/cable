//! Minimal directory browser, modelled on CABLE's `Dir` command.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Unordered,
    Name,
    Ext,
    Date,
    Size,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Unordered => "Unordered",
            SortKey::Name => "by Name",
            SortKey::Ext => "by Ext",
            SortKey::Date => "by Date",
            SortKey::Size => "by Size",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub is_dir: bool,
}

impl DirEntry {
    pub fn path_in(&self, dir: &Path) -> PathBuf {
        dir.join(&self.name)
    }
}

pub struct DirBrowser {
    pub dir: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected: usize,
    pub scroll: usize,
    pub sort: SortKey,
    /// Pane that receives a file selected with Enter.
    pub target_pane: usize,
    pub sort_prompt: bool,
    pub path_input: Option<String>,
    pub error: Option<String>,
}

impl DirBrowser {
    pub fn new(dir: impl AsRef<Path>, target_pane: usize) -> Self {
        let dir = absolute(dir.as_ref());
        let mut browser = Self {
            dir,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            sort: SortKey::Unordered,
            target_pane,
            sort_prompt: false,
            path_input: None,
            error: None,
        };
        browser.refresh();
        browser
    }

    pub fn refresh(&mut self) {
        self.error = None;
        let mut entries = Vec::new();
        match fs::read_dir(&self.dir) {
            Ok(rd) => {
                for item in rd.flatten() {
                    let name = item.file_name().to_string_lossy().into_owned();
                    if name == "." || name == ".." {
                        continue;
                    }
                    let meta = item.metadata().ok();
                    entries.push(DirEntry {
                        name,
                        size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                        modified: meta.as_ref().and_then(|m| m.modified().ok()),
                        is_dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                    });
                }
            }
            Err(e) => {
                self.error = Some(format!("Error opening file: {e}"));
            }
        }
        sort_entries(&mut entries, self.sort);
        self.entries = entries;
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
        self.scroll = self.scroll.min(self.selected);
    }

    pub fn current(&self) -> Option<&DirEntry> {
        self.entries.get(self.selected)
    }

    pub fn current_path(&self) -> Option<PathBuf> {
        self.current().map(|e| e.path_in(&self.dir))
    }

    pub fn set_sort(&mut self, key: SortKey) {
        self.sort = key;
        sort_entries(&mut self.entries, key);
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        self.sort_prompt = false;
    }

    pub fn move_by(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        let max = self.entries.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, max) as usize;
    }

    pub fn home(&mut self) {
        self.selected = 0;
    }

    pub fn end(&mut self) {
        self.selected = self.entries.len().saturating_sub(1);
    }

    pub fn ensure_visible(&mut self, page: usize) {
        let page = page.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + page {
            self.scroll = self.selected + 1 - page;
        }
    }

    /// Open the selected directory, or return the selected file path.
    pub fn activate(&mut self) -> Option<PathBuf> {
        let entry = self.current()?.clone();
        let path = entry.path_in(&self.dir);
        if entry.is_dir {
            self.dir = path;
            self.selected = 0;
            self.scroll = 0;
            self.refresh();
            None
        } else {
            Some(path)
        }
    }

    pub fn parent(&mut self) {
        if let Some(parent) = self.dir.parent() {
            self.dir = parent.to_path_buf();
            self.selected = 0;
            self.scroll = 0;
            self.refresh();
        }
    }

    pub fn toggle_target(&mut self) {
        self.target_pane = 1 - self.target_pane;
    }

    pub fn begin_path_input(&mut self) {
        let mut s = self.dir.to_string_lossy().into_owned();
        if !s.ends_with(std::path::MAIN_SEPARATOR) {
            s.push(std::path::MAIN_SEPARATOR);
        }
        self.path_input = Some(s);
        self.error = None;
    }

    /// Navigate to a typed filespec.  A directory becomes the current
    /// directory; a file is selected in its parent directory.
    pub fn apply_path_input(&mut self) {
        let Some(raw) = self.path_input.take() else {
            return;
        };
        let spec = expand_home(raw.trim());
        if spec.as_os_str().is_empty() {
            return;
        }
        let candidate = if spec.is_absolute() {
            spec
        } else {
            self.dir.join(spec)
        };
        if candidate.is_dir() {
            self.dir = candidate;
            self.selected = 0;
            self.scroll = 0;
            self.refresh();
        } else if candidate.is_file() {
            let parent = candidate
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.dir.clone());
            let name = candidate
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if parent != self.dir {
                self.dir = parent;
                self.selected = 0;
                self.scroll = 0;
                self.refresh();
            }
            if let Some(i) = self.entries.iter().position(|e| e.name == name) {
                self.selected = i;
            } else {
                self.error = Some(format!("No matching files found in directory: {name}"));
            }
        } else {
            self.error = Some(format!("Error opening file: {}", candidate.display()));
        }
    }
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn expand_home(path: &str) -> PathBuf {
    let p = Path::new(path);
    if let Ok(rest) = p.strip_prefix("~") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    p.to_path_buf()
}

pub fn sort_entries(entries: &mut [DirEntry], key: SortKey) {
    match key {
        SortKey::Unordered => {}
        SortKey::Name => entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.name.cmp(&b.name))
            })
        }),
        SortKey::Ext => entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                let ea = extension(&a.name);
                let eb = extension(&b.name);
                ea.cmp(&eb)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            })
        }),
        SortKey::Date => entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| b.modified.cmp(&a.modified))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        SortKey::Size => entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| b.size.cmp(&a.size))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
    }
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn format_size(n: u64) -> String {
    if n < 1024 {
        format!("{n}")
    } else if n < 1024 * 1024 {
        format!("{:.1}K", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1}M", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Format a timestamp as `YYYY-MM-DD HH:MM` in UTC.
pub fn format_time(t: Option<SystemTime>) -> String {
    let Some(t) = t else {
        return String::new();
    };
    let secs = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

// Howard Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y } as i32;
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn e(name: &str, size: u64, secs: u64, is_dir: bool) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            size,
            modified: Some(UNIX_EPOCH + Duration::from_secs(secs)),
            is_dir,
        }
    }

    fn names(v: &[DirEntry]) -> Vec<&str> {
        v.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn sort_by_name_and_extension() {
        let mut v = vec![
            e("b.txt", 10, 10, false),
            e("a.bin", 30, 30, false),
            e("dir", 0, 20, true),
        ];
        sort_entries(&mut v, SortKey::Name);
        assert_eq!(names(&v), vec!["dir", "a.bin", "b.txt"]);
        sort_entries(&mut v, SortKey::Ext);
        assert_eq!(names(&v), vec!["dir", "a.bin", "b.txt"]);
    }

    #[test]
    fn sort_by_date_and_size() {
        let mut v = vec![
            e("old", 100, 10, false),
            e("new", 5, 30, false),
            e("mid", 50, 20, false),
        ];
        sort_entries(&mut v, SortKey::Date);
        assert_eq!(names(&v), vec!["new", "mid", "old"]);
        sort_entries(&mut v, SortKey::Size);
        assert_eq!(names(&v), vec!["old", "mid", "new"]);
    }

    #[test]
    fn date_formatting() {
        assert_eq!(
            format_time(Some(UNIX_EPOCH + Duration::from_secs(0))),
            "1970-01-01 00:00"
        );
    }
}
