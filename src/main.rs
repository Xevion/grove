use std::cmp::Ordering;
use std::path::PathBuf;
use std::{env, fs};

use gpui::*;

const BG_BASE: u32 = 0x1e1e2e;
const BG_SURFACE: u32 = 0x313244;
const BG_HOVER: u32 = 0x45475a;
const TEXT_PRIMARY: u32 = 0xcdd6f4;
const TEXT_SECONDARY: u32 = 0xa6adc8;
const TEXT_MUTED: u32 = 0x6c7086;
const ACCENT: u32 = 0x89b4fa;
const SIDEBAR_BG: u32 = 0x181825;
const BORDER_COLOR: u32 = 0x313244;

#[derive(Clone)]
struct FileEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    size: u64,
}

impl FileEntry {
    fn display_size(&self) -> String {
        if self.is_dir {
            return "—".into();
        }
        match self.size {
            s if s < 1024 => format!("{s} B"),
            s if s < 1024 * 1024 => format!("{:.1} KB", s as f64 / 1024.0),
            s if s < 1024 * 1024 * 1024 => format!("{:.1} MB", s as f64 / (1024.0 * 1024.0)),
            s => format!("{:.1} GB", s as f64 / (1024.0 * 1024.0 * 1024.0)),
        }
    }

    fn icon(&self) -> &'static str {
        if self.is_dir {
            "📁"
        } else {
            "📄"
        }
    }
}


#[derive(Clone)]
struct Bookmark {
    label: &'static str,
    path: PathBuf,
}

fn default_bookmarks() -> Vec<Bookmark> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    vec![
        Bookmark {
            label: "Home",
            path: home.clone(),
        },
        Bookmark {
            label: "Desktop",
            path: home.join("Desktop"),
        },
        Bookmark {
            label: "Documents",
            path: home.join("Documents"),
        },
        Bookmark {
            label: "Downloads",
            path: home.join("Downloads"),
        },
        Bookmark {
            label: "Projects",
            path: home.join("projects"),
        },
        Bookmark {
            label: "/",
            path: PathBuf::from("/"),
        },
    ]
}


struct GroveApp {
    current_dir: PathBuf,
    entries: Vec<FileEntry>,
    bookmarks: Vec<Bookmark>,
    selected_index: Option<usize>,
}

impl GroveApp {
    fn new() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut app = Self {
            current_dir: cwd.clone(),
            entries: Vec::new(),
            bookmarks: default_bookmarks(),
            selected_index: None,
        };
        app.read_directory(&cwd);
        app
    }

    fn read_directory(&mut self, path: &PathBuf) {
        self.current_dir = path.clone();
        self.selected_index = None;
        self.entries.clear();

        let Ok(read_dir) = fs::read_dir(path) else {
            return;
        };

        for entry in read_dir.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            self.entries.push(FileEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                is_dir: meta.is_dir(),
                size: meta.len(),
            });
        }

        // Directories first, then alphabetical
        self.entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
    }

    fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.read_directory(&path.clone());
        }
    }

    fn navigate_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.read_directory(&parent.clone());
        }
    }
}


impl Render for GroveApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

impl GroveApp {
    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.navigate_up();
                        cx.notify();
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

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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

        for bookmark in self.bookmarks.clone() {
            let path = bookmark.path.clone();
            let exists = path.exists();

            let mut bookmark_el = div()
                .id(SharedString::from(format!("bm-{}", bookmark.label)))
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
                .child(bookmark.label);

            if exists {
                bookmark_el = bookmark_el.on_click(cx.listener(move |this, _event, _window, cx| {
                    this.navigate_to(path.clone());
                    cx.notify();
                }));
            }

            sidebar = sidebar.child(bookmark_el);
        }

        sidebar
    }

    fn render_file_list(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div()
            .id("file-list")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .py_1();

        if self.entries.is_empty() {
            return list.child(
                div()
                    .flex()
                    .size_full()
                    .justify_center()
                    .items_center()
                    .py_8()
                    .text_color(rgb(TEXT_MUTED))
                    .text_sm()
                    .child("Empty directory"),
            );
        }

        for (i, entry) in self.entries.clone().iter().enumerate() {
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            let is_selected = self.selected_index == Some(i);

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

            let row = row
                .child(
                    div()
                        .w(px(20.))
                        .text_center()
                        .child(entry.icon()),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_x_hidden()
                        .text_color(if is_dir {
                            rgb(ACCENT)
                        } else {
                            rgb(TEXT_PRIMARY)
                        })
                        .child(entry.name.clone()),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_color(rgb(TEXT_MUTED))
                        .text_right()
                        .child(entry.display_size()),
                )
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    if is_dir {
                        this.navigate_to(path.clone());
                    } else {
                        this.selected_index = Some(i);
                    }
                    cx.notify();
                }));

            list = list.child(row);
        }

        list
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1000.), px(650.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Grove".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.open_window(options, |_window, cx| cx.new(|_| GroveApp::new()))
            .unwrap();

        cx.activate(true);
    });
}
