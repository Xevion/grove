use gpui::{
    div, px, rgb, AnyElement, App, AppContext, Context, Div, ElementId, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled,
    Window,
};

use crate::theme::{BORDER_COLOR, TEXT_MUTED};

// Deferred features (designed-for but not wired up):
// - Click-to-sort: add `sortable: bool` and `sort_direction: Option<SortDir>` to ColumnDef
// - Column visibility toggle: add `visible: bool` to ColumnDef, skip hidden columns in rendering
// - Column reordering: drag-drop on headers to reindex columns Vec
// - Persist settings: serialize widths/visibility/order to config file

#[derive(Clone)]
pub enum ColumnWidth {
    Fixed(Pixels),
    Flex(f32),
}

pub struct ColumnDef {
    pub id: &'static str,
    pub label: &'static str,
    pub width: ColumnWidth,
    pub min_width: Pixels,
}

/// Marker type for column resize drags. The `index` field identifies
/// which column is to the left of the drag handle.
pub struct ColumnResize {
    pub index: usize,
}

/// Invisible drag visual required by gpui's `on_drag` API.
pub struct EmptyDrag;

impl Render for EmptyDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

/// Width of spacer divs between columns (matches drag handle width in header).
pub const HANDLE_WIDTH: Pixels = px(5.);

/// `text_sm` = rems(0.875) → 14px at default 16px/rem
pub const FILE_LIST_FONT_PX: f32 = 14.0;

pub struct ColumnTableState {
    pub columns: Vec<ColumnDef>,
    /// Tracks the last mouse X position during a column resize drag.
    pub drag_last_x: Option<Pixels>,
}

impl ColumnTableState {
    pub const fn new(columns: Vec<ColumnDef>) -> Self {
        Self {
            columns,
            drag_last_x: None,
        }
    }

    /// Total width consumed by spacer divs between columns.
    fn spacer_total(&self) -> Pixels {
        #[allow(clippy::cast_precision_loss)]
        px(self.columns.len().saturating_sub(1) as f32 * f32::from(HANDLE_WIDTH))
    }

    /// Sum of all Fixed column widths (clamped to min) and total Flex ratio.
    fn sum_widths(&self) -> (Pixels, f32) {
        let mut fixed = px(0.);
        let mut flex = 0.0_f32;
        for col in &self.columns {
            match col.width {
                ColumnWidth::Fixed(w) => fixed += w.max(col.min_width),
                ColumnWidth::Flex(r) => flex += r,
            }
        }
        (fixed, flex)
    }

    /// Resolves the pixel width of a column given the total content width
    /// available for all columns (i.e., row content area minus padding/margins).
    pub fn resolve_column_width(&self, index: usize, content_width: Pixels) -> Pixels {
        let col = &self.columns[index];
        match col.width {
            ColumnWidth::Fixed(w) => w.max(col.min_width),
            ColumnWidth::Flex(ratio) => {
                let (fixed_sum, flex_total) = self.sum_widths();
                let flex_space = content_width - fixed_sum - self.spacer_total();
                let flex_space_f: f32 = flex_space.max(px(0.)).into();
                if flex_total > 0.0 {
                    px(flex_space_f * (ratio / flex_total)).max(col.min_width)
                } else {
                    col.min_width
                }
            }
        }
    }

    /// Applies the correct width styling to a div based on a column's width config.
    /// Fixed columns get a fixed pixel width; flex columns get `flex_grow` with their ratio.
    pub fn style_cell(&self, index: usize, cell: Div) -> Div {
        let col = &self.columns[index];
        let cell = cell.overflow_hidden().whitespace_nowrap();
        match col.width {
            ColumnWidth::Fixed(w) => {
                let clamped = w.max(col.min_width);
                cell.w(clamped).max_w(clamped).flex_none()
            }
            ColumnWidth::Flex(ratio) => {
                let mut styled = cell.flex_basis(px(0.)).min_w(col.min_width).flex_shrink();
                styled.style().flex_grow = Some(ratio);
                styled
            }
        }
    }

    /// Renders the column header row with drag handles between columns.
    pub fn render_header(&self) -> AnyElement {
        let mut header = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(rgb(BORDER_COLOR))
            .text_xs()
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(rgb(TEXT_MUTED));

        for (i, col) in self.columns.iter().enumerate() {
            let cell = div().truncate().child(SharedString::from(col.label));

            let cell = if col.id == "size" {
                cell.text_right()
            } else {
                cell
            };

            let cell = self.style_cell(i, cell);
            header = header.child(cell);

            // Drag handle between columns (not after the last)
            if i < self.columns.len() - 1 {
                let handle = div()
                    .id(ElementId::NamedInteger("col-resize".into(), i as u64))
                    .w(HANDLE_WIDTH)
                    .h_full()
                    .flex_none()
                    .cursor_col_resize()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id(ElementId::NamedInteger("col-divider".into(), i as u64))
                            .w(px(1.))
                            .h_full()
                            .bg(rgb(BORDER_COLOR))
                            .hover(|s| s.bg(rgb(TEXT_MUTED))),
                    )
                    .on_drag(
                        ColumnResize { index: i },
                        |_, _, _window, cx: &mut App| cx.new(|_| EmptyDrag),
                    );

                header = header.child(handle);
            }
        }

        header.into_any_element()
    }

    /// Pins a Flex column to Fixed at its current resolved width.
    /// Called at drag start so delta math works uniformly.
    pub fn pin_flex_column(&mut self, index: usize, content_width: Pixels) {
        if let ColumnWidth::Flex(_) = self.columns[index].width {
            let resolved = self.resolve_column_width(index, content_width);
            self.columns[index].width = ColumnWidth::Fixed(resolved);
        }
    }

    /// Applies a drag delta (in pixels) to resize columns around the handle at `index`.
    /// `content_width` is the row content area width (used to resolve Flex column widths
    /// for `min_width` clamping). Returns the actual delta consumed.
    ///
    /// Both columns adjacent to the handle must be `Fixed` before calling this
    /// (use `pin_flex_column` at drag start).
    pub fn apply_resize(&mut self, index: usize, delta: Pixels, content_width: Pixels) -> Pixels {
        let len = self.columns.len();
        if index >= len {
            return px(0.);
        }

        let left_headroom = match self.columns[index].width {
            ColumnWidth::Fixed(w) => (w - self.columns[index].min_width).max(px(0.)),
            ColumnWidth::Flex(_) => {
                let resolved = self.resolve_column_width(index, content_width);
                (resolved - self.columns[index].min_width).max(px(0.))
            }
        };

        let right_headroom = if index + 1 < len {
            match self.columns[index + 1].width {
                ColumnWidth::Fixed(w) => (w - self.columns[index + 1].min_width).max(px(0.)),
                ColumnWidth::Flex(_) => {
                    let resolved = self.resolve_column_width(index + 1, content_width);
                    (resolved - self.columns[index + 1].min_width).max(px(0.))
                }
            }
        } else {
            px(0.)
        };

        let clamped = delta.clamp(-left_headroom, right_headroom);

        if let ColumnWidth::Fixed(w) = &mut self.columns[index].width {
            *w += clamped;
        }
        if index + 1 < len {
            if let ColumnWidth::Fixed(w) = &mut self.columns[index + 1].width {
                *w -= clamped;
            }
        }

        clamped
    }
}
