use std::cmp::Ordering;
use std::path::PathBuf;

use gpui::SharedString;

#[derive(Clone, Debug)]
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

#[cfg(test)]
mod tests {
    use assert2::assert;
    use gpui::SharedString;
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    fn entry(name: &str, is_dir: bool) -> FileEntry {
        FileEntry {
            name: SharedString::from(name.to_string()),
            path: name.into(),
            is_dir,
            size_display: SharedString::from("0 B"),
            modified_display: SharedString::from("—"),
        }
    }

    #[rstest]
    #[case("alpha", false, "beta", false, Ordering::Less)]
    #[case("beta", false, "alpha", false, Ordering::Greater)]
    #[case("same", false, "same", false, Ordering::Equal)]
    #[case("dir", true, "file", false, Ordering::Less)]
    #[case("file", false, "dir", true, Ordering::Greater)]
    #[case("a_dir", true, "b_dir", true, Ordering::Less)]
    fn cmp_entries_cases(
        #[case] a_name: &str,
        #[case] a_dir: bool,
        #[case] b_name: &str,
        #[case] b_dir: bool,
        #[case] expected: Ordering,
    ) {
        let a = entry(a_name, a_dir);
        let b = entry(b_name, b_dir);
        assert!(cmp_entries(&a, &b) == expected);
    }

    #[test]
    fn cmp_entries_case_insensitive() {
        let upper = entry("Zebra", false);
        let lower = entry("zebra", false);
        assert!(cmp_entries(&upper, &lower) == Ordering::Equal);
    }

    #[test]
    fn sort_entries_dirs_first() {
        let mut entries = vec![
            entry("zebra.txt", false),
            entry("alpha", true),
            entry("apple.rs", false),
            entry("beta", true),
        ];
        sort_entries(&mut entries);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_ref()).collect();
        assert!(names == vec!["alpha", "beta", "apple.rs", "zebra.txt"]);
    }

    #[test]
    fn sort_entries_empty() {
        let mut entries: Vec<FileEntry> = vec![];
        sort_entries(&mut entries);
        assert!(entries.is_empty());
    }

    #[test]
    fn sort_entries_single() {
        let mut entries = vec![entry("only", false)];
        sort_entries(&mut entries);
        assert!(entries.len() == 1);
        assert!(entries[0].name.as_ref() == "only");
    }

    #[test]
    fn merge_sorted_both_empty() {
        let mut buf: Vec<FileEntry> = vec![];
        merge_sorted(&mut buf, vec![]);
        assert!(buf.is_empty());
    }

    #[test]
    fn merge_sorted_into_empty() {
        let mut buf: Vec<FileEntry> = vec![];
        let mut batch = vec![entry("c.txt", false), entry("a.txt", false)];
        sort_entries(&mut batch);
        merge_sorted(&mut buf, batch);
        let names: Vec<&str> = buf.iter().map(|e| e.name.as_ref()).collect();
        assert!(names == vec!["a.txt", "c.txt"]);
    }

    #[test]
    fn merge_sorted_empty_batch() {
        let mut buf = vec![entry("a.txt", false)];
        merge_sorted(&mut buf, vec![]);
        assert!(buf.len() == 1);
    }

    #[test]
    fn merge_sorted_interleave() {
        let mut buf = vec![entry("a.txt", false), entry("c.txt", false)];
        let batch = vec![entry("b.txt", false)];
        merge_sorted(&mut buf, batch);
        let names: Vec<&str> = buf.iter().map(|e| e.name.as_ref()).collect();
        assert!(names == vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn merge_sorted_dirs_first_across_merge() {
        let mut buf = vec![entry("z_dir", true), entry("a.txt", false)];
        let batch = vec![entry("a_dir", true), entry("m.txt", false)];
        merge_sorted(&mut buf, batch);
        let names: Vec<&str> = buf.iter().map(|e| e.name.as_ref()).collect();
        assert!(names == vec!["a_dir", "z_dir", "a.txt", "m.txt"]);
    }

    fn arb_file_entry() -> impl Strategy<Value = FileEntry> {
        ("[a-z]{1,8}", any::<bool>()).prop_map(|(name, is_dir)| entry(&name, is_dir))
    }

    proptest! {
        #[test]
        fn merge_sorted_preserves_sort_invariant(
            buf_entries in proptest::collection::vec(arb_file_entry(), 0..20),
            batch_entries in proptest::collection::vec(arb_file_entry(), 0..20),
        ) {
            let mut buf = buf_entries;
            sort_entries(&mut buf);
            let mut batch = batch_entries;
            sort_entries(&mut batch);
            let expected_len = buf.len() + batch.len();

            merge_sorted(&mut buf, batch);

            // Length preserved
            prop_assert_eq!(buf.len(), expected_len);

            // Result is sorted: dirs before files, then case-insensitive name
            for pair in buf.windows(2) {
                prop_assert!(cmp_entries(&pair[0], &pair[1]).is_le());
            }
        }
    }

    #[cfg(not(target_family = "wasm"))]
    mod native_tests {
        use assert2::assert;
        use proptest::prelude::*;
        use rstest::rstest;

        use super::super::native::format_size;

        #[rstest]
        #[case(0, "0 B")]
        #[case(1, "1 B")]
        #[case(512, "512 B")]
        #[case(1023, "1023 B")]
        #[case(1024, "1.0 KB")]
        #[case(1536, "1.5 KB")]
        #[case(1_048_576, "1.0 MB")]
        #[case(1_073_741_824, "1.0 GB")]
        #[case(1_099_511_627_776, "1.0 TB")]
        fn format_size_known_values(#[case] size: u64, #[case] expected: &str) {
            assert!(format_size(size) == expected);
        }

        #[test]
        fn format_size_max_u64() {
            let result = format_size(u64::MAX);
            // Should not panic and should contain a unit
            assert!(result.contains(' '));
        }

        proptest! {
            #[test]
            fn format_size_never_panics(size: u64) {
                let result = format_size(size);
                // Every result has a space separating value from unit
                prop_assert!(result.contains(' '));
            }

            #[test]
            fn format_size_small_values_are_bytes(size in 0u64..1024) {
                let result = format_size(size);
                prop_assert!(result.ends_with(" B"), "Expected bytes unit for {size}, got {result}");
            }
        }

        #[cfg(not(target_family = "wasm"))]
        mod elapsed_tests {
            use std::time::Duration;

            use assert2::assert;
            use rstest::rstest;

            use super::super::super::native::Elapsed;

            #[rstest]
            #[case(Duration::from_nanos(500), "500ns")]
            #[case(Duration::from_micros(50), "50.0µs")]
            #[case(Duration::from_millis(5), "5.00ms")]
            #[case(Duration::from_secs(2), "2.00s")]
            fn elapsed_display(#[case] duration: Duration, #[case] expected: &str) {
                assert!(format!("{}", Elapsed(duration)) == expected);
            }
        }
    }
}
