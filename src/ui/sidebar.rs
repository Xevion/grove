use gpui::{
    div, px, rgb, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};

use crate::app::GroveApp;
use crate::theme::{BG_HOVER, BORDER_COLOR, SIDEBAR_BG, TEXT_MUTED, TEXT_PRIMARY};

impl GroveApp {
    pub(crate) fn render_sidebar(&self, cx: &Context<Self>) -> impl IntoElement {
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
                .id(SharedString::from(format!("bm-{label}")))
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
                bookmark_el = bookmark_el.on_click(cx.listener(move |this, _event, window, cx| {
                    this.navigate_to(path.clone(), window, cx);
                }));
            }

            sidebar = sidebar.child(bookmark_el);
        }

        sidebar
    }
}
