use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, IntoElement,
    LayoutId, Pixels, Style, TextRun, Window, fill, point, px, rgb, rgba, size,
};
use rope::CellText;
use settings::AppSettings;

use crate::{CELL_SIZE, GenkoApp};

pub(crate) struct BoardElement {
    pub(crate) app: Entity<GenkoApp>,
}

impl IntoElement for BoardElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BoardElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let app = self.app.read(cx);
        let rows_per_column = app.rows_per_column();
        let visible_columns = app.visible_columns();
        let mut style = Style::default();
        style.size.width = px(CELL_SIZE * visible_columns as f32).into();
        style.size.height = px(CELL_SIZE * rows_per_column as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.app.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.app.clone()),
            cx,
        );
        self.app.update(cx, |app, _cx| {
            app.last_board_bounds = Some(bounds);
        });

        self.paint_paper(bounds, window);
        let show_grid = AppSettings::global(cx).show_grid_lines;

        let (
            visible_text,
            selected_range,
            marked_range,
            cursor_index,
            scroll_column,
            rows_per_column,
            visible_columns,
        ) = {
            let app = self.app.read(cx);
            (
                app.visible_text(),
                app.selected_range.clone(),
                app.marked_range.clone(),
                app.cursor_cell,
                app.scroll_column,
                app.rows_per_column(),
                app.visible_columns(),
            )
        };

        self.paint_selection(
            &visible_text,
            &selected_range,
            marked_range.as_ref(),
            bounds,
            scroll_column,
            rows_per_column,
            visible_columns,
            window,
        );
        if show_grid {
            self.paint_grid(bounds, rows_per_column, visible_columns, window);
        }
        self.paint_text(
            &visible_text,
            bounds,
            scroll_column,
            rows_per_column,
            visible_columns,
            window,
            cx,
        );
        if focus_handle.is_focused(window) {
            self.paint_cursor(
                cursor_index,
                bounds,
                scroll_column,
                rows_per_column,
                visible_columns,
                window,
            );
        }
    }
}

impl BoardElement {
    fn paint_paper(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        window.paint_quad(fill(bounds, rgb(0xfffbf2)));
    }

    fn paint_grid(
        &self,
        bounds: Bounds<Pixels>,
        rows_per_column: usize,
        visible_columns: usize,
        window: &mut Window,
    ) {
        for column in 0..=visible_columns {
            let x = bounds.left() + px(column as f32 * CELL_SIZE);
            window.paint_quad(fill(
                Bounds::new(point(x, bounds.top()), size(px(1.0), bounds.size.height)),
                rgb(0xd94b4b),
            ));
        }

        for row in 0..=rows_per_column {
            let y = bounds.top() + px(row as f32 * CELL_SIZE);
            window.paint_quad(fill(
                Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(1.0))),
                rgb(0xd94b4b),
            ));
        }
    }

    fn paint_selection(
        &self,
        visible_text: &[CellText],
        selected_range: &Range<usize>,
        marked_range: Option<&Range<usize>>,
        bounds: Bounds<Pixels>,
        scroll_column: usize,
        rows_per_column: usize,
        visible_columns: usize,
        window: &mut Window,
    ) {
        for cell_text in visible_text {
            if ranges_overlap(&cell_text.range, selected_range) {
                let Some(cell_bounds) = cell_bounds_for_logical_index(
                    bounds,
                    cell_text.logical_index,
                    scroll_column,
                    rows_per_column,
                    visible_columns,
                ) else {
                    continue;
                };
                window.paint_quad(fill(cell_bounds, rgba(0x2f6fff30)));
            } else if marked_range.is_some_and(|range| ranges_overlap(&cell_text.range, range)) {
                let Some(cell_bounds) = cell_bounds_for_logical_index(
                    bounds,
                    cell_text.logical_index,
                    scroll_column,
                    rows_per_column,
                    visible_columns,
                ) else {
                    continue;
                };
                let underline_y = cell_bounds.bottom() - px(4.0);
                window.paint_quad(fill(
                    Bounds::new(
                        point(cell_bounds.left() + px(5.0), underline_y),
                        size(px(CELL_SIZE - 10.0), px(2.0)),
                    ),
                    rgb(0x2f241d),
                ));
            }
        }
    }

    fn paint_text(
        &self,
        visible_text: &[CellText],
        bounds: Bounds<Pixels>,
        scroll_column: usize,
        rows_per_column: usize,
        visible_columns: usize,
        window: &mut Window,
        cx: &mut App,
    ) {
        for cell_text in visible_text {
            let Some(cell_bounds) = cell_bounds_for_logical_index(
                bounds,
                cell_text.logical_index,
                scroll_column,
                rows_per_column,
                visible_columns,
            ) else {
                continue;
            };

            if cell_text.attached_to_previous {
                self.paint_attached_punctuation(cell_text, cell_bounds, window, cx);
            } else {
                self.paint_cell_text(cell_text, cell_bounds, window, cx);
            }
        }
    }

    fn paint_cell_text(
        &self,
        cell_text: &CellText,
        cell_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = window.text_style();
        let font_size = px(21.0);
        let line_height = px(24.0);
        let run = TextRun {
            len: cell_text.text.len(),
            font: style.font(),
            color: rgb(0x2f241d).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            window
                .text_system()
                .shape_line(cell_text.text.clone().into(), font_size, &[run], None);
        let text_origin = point(
            cell_bounds.left() + (px(CELL_SIZE) - line.width) / 2.0,
            cell_bounds.top() + (px(CELL_SIZE) - line_height) / 2.0,
        );
        line.paint(text_origin, line_height, window, cx).ok();
    }

    fn paint_attached_punctuation(
        &self,
        cell_text: &CellText,
        cell_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = window.text_style();
        let font_size = px(14.0);
        let line_height = px(16.0);
        let run = TextRun {
            len: cell_text.text.len(),
            font: style.font(),
            color: rgb(0x2f241d).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            window
                .text_system()
                .shape_line(cell_text.text.clone().into(), font_size, &[run], None);
        let text_origin = point(
            cell_bounds.right() - line.width - px(3.0),
            cell_bounds.bottom() - line_height - px(1.0),
        );
        line.paint(text_origin, line_height, window, cx).ok();
    }

    fn paint_cursor(
        &self,
        cursor_index: usize,
        bounds: Bounds<Pixels>,
        scroll_column: usize,
        rows_per_column: usize,
        visible_columns: usize,
        window: &mut Window,
    ) {
        let Some(cell_bounds) = cell_bounds_for_logical_index(
            bounds,
            cursor_index,
            scroll_column,
            rows_per_column,
            visible_columns,
        ) else {
            return;
        };
        window.paint_quad(fill(
            Bounds::new(
                point(cell_bounds.left() + px(4.0), cell_bounds.top() + px(3.0)),
                size(px(CELL_SIZE - 8.0), px(2.0)),
            ),
            rgb(0x2f241d),
        ));
    }
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

pub(crate) fn row_column_for_logical_index(
    logical_index: usize,
    first_visible_column: usize,
    rows_per_column: usize,
    visible_columns: usize,
) -> Option<(usize, usize)> {
    let rows_per_column = rows_per_column.max(1);
    let visible_columns = visible_columns.max(1);
    let logical_column = logical_index / rows_per_column;
    if logical_column < first_visible_column {
        return None;
    }

    let column_from_right = logical_column - first_visible_column;
    if column_from_right >= visible_columns {
        return None;
    }

    let row = logical_index % rows_per_column;
    let column = visible_columns - 1 - column_from_right;
    Some((row, column))
}

pub(crate) fn cell_bounds_for_logical_index(
    board_bounds: Bounds<Pixels>,
    logical_index: usize,
    first_visible_column: usize,
    rows_per_column: usize,
    visible_columns: usize,
) -> Option<Bounds<Pixels>> {
    let (row, column) = row_column_for_logical_index(
        logical_index,
        first_visible_column,
        rows_per_column,
        visible_columns,
    )?;
    Some(Bounds::new(
        point(
            board_bounds.left() + px(column as f32 * CELL_SIZE),
            board_bounds.top() + px(row as f32 * CELL_SIZE),
        ),
        size(px(CELL_SIZE), px(CELL_SIZE)),
    ))
}

pub(crate) fn logical_index_for_point(
    board_bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
    first_visible_column: usize,
    rows_per_column: usize,
    visible_columns: usize,
) -> Option<usize> {
    let rows_per_column = rows_per_column.max(1);
    let visible_columns = visible_columns.max(1);
    if !board_bounds.contains(&position) {
        return None;
    }

    let column = ((position.x - board_bounds.left()) / px(CELL_SIZE))
        .floor()
        .clamp(0.0, (visible_columns - 1) as f32) as usize;
    let row = ((position.y - board_bounds.top()) / px(CELL_SIZE))
        .floor()
        .clamp(0.0, (rows_per_column - 1) as f32) as usize;
    let column_from_right = visible_columns - 1 - column;
    Some((first_visible_column + column_from_right) * rows_per_column + row)
}
#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_ROWS_PER_COLUMN: usize = 16;
    const VISIBLE_COLUMNS: usize = 20;

    #[test]
    fn vertical_flow_starts_at_top_right() {
        let rows = DEFAULT_ROWS_PER_COLUMN;

        assert_eq!(
            row_column_for_logical_index(0, 0, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 1))
        );
        assert_eq!(
            row_column_for_logical_index(1, 0, rows, VISIBLE_COLUMNS),
            Some((1, VISIBLE_COLUMNS - 1))
        );
        assert_eq!(
            row_column_for_logical_index(rows, 0, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 2))
        );
    }

    #[test]
    fn virtual_scroll_offsets_visible_columns() {
        let rows = DEFAULT_ROWS_PER_COLUMN;

        assert_eq!(
            row_column_for_logical_index(0, 1, rows, VISIBLE_COLUMNS),
            None
        );
        assert_eq!(
            row_column_for_logical_index(rows, 1, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 1))
        );
        assert_eq!(
            row_column_for_logical_index(rows * VISIBLE_COLUMNS, 1, rows, VISIBLE_COLUMNS),
            Some((0, 0))
        );
        assert_eq!(
            row_column_for_logical_index(rows * (VISIBLE_COLUMNS + 1), 1, rows, VISIBLE_COLUMNS),
            None
        );
    }

    #[test]
    fn vertical_flow_uses_configured_rows_per_column() {
        let rows = 24;

        assert_eq!(
            row_column_for_logical_index(rows, 0, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 2))
        );
        assert_eq!(
            row_column_for_logical_index(rows - 1, 0, rows, VISIBLE_COLUMNS),
            Some((rows - 1, VISIBLE_COLUMNS - 1))
        );
    }
}
