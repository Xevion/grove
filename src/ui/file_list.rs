use std::ops::Range;

use gpui::{
    div, px, rems, rgb, uniform_list, AnyElement, ClickEvent, Context, DragMoveEvent, ElementId,
    InteractiveElement, IntoElement, MouseUpEvent, ParentElement, StatefulInteractiveElement,
    Styled, Window,
};

use crate::app::GroveApp;
use crate::icons::{Icon, IconName};
use crate::theme::{ACCENT, BG_HOVER, BG_SELECTED, BG_SELECTED_HOVER, TEXT_MUTED, TEXT_PRIMARY};
use crate::ui::column_table::{ColumnResize, FILE_LIST_FONT_PX, HANDLE_WIDTH};
use crate::ui::status_bar::smart_truncate_px;

/// Row horizontal inset: `px_3` (12px × 2 sides).
const ROW_INSET_PX: f32 = 24.0;

/// Sidebar resize handle width.
const SIDEBAR_HANDLE_PX: f32 = 4.0;

impl GroveApp {
    /// Computes the content width available for column cells within a row.
    fn row_content_width(&self, window: &Window) -> gpui::Pixels {
        let viewport = window.viewport_size().width;
        (viewport - self.sidebar_width - px(SIDEBAR_HANDLE_PX) - px(ROW_INSET_PX)).max(px(0.))
    }

    pub(crate) fn render_file_list(&self, cx: &Context<Self>) -> AnyElement {
        let entry_count = self.visible_entries.len();

        if entry_count == 0 && !self.is_loading {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .gap_2()
                .py_8()
                .text_color(rgb(TEXT_MUTED))
                .child(Icon::new(IconName::FolderOpen).size(rems(2.0)).color(rgb(TEXT_MUTED).into()))
                .child("Empty directory")
                .into_any_element();
        }

        let mut container = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .on_drag_move::<ColumnResize>(cx.listener(
                |this, event: &DragMoveEvent<ColumnResize>, window, cx| {
                    let current_x = event.event.position.x;
                    let col_resize = event.drag(cx);
                    let index = col_resize.index;
                    let content_width = this.row_content_width(window);

                    if let Some(last_x) = this.column_state.drag_last_x {
                        let delta = current_x - last_x;
                        let consumed = this.column_state.apply_resize(index, delta, content_width);
                        this.column_state.drag_last_x = Some(last_x + consumed);
                    } else {
                        // First drag event: pin any Flex columns adjacent to the handle
                        this.column_state.pin_flex_column(index, content_width);
                        if index + 1 < this.column_state.columns.len() {
                            this.column_state.pin_flex_column(index + 1, content_width);
                        }
                        this.column_state.drag_last_x = Some(current_x);
                    }
                    cx.notify();
                },
            ))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(
                |this, _event: &MouseUpEvent, _window, _cx| {
                    this.column_state.drag_last_x = None;
                },
            ));

        container = container.child(self.column_state.render_header());

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
                    cx.processor(|this, range: Range<usize>, window, cx| {
                        this.render_entry_range(range, window, cx)
                    }),
                )
                .flex_1()
                .track_scroll(&self.scroll_handle),
            )
            .into_any_element()
    }

    pub(crate) fn render_entry_range(
        &mut self,
        range: Range<usize>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let content_width = self.row_content_width(window);

        // Find the name column index and its resolved pixel width for smart truncation.
        let name_col = self.column_state.columns.iter().position(|c| c.id == "name");
        let name_budget = name_col
            .map_or(px(0.), |idx| self.column_state.resolve_column_width(idx, content_width));

        range
            .map(|i| {
                let entry = &self.entries[self.visible_entries[i]];
                let path = entry.path.clone();
                let is_dir = entry.is_dir;
                let is_selected = self.selected_index == Some(i);

                // Smart-truncate the filename to fit the name column's resolved width.
                let display_name = smart_truncate_px(
                    &mut self.measure_cache,
                    window,
                    &entry.name,
                    name_budget,
                    FILE_LIST_FONT_PX,
                );

                let mut row = div()
                    .id(ElementId::NamedInteger("entry".into(), i as u64))
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .py(px(3.))
                    .cursor_pointer()
                    .text_sm();

                if is_selected {
                    row = row
                        .bg(rgb(BG_SELECTED))
                        .hover(|s| s.bg(rgb(BG_SELECTED_HOVER)));
                } else {
                    row = row.hover(|s| s.bg(rgb(BG_HOVER)));
                }

                let col_count = self.column_state.columns.len();
                for (col_idx, col) in self.column_state.columns.iter().enumerate() {
                    let cell = match col.id {
                        "icon" => div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(entry.icon().color(rgb(if is_dir {
                                ACCENT
                            } else {
                                TEXT_MUTED
                            }).into())),
                        "name" => div()
                            .min_w_0()
                            .text_color(if is_dir {
                                rgb(ACCENT)
                            } else {
                                rgb(TEXT_PRIMARY)
                            })
                            .child(display_name.clone()),
                        "size" => div()
                            .text_color(rgb(TEXT_MUTED))
                            .text_right()
                            .child(entry.size_display.clone()),
                        "modified" => div()
                            .text_color(rgb(TEXT_MUTED))
                            .text_right()
                            .child(entry.modified_display.clone()),
                        _ => div(),
                    };

                    let cell = self.column_state.style_cell(col_idx, cell);
                    row = row.child(cell);

                    // Spacer between cells to match header drag handles
                    if col_idx < col_count - 1 {
                        row = row.child(div().w(HANDLE_WIDTH).flex_none());
                    }
                }

                row.on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    if event.click_count() >= 2 {
                        if is_dir {
                            this.navigate_to(path.clone(), window, cx);
                        } else {
                            let _ = open::that_detached(&path);
                        }
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
