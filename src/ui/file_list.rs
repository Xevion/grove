use std::ops::Range;

use gpui::{
    div, px, rgb, uniform_list, AnyElement, ClickEvent, Context, ElementId, FontWeight,
    InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled,
};

use crate::app::GroveApp;
use crate::theme::{ACCENT, BG_HOVER, BG_SURFACE, BORDER_COLOR, TEXT_MUTED, TEXT_PRIMARY};

impl GroveApp {
    pub(crate) fn render_file_list(&self, cx: &Context<Self>) -> AnyElement {
        let entry_count = self.visible_entries.len();

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

        // Sticky column header — sits above the scrollable list
        container = container.child(self.render_column_header());

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

    #[allow(clippy::unused_self)]
    fn render_column_header(&self) -> impl IntoElement {
        // Mirrors the exact padding/gap/widths used in render_entry_range
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .mx_1()
            .border_b_1()
            .border_color(rgb(BORDER_COLOR))
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(TEXT_MUTED))
            // Icon column spacer
            .child(div().w(px(20.)))
            // Name column
            .child(div().flex_1().child("Name"))
            // Size column
            .child(div().w(px(80.)).text_right().child("Size"))
    }

    pub(crate) fn render_entry_range(
        &self,
        range: Range<usize>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .map(|i| {
                let entry = &self.entries[self.visible_entries[i]];
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

                row.child(div().w(px(20.)).flex_none().text_center().child(icon))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if is_dir {
                                rgb(ACCENT)
                            } else {
                                rgb(TEXT_PRIMARY)
                            })
                            .child(name),
                    )
                    .child(
                        div()
                            .w(px(80.))
                            .flex_none()
                            .text_color(rgb(TEXT_MUTED))
                            .text_right()
                            .child(size_display),
                    )
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.click_count() >= 2 || is_dir {
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
