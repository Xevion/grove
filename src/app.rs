use std::env;
use std::path::PathBuf;
use std::time::Instant;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::*;
use tracing::{debug, info, instrument};

use crate::fs::{BATCH_SIZE, Elapsed, FileEntry, read_directory_bg, sort_entries};
use crate::model::{Bookmark, default_bookmarks};
use crate::theme::*;

pub struct GroveApp {
    pub(crate) current_dir: PathBuf,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) bookmarks: Vec<Bookmark>,
    pub(crate) selected_index: Option<usize>,
    pub(crate) is_loading: bool,
    pub(crate) scroll_handle: UniformListScrollHandle,
    loading_task: Option<Task<()>>,
    needs_initial_load: bool,
}

impl GroveApp {
    pub fn new() -> Self {
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

    pub(crate) fn navigate_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let t0 = Instant::now();
        let is_dir = path.is_dir();
        let t_stat = t0.elapsed();
        debug!(path = %path.display(), is_dir, stat = %Elapsed(t_stat), "navigate_to");
        if is_dir {
            self.start_loading(path, window, cx);
        }
    }

    pub(crate) fn navigate_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = self.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.start_loading(parent, window, cx);
        }
    }
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
