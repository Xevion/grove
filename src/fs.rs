use std::cmp::Ordering;
use std::path::PathBuf;

use gpui::SharedString;

#[derive(Clone)]
pub struct FileEntry {
    pub name: SharedString,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size_display: SharedString,
    pub modified_display: SharedString,
}

impl FileEntry {
    #[must_use]
    pub fn new(
        name: String,
        path: PathBuf,
        is_dir: bool,
        size_display: SharedString,
        modified_display: SharedString,
    ) -> Self {
        Self {
            name: SharedString::from(name),
            path,
            is_dir,
            size_display,
            modified_display,
        }
    }

    #[must_use]
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

/// Comparison function for sorting entries: directories first, then case-insensitive name.
#[must_use]
pub fn cmp_entries(a: &FileEntry, b: &FileEntry) -> Ordering {
    match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    }
}

pub fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));
}

/// Merges a pre-sorted `batch` into a sorted `buffer`.
///
/// # Panics
///
/// Will not panic: the `unwrap()` calls on peeked iterators are guarded by
/// `is_some()` checks in the loop condition.
pub fn merge_sorted(buffer: &mut Vec<FileEntry>, batch: Vec<FileEntry>) {
    if buffer.is_empty() {
        *buffer = batch;
        return;
    }
    if batch.is_empty() {
        return;
    }

    // For small batches, binary-search + insert is cheaper than a full merge
    // because it avoids reallocating the buffer. The sqrt(N) threshold
    // approximates the crossover where k insertions (each O(N) shift) exceed
    // one O(N) merge pass.
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

#[cfg(not(target_family = "wasm"))]
mod native {
    use std::fmt;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
    use std::time::{Duration, Instant, SystemTime};

    use futures::SinkExt;
    use futures::channel::mpsc;
    use rayon::prelude::*;
    use tracing::{debug, warn};

    use super::{FileEntry, sort_entries};

    const STAT_CHUNK_SIZE: usize = 200;
    const FLUSH_INTERVAL: Duration = Duration::from_millis(50);

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

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )]
    pub fn format_size(size: u64) -> String {
        const KIB: f64 = 1024.0;
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];

        if size < 1024 {
            return format!("{size} B");
        }

        let exp = (size as f64).log(KIB).floor() as usize;
        let exp = exp.min(UNITS.len() - 1);
        let value = size as f64 / KIB.powi(exp as i32);
        format!("{value:.1} {}", UNITS[exp])
    }

    fn format_modified(time: SystemTime) -> Result<String, jiff::Error> {
        let zoned = jiff::Zoned::try_from(time)?;
        Ok(zoned.strftime("%b %-d, %-I:%M %p").to_string())
    }

    fn make_entry(
        name: String,
        path: PathBuf,
        is_dir: bool,
        size: u64,
        modified: Option<SystemTime>,
    ) -> FileEntry {
        let size_display = if is_dir {
            "\u{2014}".into()
        } else {
            gpui::SharedString::from(format_size(size))
        };
        let modified_display = modified.and_then(|t| format_modified(t).ok()).map_or_else(
            || gpui::SharedString::from("\u{2014}"),
            gpui::SharedString::from,
        );
        FileEntry::new(name, path, is_dir, size_display, modified_display)
    }

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
                .filter_map(|(name, path, is_dir)| {
                    if cancel.load(AtomicOrdering::Relaxed) {
                        return None;
                    }
                    let meta = fs::metadata(path).ok();
                    let size = if *is_dir {
                        0
                    } else {
                        meta.as_ref().map_or(0, std::fs::Metadata::len)
                    };
                    let modified = meta.and_then(|m| m.modified().ok());
                    Some(make_entry(
                        name.clone(),
                        path.clone(),
                        *is_dir,
                        size,
                        modified,
                    ))
                })
                .collect();

            if cancel.load(AtomicOrdering::Relaxed) {
                debug!(path = %path.display(), "directory read cancelled during stat");
                return;
            }

            pending.extend(batch);

            if last_flush.elapsed() >= FLUSH_INTERVAL || pending.len() >= total {
                sort_entries(&mut pending);
                if tx.send(std::mem::take(&mut pending)).await.is_err() {
                    return;
                }
                last_flush = Instant::now();
            }
        }

        if !pending.is_empty() {
            sort_entries(&mut pending);
            if tx.send(pending).await.is_err() {
                return;
            }
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
}

#[cfg(not(target_family = "wasm"))]
pub use native::{Elapsed, read_directory_bg};

#[cfg(target_family = "wasm")]
mod mock {
    use super::{FileEntry, sort_entries};

    struct MockFile {
        name: &'static str,
        is_dir: bool,
        size: &'static str,
        modified: &'static str,
        children: &'static [Self],
    }

    const MOCK_TREE: &[MockFile] = &[
        MockFile {
            name: "src",
            is_dir: true,
            size: "\u{2014}",
            modified: "Apr 7, 2:30 PM",
            children: &[
                MockFile {
                    name: "app.rs",
                    is_dir: false,
                    size: "8.5 KB",
                    modified: "Apr 7, 2:28 PM",
                    children: &[],
                },
                MockFile {
                    name: "assets.rs",
                    is_dir: false,
                    size: "420 B",
                    modified: "Apr 5, 11:02 AM",
                    children: &[],
                },
                MockFile {
                    name: "fs.rs",
                    is_dir: false,
                    size: "6.1 KB",
                    modified: "Apr 7, 2:30 PM",
                    children: &[],
                },
                MockFile {
                    name: "icons.rs",
                    is_dir: false,
                    size: "3.2 KB",
                    modified: "Apr 6, 9:15 AM",
                    children: &[],
                },
                MockFile {
                    name: "lib.rs",
                    is_dir: false,
                    size: "1.1 KB",
                    modified: "Apr 7, 1:45 PM",
                    children: &[],
                },
                MockFile {
                    name: "main.rs",
                    is_dir: false,
                    size: "890 B",
                    modified: "Apr 7, 1:45 PM",
                    children: &[],
                },
                MockFile {
                    name: "model.rs",
                    is_dir: false,
                    size: "620 B",
                    modified: "Apr 5, 3:20 PM",
                    children: &[],
                },
                MockFile {
                    name: "theme.rs",
                    is_dir: false,
                    size: "340 B",
                    modified: "Apr 4, 10:00 AM",
                    children: &[],
                },
                MockFile {
                    name: "ui",
                    is_dir: true,
                    size: "\u{2014}",
                    modified: "Apr 7, 2:25 PM",
                    children: &[
                        MockFile {
                            name: "column_table.rs",
                            is_dir: false,
                            size: "5.8 KB",
                            modified: "Apr 6, 4:10 PM",
                            children: &[],
                        },
                        MockFile {
                            name: "file_list.rs",
                            is_dir: false,
                            size: "4.9 KB",
                            modified: "Apr 7, 2:25 PM",
                            children: &[],
                        },
                        MockFile {
                            name: "mod.rs",
                            is_dir: false,
                            size: "120 B",
                            modified: "Apr 5, 11:00 AM",
                            children: &[],
                        },
                        MockFile {
                            name: "sidebar.rs",
                            is_dir: false,
                            size: "1.6 KB",
                            modified: "Apr 6, 9:30 AM",
                            children: &[],
                        },
                        MockFile {
                            name: "status_bar.rs",
                            is_dir: false,
                            size: "5.2 KB",
                            modified: "Apr 7, 2:20 PM",
                            children: &[],
                        },
                        MockFile {
                            name: "toolbar.rs",
                            is_dir: false,
                            size: "3.4 KB",
                            modified: "Apr 6, 9:30 AM",
                            children: &[],
                        },
                    ],
                },
            ],
        },
        MockFile {
            name: "assets",
            is_dir: true,
            size: "\u{2014}",
            modified: "Apr 6, 9:15 AM",
            children: &[MockFile {
                name: "icons",
                is_dir: true,
                size: "\u{2014}",
                modified: "Apr 6, 9:15 AM",
                children: &[
                    MockFile {
                        name: "archive.svg",
                        is_dir: false,
                        size: "340 B",
                        modified: "Apr 4, 10:00 AM",
                        children: &[],
                    },
                    MockFile {
                        name: "file.svg",
                        is_dir: false,
                        size: "290 B",
                        modified: "Apr 4, 10:00 AM",
                        children: &[],
                    },
                    MockFile {
                        name: "folder.svg",
                        is_dir: false,
                        size: "310 B",
                        modified: "Apr 4, 10:00 AM",
                        children: &[],
                    },
                ],
            }],
        },
        MockFile {
            name: "target",
            is_dir: true,
            size: "\u{2014}",
            modified: "Apr 7, 2:30 PM",
            children: &[],
        },
        MockFile {
            name: ".gitignore",
            is_dir: false,
            size: "32 B",
            modified: "Apr 3, 9:00 AM",
            children: &[],
        },
        MockFile {
            name: "Cargo.lock",
            is_dir: false,
            size: "24.3 KB",
            modified: "Apr 7, 1:45 PM",
            children: &[],
        },
        MockFile {
            name: "Cargo.toml",
            is_dir: false,
            size: "680 B",
            modified: "Apr 7, 1:45 PM",
            children: &[],
        },
        MockFile {
            name: "CLAUDE.md",
            is_dir: false,
            size: "1.8 KB",
            modified: "Apr 5, 3:20 PM",
            children: &[],
        },
        MockFile {
            name: "README.md",
            is_dir: false,
            size: "2.1 KB",
            modified: "Apr 3, 9:00 AM",
            children: &[],
        },
    ];

    fn find_children(path: &std::path::Path) -> &'static [MockFile] {
        let components: Vec<&str> = path
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();

        let mut current = MOCK_TREE;
        for component in &components {
            let found = current.iter().find(|f| f.name == *component && f.is_dir);
            match found {
                Some(dir) => current = dir.children,
                None => return &[],
            }
        }
        current
    }

    #[must_use]
    pub fn mock_entries_for(path: &std::path::Path) -> Vec<FileEntry> {
        let children = find_children(path);
        let mut entries: Vec<FileEntry> = children
            .iter()
            .map(|f| {
                let child_path = path.join(f.name);
                FileEntry::new(
                    f.name.to_string(),
                    child_path,
                    f.is_dir,
                    gpui::SharedString::from(f.size),
                    gpui::SharedString::from(f.modified),
                )
            })
            .collect();
        sort_entries(&mut entries);
        entries
    }
}

#[cfg(target_family = "wasm")]
pub use mock::mock_entries_for;
