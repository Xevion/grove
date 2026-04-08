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
use crate::ui::status_bar::{TextMeasureCache, TruncationKey};

pub struct GroveApp {
    pub(crate) current_dir: PathBuf,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) visible_entries: Vec<usize>,
    pub(crate) bookmarks: Vec<Bookmark>,
    pub(crate) selected_index: Option<usize>,
    pub(crate) is_loading: bool,
    pub(crate) show_hidden: bool,
    pub(crate) scroll_handle: UniformListScrollHandle,
    pub(crate) focus_handle: FocusHandle,
    loading_task: Option<Task<()>>,
    needs_initial_load: bool,
    pub(crate) measure_cache: TextMeasureCache,
    pub(crate) truncation_cache: Option<TruncationKey>,
    pub(crate) truncation_result: (String, String),
}

impl GroveApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            current_dir: env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            entries: Vec::new(),
            visible_entries: vec![],
            bookmarks: default_bookmarks(),
            selected_index: None,
            loading_task: None,
            is_loading: true,
            show_hidden: false,
            needs_initial_load: true,
            scroll_handle: UniformListScrollHandle::default(),
            focus_handle: cx.focus_handle(),
            measure_cache: TextMeasureCache::new(),
            truncation_cache: None,
            truncation_result: (String::new(), String::new()),
        }
    }

    pub(crate) fn rebuild_visible(&mut self) {
        self.visible_entries = self.entries.iter()
            .enumerate()
            .filter(|(_, e)| self.show_hidden || !e.name.starts_with('.'))
            .map(|(i, _)| i)
            .collect();
        if let Some(i) = self.selected_index
            && i >= self.visible_entries.len()
        {
            self.selected_index = self.visible_entries.len().checked_sub(1);
        }
    }

    fn select_offset(&mut self, delta: isize) {
        let count = self.visible_entries.len();
        if count == 0 {
            return;
        }
        let next = match self.selected_index {
            None => {
                if delta >= 0 { 0 } else { count - 1 }
            }
            Some(i) => {
                let next = i as isize + delta;
                next.clamp(0, count as isize - 1) as usize
            }
        };
        self.selected_index = Some(next);
        self.scroll_handle.scroll_to_item(next, ScrollStrategy::Center);
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(vi) = self.selected_index else { return };
        let Some(&ei) = self.visible_entries.get(vi) else { return };
        let entry = &self.entries[ei];
        let path = entry.path.clone();
        if entry.is_dir {
            self.navigate_to(path, window, cx);
        } else {
            let _ = open::that_detached(&path);
        }
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let ctrl = event.keystroke.modifiers.control;

        match (key, ctrl) {
            ("down", false) | ("j", false) => {
                self.select_offset(1);
                cx.notify();
            }
            ("up", false) | ("k", false) => {
                self.select_offset(-1);
                cx.notify();
            }
            ("enter", false) => {
                self.open_selected(window, cx);
            }
            ("backspace", false) => {
                self.navigate_up(window, cx);
            }
            ("h", true) | (".", false) => {
                self.show_hidden = !self.show_hidden;
                self.rebuild_visible();
                cx.notify();
            }
            ("escape", false) => {
                self.selected_index = None;
                cx.notify();
            }
            _ => {}
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
                            this.rebuild_visible();
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
                this.rebuild_visible();
                this.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
                this.is_loading = false;
                cx.notify();
            });
        });

        self.loading_task = Some(task);
        cx.notify();
    }

    pub(crate) fn navigate_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        debug!(path = %path.display(), "navigate_to");
        self.start_loading(path, window, cx);
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
            window.focus(&self.focus_handle);
            let cwd = self.current_dir.clone();
            self.start_loading(cwd, window, cx);
        }

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
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
            .child(self.render_status_bar(window, cx))
    }
}
