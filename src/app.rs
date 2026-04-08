use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{
    actions, div, px, rgb, App, AppContext, Context, DragMoveEvent, FocusHandle,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Pixels, Render, ScrollStrategy,
    StatefulInteractiveElement, Styled, Task, UniformListScrollHandle, Window,
};
use tracing::{debug, info, instrument};

use crate::fs::{Elapsed, FileEntry, merge_sorted, read_directory_bg};
use crate::model::{Bookmark, default_bookmarks};
use crate::theme::{BG_BASE, BORDER_COLOR, TEXT_PRIMARY};
use crate::ui::column_table::{ColumnDef, ColumnTableState, ColumnWidth, EmptyDrag};
use crate::ui::status_bar::{TextMeasureCache, TruncationKey};

/// Marker type for sidebar resize drags.
struct SidebarResize;

actions!(
    grove,
    [MoveDown, MoveUp, Open, NavigateUp, ToggleHidden, Deselect]
);

impl Eq for MoveDown {}
impl Eq for MoveUp {}
impl Eq for Open {}
impl Eq for NavigateUp {}
impl Eq for ToggleHidden {}
impl Eq for Deselect {}

pub fn register_keybindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("j", MoveDown, Some("Grove")),
        KeyBinding::new("down", MoveDown, Some("Grove")),
        KeyBinding::new("k", MoveUp, Some("Grove")),
        KeyBinding::new("up", MoveUp, Some("Grove")),
        KeyBinding::new("enter", Open, Some("Grove")),
        KeyBinding::new("backspace", NavigateUp, Some("Grove")),
        KeyBinding::new("ctrl-h", ToggleHidden, Some("Grove")),
        KeyBinding::new(".", ToggleHidden, Some("Grove")),
        KeyBinding::new("escape", Deselect, Some("Grove")),
    ]);
}

pub const SIDEBAR_DEFAULT_WIDTH: Pixels = px(200.);
pub const SIDEBAR_MIN_WIDTH: Pixels = px(120.);
pub const SIDEBAR_MAX_WIDTH: Pixels = px(400.);

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
    loading_cancel: Option<Arc<AtomicBool>>,
    needs_initial_load: bool,
    pub(crate) measure_cache: TextMeasureCache,
    pub(crate) truncation_cache: Option<TruncationKey>,
    pub(crate) truncation_result: (String, String),
    pub(crate) column_state: ColumnTableState,
    pub(crate) sidebar_width: Pixels,
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
            loading_cancel: None,
            is_loading: true,
            show_hidden: false,
            needs_initial_load: true,
            scroll_handle: UniformListScrollHandle::default(),
            focus_handle: cx.focus_handle(),
            measure_cache: TextMeasureCache::new(),
            truncation_cache: None,
            truncation_result: (String::new(), String::new()),
            column_state: ColumnTableState::new(vec![
                ColumnDef {
                    id: "icon",
                    label: "",
                    width: ColumnWidth::Fixed(px(24.)),
                    min_width: px(24.),
                },
                ColumnDef {
                    id: "name",
                    label: "Name",
                    width: ColumnWidth::Flex(1.0),
                    min_width: px(100.),
                },
                ColumnDef {
                    id: "size",
                    label: "Size",
                    width: ColumnWidth::Fixed(px(80.)),
                    min_width: px(50.),
                },
                ColumnDef {
                    id: "modified",
                    label: "Modified",
                    width: ColumnWidth::Fixed(px(120.)),
                    min_width: px(80.),
                },
            ]),
            sidebar_width: SIDEBAR_DEFAULT_WIDTH,
        }
    }

    pub(crate) fn rebuild_visible(&mut self) {
        self.visible_entries = self
            .entries
            .iter()
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
        let next = self.selected_index.map_or_else(
            || if delta >= 0 { 0 } else { count - 1 },
            |i| {
                let next = i.cast_signed() + delta;
                next.clamp(0, count.cast_signed() - 1).cast_unsigned()
            },
        );
        self.selected_index = Some(next);
        self.scroll_handle
            .scroll_to_item(next, ScrollStrategy::Center);
    }

    fn move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_offset(1);
        cx.notify();
    }

    fn move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_offset(-1);
        cx.notify();
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let Some(vi) = self.selected_index else {
            return;
        };
        let Some(&ei) = self.visible_entries.get(vi) else {
            return;
        };
        let entry = &self.entries[ei];
        let path = entry.path.clone();
        if entry.is_dir {
            self.navigate_to(path, window, cx);
        } else {
            let _ = open::that_detached(&path);
        }
    }

    fn navigate_up_action(&mut self, _: &NavigateUp, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_up(window, cx);
    }

    fn toggle_hidden(&mut self, _: &ToggleHidden, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_hidden = !self.show_hidden;
        self.rebuild_visible();
        cx.notify();
    }

    fn deselect(&mut self, _: &Deselect, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = None;
        cx.notify();
    }

    #[instrument(skip(self, window, cx), fields(path = %path.display()))]
    fn start_loading(&mut self, path: PathBuf, window: &Window, cx: &mut Context<Self>) {
        let t0 = Instant::now();
        info!("loading directory");

        // Signal any in-flight background reader to stop
        if let Some(cancel) = self.loading_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }

        self.current_dir.clone_from(&path);
        self.selected_index = None;
        self.entries.clear();
        self.visible_entries.clear();
        self.is_loading = true;
        self.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);

        let (tx, mut rx) = mpsc::channel::<Vec<FileEntry>>(8);
        let cancel = Arc::new(AtomicBool::new(false));
        self.loading_cancel = Some(Arc::clone(&cancel));

        cx.background_spawn(async move {
            read_directory_bg(path, tx, cancel).await;
        })
        .detach();

        let task = cx.spawn_in(window, async move |weak, cx| {
            let t_recv_start = Instant::now();
            let mut batch_count = 0u32;

            while let Some(batch) = rx.next().await {
                batch_count += 1;
                let ok = weak
                    .update_in(cx, |this, _window, cx| {
                        merge_sorted(&mut this.entries, batch);
                        this.rebuild_visible();
                        cx.notify();
                    })
                    .is_ok();
                if !ok {
                    return;
                }
            }

            let t_recv = t_recv_start.elapsed();
            let _ = weak.update_in(cx, |this, _window, cx| {
                let t_total = t0.elapsed();
                debug!(
                    count = this.entries.len(),
                    batches = batch_count,
                    recv = %Elapsed(t_recv),
                    total = %Elapsed(t_total),
                    "directory load complete"
                );
                this.is_loading = false;
                cx.notify();
            });
        });

        self.loading_task = Some(task);
        cx.notify();
    }

    pub(crate) fn navigate_to(
        &mut self,
        path: PathBuf,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        debug!(path = %path.display(), "navigate_to");
        self.start_loading(path, window, cx);
    }

    pub(crate) fn navigate_up(&mut self, window: &Window, cx: &mut Context<Self>) {
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
            .key_context("Grove")
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::navigate_up_action))
            .on_action(cx.listener(Self::toggle_hidden))
            .on_action(cx.listener(Self::deselect))
            .flex()
            .flex_col()
            .bg(rgb(BG_BASE))
            .text_color(rgb(TEXT_PRIMARY))
            .size_full()
            .font_family("sans-serif")
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .id("content-row")
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        div()
                            .id("sidebar-resize")
                            .w(px(4.))
                            .h_full()
                            .flex_none()
                            .cursor_col_resize()
                            .border_r_1()
                            .border_color(rgb(BORDER_COLOR))
                            .hover(|s| s.bg(rgb(BORDER_COLOR)))
                            .on_drag(SidebarResize, |_, _, _window, cx: &mut App| {
                                cx.new(|_| EmptyDrag)
                            }),
                    )
                    .child(self.render_file_list(cx))
                    .on_drag_move::<SidebarResize>(cx.listener(
                        |this, event: &DragMoveEvent<SidebarResize>, _window, cx| {
                            let x = event.event.position.x;
                            this.sidebar_width =
                                x.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                            cx.notify();
                        },
                    )),
            )
            .child(self.render_status_bar(window, cx))
    }
}
