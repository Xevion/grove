use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use futures::SinkExt;
use futures::channel::mpsc;
use gpui::SharedString;
use rayon::prelude::*;
use tracing::{debug, warn};

pub const BATCH_SIZE: usize = 200;

#[derive(Clone)]
pub struct FileEntry {
    pub name: SharedString,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size_display: SharedString,
}

impl FileEntry {
    pub fn new(name: String, path: PathBuf, is_dir: bool, size: u64) -> Self {
        let size_display = if is_dir {
            "—".into()
        } else {
            SharedString::from(format_size(size))
        };
        Self {
            name: SharedString::from(name),
            path,
            is_dir,
            size_display,
        }
    }

    pub fn icon(&self) -> &'static str {
        if self.is_dir { "📁" } else { "📄" }
    }
}

pub struct Elapsed(pub std::time::Duration);

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nanos = self.0.as_nanos();
        if nanos < 1_000 {
            write!(f, "{nanos}ns")
        } else if nanos < 1_000_000 {
            write!(f, "{:.1}µs", nanos as f64 / 1_000.0)
        } else if nanos < 1_000_000_000 {
            write!(f, "{:.2}ms", nanos as f64 / 1_000_000.0)
        } else {
            write!(f, "{:.2}s", self.0.as_secs_f64())
        }
    }
}

pub fn format_size(size: u64) -> String {
    match size {
        s if s < 1024 => format!("{s} B"),
        s if s < 1024 * 1024 => format!("{:.1} KB", s as f64 / 1024.0),
        s if s < 1024 * 1024 * 1024 => format!("{:.1} MB", s as f64 / (1024.0 * 1024.0)),
        s => format!("{:.1} GB", s as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

pub fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
}

pub async fn read_directory_bg(path: PathBuf, mut tx: mpsc::Sender<Vec<FileEntry>>) {
    let t0 = Instant::now();

    let read_dir = match fs::read_dir(&path) {
        Ok(rd) => rd,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "failed to read directory");
            return;
        }
    };

    // Phase 1: collect names + file_type from getdents64 (no stat syscalls)
    let raw_entries: Vec<_> = read_dir
        .flatten()
        .filter_map(|entry| {
            let ft = entry.file_type().ok()?;
            Some((entry.file_name().to_string_lossy().into_owned(), entry.path(), ft.is_dir()))
        })
        .collect();

    let t_readdir = t0.elapsed();
    let total = raw_entries.len();
    let dir_count = raw_entries.iter().filter(|(_, _, is_dir)| *is_dir).count();
    let file_count = total - dir_count;

    // Phase 2: parallel stat only for files (directories don't need size)
    let t_stat_start = Instant::now();
    let batch: Vec<FileEntry> = raw_entries
        .into_par_iter()
        .map(|(name, path, is_dir)| {
            let size = if is_dir {
                0
            } else {
                fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
            };
            FileEntry::new(name, path, is_dir, size)
        })
        .collect();
    let t_stat = t_stat_start.elapsed();

    if !batch.is_empty() {
        let _ = tx.send(batch).await;
    }

    debug!(
        path = %path.display(),
        total,
        dirs = dir_count,
        files = file_count,
        readdir = %Elapsed(t_readdir),
        stat = %Elapsed(t_stat),
        total_io = %Elapsed(t0.elapsed()),
        "directory read complete"
    );
}
