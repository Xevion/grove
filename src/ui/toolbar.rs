use gpui::*;

use crate::app::GroveApp;
use crate::theme::*;

impl GroveApp {
    pub(crate) fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
}
