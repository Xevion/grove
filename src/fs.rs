use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{Duration, Instant, SystemTime};

use futures::SinkExt;
use futures::channel::mpsc;
use gpui::SharedString;
use jiff::Zoned;
use rayon::prelude::*;
use tracing::{debug, warn};

/// How many raw entries to stat per rayon chunk.
const STAT_CHUNK_SIZE: usize = 200;

/// Minimum interval between channel flushes during streaming.
const FLUSH_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub struct FileEntry {
    pub name: SharedString,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size_display: SharedString,
    pub modified_display: SharedString,
}

impl FileEntry {
    pub fn new(name: String, path: PathBuf, is_dir: bool, size: u64, modified: Option<SystemTime>) -> Self {
        let size_display = if is_dir {
            "—".into()
        } else {
            SharedString::from(format_size(size))
        };
        let modified_display = modified
            .and_then(|t| format_modified(t).ok())
            .map_or_else(|| SharedString::from("—"), SharedString::from);
        Self {
            name: SharedString::from(name),
            path,
            is_dir,
            size_display,
            modified_display,
        }
    }

    pub fn icon(&self) -> crate::icons::Icon {
        use crate::icons::{Icon, IconName};
        let name = if self.is_dir {
            IconName::Folder
        } else {
            IconName::for_filename(self.name.as_ref())
        };
        Icon::new(name)
    }
}

pub struct Elapsed(pub Duration);

impl fmt::Display for Elapsed {
    #[allow(clippy::cast_precision_loss)]
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

#[allow(clippy::cast_precision_loss)]
pub fn format_size(size: u64) -> String {
    match size {
        s if s < 1024 => format!("{s} B"),
        s if s < 1024 * 1024 => format!("{:.1} KB", s as f64 / 1024.0),
        s if s < 1024 * 1024 * 1024 => format!("{:.1} MB", s as f64 / (1024.0 * 1024.0)),
        s => format!("{:.1} GB", s as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

/// Formats a `SystemTime` into a human-readable string like "Apr 5, 2:30 PM".
fn format_modified(time: SystemTime) -> Result<String, jiff::Error> {
    let zoned = Zoned::try_from(time)?;
    // Short month + day + time: "Apr 5, 2:30 PM"
    Ok(zoned.strftime("%b %-d, %-I:%M %p").to_string())
}

/// Comparison function for sorting entries: directories first, then case-insensitive name.
pub fn cmp_entries(a: &FileEntry, b: &FileEntry) -> Ordering {
    match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    }
}

pub fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(cmp_entries);
}

/// Merges a pre-sorted `batch` into a sorted `buffer`.
///
/// Uses binary-search insert for small batches (batch < sqrt(buffer)),
/// merge-sort merge for larger ones.
pub fn merge_sorted(buffer: &mut Vec<FileEntry>, batch: Vec<FileEntry>) {
    if buffer.is_empty() {
        *buffer = batch;
        return;
    }
    if batch.is_empty() {
        return;
    }

    if batch.len() < buffer.len().isqrt().max(1) {
        for entry in batch {
            let pos = buffer.partition_point(|e| cmp_entries(e, &entry).is_lt());
            buffer.insert(pos, entry);
        }
    } else {
        let old = std::mem::take(buffer);
        buffer.reserve(old.len() + batch.len());
        let mut a = old.into_iter().peekable();
        let mut b = batch.into_iter().peekable();
        while a.peek().is_some() && b.peek().is_some() {
            if cmp_entries(a.peek().unwrap(), b.peek().unwrap()).is_le() {
                buffer.push(a.next().unwrap());
            } else {
                buffer.push(b.next().unwrap());
            }
        }
        buffer.extend(a);
        buffer.extend(b);
    }
}

/// Reads a directory in the background, streaming sorted batches via `tx`.
///
/// Phase 1: readdir all entries (fast, no stat syscalls).
/// Phase 2: stat entries in chunks via rayon, flushing sorted batches
///          every ~50ms for responsive UI updates.
pub async fn read_directory_bg(
    path: PathBuf,
    mut tx: mpsc::Sender<Vec<FileEntry>>,
    cancel: Arc<AtomicBool>,
) {
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
            Some((
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
                ft.is_dir(),
            ))
        })
        .collect();

    if cancel.load(AtomicOrdering::Relaxed) {
        return;
    }

    let t_readdir = t0.elapsed();
    let total = raw_entries.len();
    let dir_count = raw_entries.iter().filter(|(_, _, is_dir)| *is_dir).count();
    let file_count = total - dir_count;

    // Phase 2: stat in chunks, sort each, flush on time interval
    let t_stat_start = Instant::now();
    let mut last_flush = Instant::now();
    let mut pending = Vec::new();

    for chunk in raw_entries.chunks(STAT_CHUNK_SIZE) {
        if cancel.load(AtomicOrdering::Relaxed) {
            debug!(path = %path.display(), "directory read cancelled");
            return;
        }

        let batch: Vec<FileEntry> = chunk
            .par_iter()
            .map(|(name, path, is_dir)| {
                let meta = fs::metadata(path).ok();
                let size = if *is_dir {
                    0
                } else {
                    meta.as_ref().map_or(0, std::fs::Metadata::len)
                };
                let modified = meta.and_then(|m| m.modified().ok());
                FileEntry::new(name.clone(), path.clone(), *is_dir, size, modified)
            })
            .collect();

        pending.extend(batch);

        if last_flush.elapsed() >= FLUSH_INTERVAL || pending.len() >= total {
            sort_entries(&mut pending);
            let _ = tx.send(std::mem::take(&mut pending)).await;
            last_flush = Instant::now();
        }
    }

    // Final flush for any remaining entries
    if !pending.is_empty() {
        sort_entries(&mut pending);
        let _ = tx.send(pending).await;
    }

    let t_stat = t_stat_start.elapsed();

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
