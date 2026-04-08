use std::path::PathBuf;

use crate::ui::file_table::ColumnKind;
use gpui::{
    App, AppContext, Context, DragMoveEvent, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, Subscription,
    Window, actions, div, px, rgb,
};
use gpui_component::ElementExt as _;
use gpui_component::table::{DataTable, TableEvent, TableState};

#[cfg(not(target_family = "wasm"))]
use gpui::Task;
#[cfg(not(target_family = "wasm"))]
use std::sync::Arc;
#[cfg(not(target_family = "wasm"))]
use std::sync::atomic::AtomicBool;
use tracing::debug;

use crate::fs::FileEntry;
use crate::model::{Bookmark, default_bookmarks};
use crate::theme::{BG_BASE, BORDER_COLOR, TEXT_PRIMARY};
use crate::ui::column_table::EmptyDrag;
use crate::ui::file_table::FileTableDelegate;
use crate::ui::status_bar::{TextMeasureCache, TruncationKey};

#[cfg(not(target_family = "wasm"))]
use crate::fs::{Elapsed, merge_sorted, read_directory_bg};
#[cfg(not(target_family = "wasm"))]
use futures::StreamExt;
#[cfg(not(target_family = "wasm"))]
use futures::channel::mpsc;
#[cfg(not(target_family = "wasm"))]
use std::sync::atomic::Ordering;
#[cfg(not(target_family = "wasm"))]
use tracing::{info, instrument};

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
    pub current_dir: PathBuf,
    pub bookmarks: Vec<Bookmark>,
    pub show_hidden: bool,
    pub focus_handle: FocusHandle,
    pub table_state: Option<Entity<TableState<FileTableDelegate>>>,
    table_subscription: Option<Subscription>,
    #[cfg(not(target_family = "wasm"))]
    loading_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    loading_cancel: Option<Arc<AtomicBool>>,
    needs_initial_load: bool,
    pub measure_cache: TextMeasureCache,
    pub truncation_cache: Option<TruncationKey>,
    pub truncation_result: (String, String),
    pub sidebar_width: Pixels,
}

impl GroveApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        #[cfg(not(target_family = "wasm"))]
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        #[cfg(target_family = "wasm")]
        let current_dir = PathBuf::new();

        Self {
            current_dir,
            bookmarks: default_bookmarks(),
            show_hidden: false,
            #[cfg(not(target_family = "wasm"))]
            loading_task: None,
            #[cfg(not(target_family = "wasm"))]
            loading_cancel: None,
            needs_initial_load: true,
            focus_handle: cx.focus_handle(),
            table_state: None,
            table_subscription: None,
            measure_cache: TextMeasureCache::new(),
            truncation_cache: None,
            truncation_result: (String::new(), String::new()),
            sidebar_width: SIDEBAR_DEFAULT_WIDTH,
        }
    }

    /// Ensures the table state entity exists, creating it on first call.
    fn ensure_table_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.table_state.is_none() {
            let delegate = FileTableDelegate::new();
            let state = cx.new(|cx| TableState::new(delegate, window, cx));

            let sub = cx.subscribe_in(&state, window, |this, table_state, event: &TableEvent, window, cx| {
                match event {
                    TableEvent::DoubleClickedRow(row_ix) => {
                        let entry_info = {
                            let state = table_state.read(cx);
                            let d = state.delegate();
                            d.visible.get(*row_ix).map(|&ei| {
                                let entry = &d.entries[ei];
                                (entry.path.clone(), entry.is_dir)
                            })
                        };
                        if let Some((path, is_dir)) = entry_info {
                            if is_dir {
                                this.navigate_to(path, window, cx);
                            } else {
                                #[cfg(not(target_family = "wasm"))]
                                if let Err(e) = open::that_detached(&path) {
                                    tracing::warn!(path = %path.display(), error = %e, "failed to open file");
                                }
                            }
                        }
                    }
                    TableEvent::ColumnWidthsChanged(widths) => {
                        // When the user resizes a column, pin any Fill columns to their new width
                        table_state.update(cx, |state, cx| {
                            let d = state.delegate_mut();
                            for (ix, &new_width) in widths.iter().enumerate() {
                                if let Some(spec) = d.column_specs.get_mut(ix) {
                                    if let ColumnKind::Fill { pinned, .. } = &mut spec.kind {
                                        *pinned = Some(new_width);
                                    }
                                }
                            }
                            state.refresh(cx);
                        });
                    }
                    _ => {}
                }
            });

            self.table_state = Some(state);
            self.table_subscription = Some(sub);
        }
    }

    /// Runs a closure on the table delegate, notifying the table state afterward.
    fn with_delegate(&self, cx: &mut Context<Self>, f: impl FnOnce(&mut FileTableDelegate)) {
        if let Some(state) = &self.table_state {
            state.update(cx, |state, cx| {
                f(state.delegate_mut());
                cx.notify();
            });
        }
    }

    pub fn rebuild_visible(&mut self, cx: &mut Context<Self>) {
        let show_hidden = self.show_hidden;
        self.with_delegate(cx, |d| {
            d.show_hidden = show_hidden;
            d.rebuild_visible();
        });
    }

    #[allow(clippy::unused_self, clippy::missing_const_for_fn)]
    fn move_down(&mut self, _: &MoveDown, _window: &mut Window, _cx: &mut Context<Self>) {
        // DataTable handles keyboard navigation internally
    }

    #[allow(clippy::unused_self, clippy::missing_const_for_fn)]
    fn move_up(&mut self, _: &MoveUp, _window: &mut Window, _cx: &mut Context<Self>) {
        // DataTable handles keyboard navigation internally
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let entry_info = self.table_state.as_ref().and_then(|state| {
            let state = state.read(cx);
            let row = state.selected_row()?;
            let d = state.delegate();
            let vi = *d.visible.get(row)?;
            let entry = &d.entries[vi];
            Some((entry.path.clone(), entry.is_dir))
        });
        let Some((path, is_dir)) = entry_info else {
            return;
        };
        if is_dir {
            self.navigate_to(path, window, cx);
        } else {
            #[cfg(not(target_family = "wasm"))]
            if let Err(e) = open::that_detached(&path) {
                tracing::warn!(path = %path.display(), error = %e, "failed to open file");
            }
        }
    }

    fn navigate_up_action(&mut self, _: &NavigateUp, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_up(window, cx);
    }

    fn toggle_hidden(&mut self, _: &ToggleHidden, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_hidden = !self.show_hidden;
        self.rebuild_visible(cx);
    }

    fn deselect(&mut self, _: &Deselect, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = &self.table_state {
            state.update(cx, |state, cx| {
                state.clear_selection(cx);
            });
        }
    }

    /// Cancels any in-flight load, spawns a background directory read for `path`,
    /// and returns a channel receiver for the entry batches.
    #[cfg(not(target_family = "wasm"))]
    fn spawn_directory_read(
        &mut self,
        path: PathBuf,
        cx: &Context<Self>,
    ) -> mpsc::Receiver<Vec<FileEntry>> {
        if let Some(cancel) = self.loading_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }

        let (tx, rx) = mpsc::channel::<Vec<FileEntry>>(8);
        let cancel = Arc::new(AtomicBool::new(false));
        self.loading_cancel = Some(Arc::clone(&cancel));

        cx.background_spawn(async move {
            read_directory_bg(path, tx, cancel).await;
        })
        .detach();

        rx
    }

    #[cfg(not(target_family = "wasm"))]
    #[instrument(skip(self, window, cx), fields(path = %path.display()))]
    fn start_loading(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let t0 = std::time::Instant::now();
        info!("loading directory");

        self.ensure_table_state(window, cx);
        self.current_dir.clone_from(&path);

        // Clear delegate state
        let show_hidden = self.show_hidden;
        self.with_delegate(cx, |d| {
            d.entries.clear();
            d.visible.clear();
            d.is_loading = true;
            d.show_hidden = show_hidden;
        });
        if let Some(state) = &self.table_state {
            state.update(cx, |state, cx| {
                state.clear_selection(cx);
            });
        }

        let mut rx = self.spawn_directory_read(path, cx);

        let Some(table) = self.table_state.clone() else {
            return;
        };
        let task = cx.spawn_in(window, async move |weak, cx| {
            let t_recv_start = std::time::Instant::now();
            let mut batch_count = 0u32;

            while let Some(batch) = rx.next().await {
                batch_count += 1;
                let ok = table
                    .update_in(cx, |state, _window, cx| {
                        let d = state.delegate_mut();
                        merge_sorted(&mut d.entries, batch);
                        d.rebuild_visible();
                        cx.notify();
                    })
                    .is_ok();
                if !ok {
                    return;
                }
            }

            let t_recv = t_recv_start.elapsed();
            let count = table
                .update_in(cx, |state, _window, cx| {
                    let d = state.delegate_mut();
                    d.is_loading = false;
                    let count = d.entries.len();
                    cx.notify();
                    count
                })
                .unwrap_or(0);

            let _ = weak.update(cx, |_this, cx| {
                let t_total = t0.elapsed();
                debug!(
                    count,
                    batches = batch_count,
                    recv = %Elapsed(t_recv),
                    total = %Elapsed(t_total),
                    "directory load complete"
                );
                cx.notify();
            });
        });

        self.loading_task = Some(task);
        cx.notify();
    }

    #[cfg(target_family = "wasm")]
    fn start_loading(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_table_state(window, cx);
        self.current_dir.clone_from(&path);
        let entries = crate::fs::mock_entries_for(&path);
        let show_hidden = self.show_hidden;
        self.with_delegate(cx, |d| {
            d.entries = entries;
            d.is_loading = false;
            d.show_hidden = show_hidden;
            d.rebuild_visible();
        });
        if let Some(state) = &self.table_state {
            state.update(cx, |state, cx| {
                state.clear_selection(cx);
            });
        }
        cx.notify();
    }

    pub fn navigate_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        debug!(path = %path.display(), "navigate_to");
        self.start_loading(path, window, cx);
    }

    pub fn navigate_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = self.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.start_loading(parent, window, cx);
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn refresh_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.current_dir.clone();
        let t0 = std::time::Instant::now();
        debug!(path = %path.display(), "refresh_current");

        self.ensure_table_state(window, cx);

        let selected_path = self.table_state.as_ref().and_then(|state| {
            let state = state.read(cx);
            let row = state.selected_row()?;
            let d = state.delegate();
            let vi = *d.visible.get(row)?;
            Some(d.entries[vi].path.clone())
        });

        let mut rx = self.spawn_directory_read(path, cx);

        let Some(table) = self.table_state.clone() else {
            return;
        };
        let show_hidden = self.show_hidden;
        let task = cx.spawn_in(window, async move |_weak, cx| {
            let t_recv_start = std::time::Instant::now();
            let mut new_entries = Vec::new();
            let mut batch_count = 0u32;

            while let Some(batch) = rx.next().await {
                batch_count += 1;
                merge_sorted(&mut new_entries, batch);
            }

            let t_recv = t_recv_start.elapsed();
            let _ = table.update_in(cx, |state, _window, cx| {
                let t_total = t0.elapsed();
                let d = state.delegate_mut();
                debug!(
                    count = new_entries.len(),
                    batches = batch_count,
                    recv = %Elapsed(t_recv),
                    total = %Elapsed(t_total),
                    "refresh complete"
                );

                d.entries = new_entries;
                d.show_hidden = show_hidden;
                d.rebuild_visible();
                d.is_loading = false;

                if let Some(ref sel_path) = selected_path
                    && let Some(row) = d
                        .visible
                        .iter()
                        .position(|&ei| d.entries[ei].path == *sel_path)
                {
                    state.set_selected_row(row, cx);
                }

                cx.notify();
            });
        });

        self.loading_task = Some(task);
        cx.notify();
    }

    #[cfg(target_family = "wasm")]
    pub fn refresh_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.current_dir.clone();
        self.start_loading(path, window, cx);
    }
}

impl Render for GroveApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_initial_load {
            self.needs_initial_load = false;
            window.focus(&self.focus_handle, cx);
            let cwd = self.current_dir.clone();
            self.start_loading(cwd, window, cx);
        }

        let file_list = self.table_state.as_ref().map_or_else(
            || div().into_any_element(),
            |table_state| {
                DataTable::new(table_state)
                    .bordered(false)
                    .into_any_element()
            },
        );

        let table_entity = self.table_state.clone();

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
                    .child(
                        div()
                            .id("file-table-container")
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .overflow_hidden()
                            .on_prepaint({
                                move |bounds, _window, cx| {
                                    if let Some(ref table) = table_entity {
                                        table.update(cx, |state, cx| {
                                            let d = state.delegate_mut();
                                            let new_width = bounds.size.width;
                                            if (d.container_width - new_width).abs() > px(1.) {
                                                d.container_width = new_width;
                                                state.refresh(cx);
                                            }
                                        });
                                    }
                                }
                            })
                            .child(file_list),
                    )
                    .on_drag_move::<SidebarResize>(cx.listener(
                        |this, event: &DragMoveEvent<SidebarResize>, _window, cx| {
                            let x = event.event.position.x;
                            this.sidebar_width = x.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                            cx.notify();
                        },
                    )),
            )
            .child(self.render_status_bar(window, cx))
    }
}
