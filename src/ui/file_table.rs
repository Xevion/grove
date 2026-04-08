use gpui::{App, Context, IntoElement, ParentElement, Pixels, Styled, Window, div, px, rems, rgb};
use gpui_component::table::{Column, ColumnSort, TableDelegate, TableState};

use crate::fs::FileEntry;
use crate::icons::{Icon, IconName};
use crate::theme::{ACCENT, TEXT_MUTED, TEXT_PRIMARY};

/// How a column determines its width.
#[derive(Clone, Debug)]
pub enum ColumnKind {
    /// Fixed pixel width, non-resizable (e.g. icon column).
    Fixed(Pixels),
    /// Content-fit: a sensible default width, non-resizable (e.g. size, modified).
    ContentFit(Pixels),
    /// Fills remaining space after fixed/content-fit columns are subtracted.
    /// If `pinned` is `Some`, the user has manually resized this column.
    Fill { min: Pixels, pinned: Option<Pixels> },
}

/// Specification for a single column in the file table.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct ColumnSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ColumnKind,
    pub sortable: bool,
    pub movable: bool,
    pub selectable: bool,
    pub text_right: bool,
}

pub const COLUMN_SPECS: &[ColumnSpec] = &[
    ColumnSpec {
        id: "icon",
        name: "",
        kind: ColumnKind::Fixed(px(28.)),
        sortable: false,
        movable: false,
        selectable: false,
        text_right: false,
    },
    ColumnSpec {
        id: "name",
        name: "Name",
        kind: ColumnKind::Fill {
            min: px(120.),
            pinned: None,
        },
        sortable: true,
        movable: false,
        selectable: true,
        text_right: false,
    },
    ColumnSpec {
        id: "size",
        name: "Size",
        kind: ColumnKind::ContentFit(px(80.)),
        sortable: false,
        movable: false,
        selectable: true,
        text_right: true,
    },
    ColumnSpec {
        id: "modified",
        name: "Modified",
        kind: ColumnKind::ContentFit(px(140.)),
        sortable: false,
        movable: false,
        selectable: true,
        text_right: true,
    },
];

/// Resolve column widths given available container width.
#[must_use]
pub fn resolve_widths(specs: &[ColumnSpec], container_width: Pixels) -> Vec<Pixels> {
    let non_fill: Pixels = specs
        .iter()
        .map(|s| match &s.kind {
            ColumnKind::Fixed(w)
            | ColumnKind::ContentFit(w)
            | ColumnKind::Fill {
                pinned: Some(w), ..
            } => *w,
            ColumnKind::Fill { .. } => px(0.),
        })
        .sum();

    let fill_count = specs
        .iter()
        .filter(|s| matches!(s.kind, ColumnKind::Fill { pinned: None, .. }))
        .count();

    let remaining = (container_width - non_fill).max(px(0.));
    let per_fill = if fill_count > 0 {
        #[allow(clippy::cast_precision_loss)]
        let per_fill = remaining / fill_count as f32;
        per_fill
    } else {
        px(0.)
    };

    specs
        .iter()
        .map(|s| match &s.kind {
            ColumnKind::Fixed(w)
            | ColumnKind::ContentFit(w)
            | ColumnKind::Fill {
                pinned: Some(w), ..
            } => *w,
            ColumnKind::Fill { min, pinned: None } => per_fill.max(*min),
        })
        .collect()
}

/// Delegate powering the gpui-component `DataTable` for the file list.
pub struct FileTableDelegate {
    pub entries: Vec<FileEntry>,
    /// Indices into `entries` after filtering (e.g. hidden files).
    pub visible: Vec<usize>,
    pub show_hidden: bool,
    pub is_loading: bool,
    /// Measured container width from the parent's `on_prepaint`.
    pub container_width: Pixels,
    /// Column specs with mutable pinning state.
    pub column_specs: Vec<ColumnSpec>,
}

impl Default for FileTableDelegate {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTableDelegate {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            visible: Vec::new(),
            show_hidden: false,
            is_loading: false,
            container_width: px(0.),
            column_specs: COLUMN_SPECS.to_vec(),
        }
    }

    /// Returns resolved pixel widths for all columns.
    fn resolved_widths(&self) -> Vec<Pixels> {
        resolve_widths(&self.column_specs, self.container_width)
    }

    /// Pin the Name column (index 1) to a specific pixel width after user resize.
    pub fn pin_name_column(&mut self, width: Pixels) {
        if let Some(spec) = self.column_specs.get_mut(1)
            && let ColumnKind::Fill { pinned, .. } = &mut spec.kind
        {
            *pinned = Some(width);
        }
    }

    /// Reset the Name column to fill mode.
    pub fn unpin_name_column(&mut self) {
        if let Some(spec) = self.column_specs.get_mut(1)
            && let ColumnKind::Fill { pinned, .. } = &mut spec.kind
        {
            *pinned = None;
        }
    }

    pub fn rebuild_visible(&mut self) {
        self.visible = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.show_hidden || !e.name.starts_with('.'))
            .map(|(i, _)| i)
            .collect();
    }

    fn entry(&self, row_ix: usize) -> &FileEntry {
        &self.entries[self.visible[row_ix]]
    }
}

impl TableDelegate for FileTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.column_specs.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.visible.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let spec = &self.column_specs[col_ix];
        let widths = self.resolved_widths();
        let w = widths[col_ix];

        let resizable = matches!(spec.kind, ColumnKind::Fill { .. });

        let mut col = Column::new(spec.id, spec.name)
            .width(w)
            .resizable(resizable)
            .movable(spec.movable)
            .selectable(spec.selectable);
        if spec.sortable {
            col = col.sortable();
        }
        if spec.text_right {
            col = col.text_right();
        }
        // For fill columns, set a min width
        if let ColumnKind::Fill { min, .. } = &spec.kind {
            col = col.min_width(*min);
        }
        col
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let entry = self.entry(row_ix);
        match col_ix {
            0 => div()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    entry
                        .icon()
                        .color(rgb(if entry.is_dir { ACCENT } else { TEXT_MUTED }).into()),
                )
                .into_any_element(),
            1 => div()
                .truncate()
                .text_color(if entry.is_dir {
                    rgb(ACCENT)
                } else {
                    rgb(TEXT_PRIMARY)
                })
                .child(entry.name.clone())
                .into_any_element(),
            2 => div()
                .text_color(rgb(TEXT_MUTED))
                .child(entry.size_display.clone())
                .into_any_element(),
            3 => div()
                .text_color(rgb(TEXT_MUTED))
                .child(entry.modified_display.clone())
                .into_any_element(),
            _ => unreachable!(),
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        let ascending = matches!(sort, ColumnSort::Ascending | ColumnSort::Default);
        if col_ix == 1 {
            self.visible.sort_by(|&a, &b| {
                let ea = &self.entries[a];
                let eb = &self.entries[b];
                let ord = match (ea.is_dir, eb.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
                };
                if ascending { ord } else { ord.reverse() }
            });
        }
    }

    fn loading(&self, _cx: &App) -> bool {
        self.is_loading
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_2()
            .py_8()
            .text_color(rgb(TEXT_MUTED))
            .child(
                Icon::new(IconName::FolderOpen)
                    .size(rems(2.0))
                    .color(rgb(TEXT_MUTED).into()),
            )
            .child("Empty directory")
    }
}
