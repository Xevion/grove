use std::cmp::Ordering;
use std::fmt;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;
use std::{env, fs};

use futures::channel::mpsc;
use futures::StreamExt;
use gpui::*;
use rayon::prelude::*;
use tracing::{debug, info, instrument, warn};
use tracing_subscriber::EnvFilter;

const BG_BASE: u32 = 0x1e1e2e;
const BG_SURFACE: u32 = 0x313244;
const BG_HOVER: u32 = 0x45475a;
const TEXT_PRIMARY: u32 = 0xcdd6f4;
const TEXT_SECONDARY: u32 = 0xa6adc8;
const TEXT_MUTED: u32 = 0x6c7086;
const ACCENT: u32 = 0x89b4fa;
const SIDEBAR_BG: u32 = 0x181825;
const BORDER_COLOR: u32 = 0x313244;

const BATCH_SIZE: usize = 200;

#[derive(Clone)]
struct FileEntry {
    name: SharedString,
    path: PathBuf,
    is_dir: bool,
    size_display: SharedString,
}

impl FileEntry {
    fn new(name: String, path: PathBuf, is_dir: bool, size: u64) -> Self {
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

    fn icon(&self) -> &'static str {
        if self.is_dir { "📁" } else { "📄" }
    }
}

struct Elapsed(std::time::Duration);

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

fn format_size(size: u64) -> String {
    match size {
        s if s < 1024 => format!("{s} B"),
        s if s < 1024 * 1024 => format!("{:.1} KB", s as f64 / 1024.0),
        s if s < 1024 * 1024 * 1024 => format!("{:.1} MB", s as f64 / (1024.0 * 1024.0)),
        s => format!("{:.1} GB", s as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
}

#[derive(Clone)]
struct Bookmark {
    label: &'static str,
    path: PathBuf,
    exists: bool,
}

fn default_bookmarks() -> Vec<Bookmark> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    [
        ("Home", home.clone()),
        ("Desktop", home.join("Desktop")),
        ("Documents", home.join("Documents")),
        ("Downloads", home.join("Downloads")),
        ("Projects", home.join("projects")),
        ("/", PathBuf::from("/")),
    ]
    .into_iter()
    .map(|(label, path)| {
        let exists = path.exists();
        Bookmark { label, path, exists }
    })
    .collect()
}

struct GroveApp {
    current_dir: PathBuf,
    entries: Vec<FileEntry>,
    bookmarks: Vec<Bookmark>,
    selected_index: Option<usize>,
    loading_task: Option<Task<()>>,
    is_loading: bool,
    needs_initial_load: bool,
    scroll_handle: UniformListScrollHandle,
}

impl GroveApp {
    fn new() -> Self {
        Self {
            current_dir: env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            entries: Vec::new(),
            bookmarks: default_bookmarks(),
            selected_index: None,
            loading_task: None,
            is_loading: true,
            needs_initial_load: true,
            scroll_handle: UniformListScrollHandle::default(),
        }
    }

    #[instrument(skip(self, window, cx), fields(path = %path.display()))]
    fn start_loading(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let t0 = Instant::now();
        info!("loading directory");
        self.current_dir = path.clone();
        self.selected_index = None;
        self.is_loading = true;

        let (tx, mut rx) = mpsc::channel::<Vec<FileEntry>>(8);

        cx.background_spawn(async move {
            read_directory_bg(path, tx).await;
        })
        .detach();

        let task = cx.spawn_in(window, async move |weak, cx| {
            let t_recv_start = Instant::now();
            let mut collected = Vec::new();

            while let Some(batch) = rx.next().await {
                collected.extend(batch);

                if collected.len() >= BATCH_SIZE {
                    let snapshot = collected.clone();
                    let ok = weak
                        .update_in(cx, |this, _window, cx| {
                            this.entries = snapshot;
                            this.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
                            cx.notify();
                        })
                        .is_ok();
                    if !ok {
                        return;
                    }
                }
            }
            let t_recv = t_recv_start.elapsed();

            let t_sort_start = Instant::now();
            sort_entries(&mut collected);
            let t_sort = t_sort_start.elapsed();

            let count = collected.len();
            let _ = weak.update_in(cx, |this, _window, cx| {
                let t_total = t0.elapsed();
                debug!(
                    count,
                    recv = %Elapsed(t_recv),
                    sort = %Elapsed(t_sort),
                    total = %Elapsed(t_total),
                    "directory load complete"
                );
                this.entries = collected;
                this.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
                this.is_loading = false;
                cx.notify();
            });
        });

        self.loading_task = Some(task);
        cx.notify();
    }

    fn navigate_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let t0 = Instant::now();
        let is_dir = path.is_dir();
        let t_stat = t0.elapsed();
        debug!(path = %path.display(), is_dir, stat = %Elapsed(t_stat), "navigate_to");
        if is_dir {
            self.start_loading(path, window, cx);
        }
    }

    fn navigate_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = self.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.start_loading(parent, window, cx);
        }
    }
}

async fn read_directory_bg(path: PathBuf, mut tx: mpsc::Sender<Vec<FileEntry>>) {
    use futures::SinkExt;

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

impl Render for GroveApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_initial_load {
            self.needs_initial_load = false;
            let cwd = self.current_dir.clone();
            self.start_loading(cwd, window, cx);
        }

        div()
            .flex()
            .flex_col()
            .bg(rgb(BG_BASE))
            .text_color(rgb(TEXT_PRIMARY))
            .size_full()
            .font_family("sans-serif")
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(self.render_file_list(cx)),
            )
    }
}

impl GroveApp {
    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let path_display = self.current_dir.display().to_string();

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(rgb(BG_SURFACE))
            .border_b_1()
            .border_color(rgb(BORDER_COLOR))
            .child(
                div()
                    .id("nav-up")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(BG_HOVER)))
                    .text_sm()
                    .child("↑ Up")
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.navigate_up(window, cx);
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(BG_BASE))
                    .text_sm()
                    .text_color(rgb(TEXT_SECONDARY))
                    .overflow_x_hidden()
                    .child(path_display),
            )
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .flex()
            .flex_col()
            .w(px(200.))
            .min_w(px(200.))
            .bg(rgb(SIDEBAR_BG))
            .border_r_1()
            .border_color(rgb(BORDER_COLOR))
            .py_2()
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(TEXT_MUTED))
                    .child("BOOKMARKS"),
            );

        for bookmark in &self.bookmarks {
            let path = bookmark.path.clone();
            let exists = bookmark.exists;
            let label = bookmark.label;

            let mut bookmark_el = div()
                .id(SharedString::from(format!("bm-{}", label)))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .mx_1()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .text_color(if exists {
                    rgb(TEXT_PRIMARY)
                } else {
                    rgb(TEXT_MUTED)
                })
                .hover(|s| s.bg(rgb(BG_HOVER)))
                .child(label);

            if exists {
                bookmark_el =
                    bookmark_el.on_click(cx.listener(move |this, _event, window, cx| {
                        this.navigate_to(path.clone(), window, cx);
                    }));
            }

            sidebar = sidebar.child(bookmark_el);
        }

        sidebar
    }

    fn render_file_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entry_count = self.entries.len();

        if entry_count == 0 && !self.is_loading {
            return div()
                .flex_1()
                .flex()
                .justify_center()
                .items_center()
                .py_8()
                .text_color(rgb(TEXT_MUTED))
                .text_sm()
                .child("Empty directory")
                .into_any_element();
        }

        let mut container = div().flex().flex_col().flex_1().min_h_0();

        if self.is_loading {
            container = container.child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(if entry_count > 0 {
                        format!("Loading… ({entry_count} entries)")
                    } else {
                        "Loading…".into()
                    }),
            );
        }

        container
            .child(
                uniform_list(
                    "file-list",
                    entry_count,
                    cx.processor(|this, range: Range<usize>, _window, cx| {
                        this.render_entry_range(range, cx)
                    }),
                )
                .flex_1()
                .track_scroll(self.scroll_handle.clone()),
            )
            .into_any_element()
    }

    fn render_entry_range(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .map(|i| {
                let entry = &self.entries[i];
                let path = entry.path.clone();
                let is_dir = entry.is_dir;
                let is_selected = self.selected_index == Some(i);
                let name = entry.name.clone();
                let size_display = entry.size_display.clone();
                let icon = entry.icon();

                let mut row = div()
                    .id(ElementId::NamedInteger("entry".into(), i as u64))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py(px(3.))
                    .mx_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .hover(|s| s.bg(rgb(BG_HOVER)));

                if is_selected {
                    row = row.bg(rgb(BG_SURFACE));
                }

                row.child(div().w(px(20.)).text_center().child(icon))
                    .child(
                        div()
                            .flex_1()
                            .overflow_x_hidden()
                            .text_color(if is_dir { rgb(ACCENT) } else { rgb(TEXT_PRIMARY) })
                            .child(name),
                    )
                    .child(
                        div()
                            .w(px(80.))
                            .text_color(rgb(TEXT_MUTED))
                            .text_right()
                            .child(size_display),
                    )
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        if is_dir {
                            this.navigate_to(path.clone(), window, cx);
                        } else {
                            this.selected_index = Some(i);
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect()
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,grove=debug")
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    info!("tracing initialized");
}

fn main() {
    init_tracing();

    Application::new().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1000.), px(650.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Grove".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.open_window(options, |_window, cx| cx.new(|_| GroveApp::new()))
            .unwrap();

        cx.activate(true);
    });
}
