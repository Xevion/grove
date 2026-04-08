use std::path::PathBuf;

use gpui::{
    AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, rems, rgb,
};

use crate::app::GroveApp;
use crate::icons::{Icon, IconName};
use crate::theme::{BG_HOVER, BG_SURFACE, BORDER_COLOR, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};

impl GroveApp {
    pub(crate) fn render_toolbar(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_3()
            .py_2()
            .h(rems(2.5))
            .bg(rgb(BG_SURFACE))
            .border_b_1()
            .border_color(rgb(BORDER_COLOR))
            .child(
                div()
                    .id("nav-up")
                    .px_1()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(BG_HOVER)))
                    .child(Icon::new(IconName::ArrowUp).color(rgb(TEXT_MUTED).into()))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.navigate_up(window, cx);
                    })),
            )
            .child(self.render_breadcrumb(cx))
            .child(self.render_toolbar_actions(cx))
    }

    fn render_breadcrumb(&self, cx: &Context<Self>) -> AnyElement {
        let mut breadcrumb = div()
            .flex_1()
            .flex()
            .flex_row()
            .items_center()
            .overflow_x_hidden()
            .text_sm();

        let mut segments: Vec<(String, PathBuf)> = Vec::new();
        let mut accumulator = PathBuf::new();
        for component in self.current_dir.components() {
            accumulator.push(component);
            let label = match component {
                std::path::Component::RootDir => "/".to_string(),
                _ => component.as_os_str().to_string_lossy().into_owned(),
            };
            segments.push((label, accumulator.clone()));
        }

        let last_idx = segments.len().saturating_sub(1);
        for (i, (label, path)) in segments.into_iter().enumerate() {
            if i > 0 {
                breadcrumb = breadcrumb.child(
                    Icon::new(IconName::ChevronRight)
                        .size(rems(0.75))
                        .color(rgb(TEXT_MUTED).into()),
                );
            }

            let is_last = i == last_idx;
            breadcrumb = breadcrumb.child(
                div()
                    .id(ElementId::NamedInteger("bc".into(), i as u64))
                    .px_1()
                    .py(gpui::px(2.))
                    .rounded_sm()
                    .cursor_pointer()
                    .text_color(if is_last {
                        rgb(TEXT_PRIMARY)
                    } else {
                        rgb(TEXT_SECONDARY)
                    })
                    .hover(|s| s.bg(rgb(BG_HOVER)))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.navigate_to(path.clone(), window, cx);
                    }))
                    .child(SharedString::from(label)),
            );
        }

        breadcrumb.into_any_element()
    }

    fn render_toolbar_actions(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .flex_none()
            .child(
                div()
                    .id("toggle-hidden")
                    .px_1()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(BG_HOVER)))
                    .child(
                        Icon::new(if self.show_hidden {
                            IconName::Eye
                        } else {
                            IconName::EyeOff
                        })
                        .color(
                            rgb(if self.show_hidden {
                                TEXT_PRIMARY
                            } else {
                                TEXT_MUTED
                            })
                            .into(),
                        ),
                    )
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.show_hidden = !this.show_hidden;
                        this.rebuild_visible(cx);
                    })),
            )
            .child(
                div()
                    .id("refresh")
                    .px_1()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(BG_HOVER)))
                    .child(Icon::new(IconName::Refresh).color(rgb(TEXT_MUTED).into()))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.refresh_current(window, cx);
                    })),
            )
            .into_any_element()
    }
}
