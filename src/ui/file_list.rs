use std::ops::Range;

use gpui::*;

use crate::app::GroveApp;
use crate::theme::*;

impl GroveApp {
    pub(crate) fn render_file_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(crate) fn render_entry_range(
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
