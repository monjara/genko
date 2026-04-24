use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, IntoElement,
    LayoutId, Pixels, Style, TextAlign, TextRun, Window, fill, point, px, rgb, rgba, size,
};
use rope::CellText;
use settings::AppSettings;
use theme::{GRID_LINE, PAPER_BACKGROUND, SELECTION_BACKGROUND, TEXT_PRIMARY};

use crate::{AUTOMATIC_ROWS_RESERVED_CELLS, CELL_SIZE, Editor, RUBY_GUTTER_SIZE};

pub(crate) struct EditorCanvas {
    pub(crate) editor: Entity<Editor>,
}

impl EditorCanvas {
    pub(crate) fn new(editor: Entity<Editor>) -> Self {
        Self { editor }
    }
}

impl IntoElement for EditorCanvas {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorCanvas {
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
        let editor = self.editor.read(cx);
        let mut style = Style::default();
        style.size.width = board_width_for_columns(editor.state.visible_columns()).into();
        style.size.height = px(CELL_SIZE * editor.state.rows_per_column() as f32).into();
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
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        self.editor.update(cx, |editor, _cx| {
            editor.last_board_bounds = Some(bounds);
        });

        let show_grid = AppSettings::global(cx).show_grid_lines;
        let (
            visible_text,
            selected_range,
            marked_range,
            block_selection,
            cursor_index,
            scroll_column,
            rows_per_column,
            visible_columns,
        ) = {
            let editor = self.editor.read(cx);
            (
                editor.state.visible_text(),
                editor.state.selected_range.clone(),
                editor.state.marked_range.clone(),
                editor.state.block_selection,
                editor.state.cursor_cell,
                editor.state.scroll_column,
                editor.state.rows_per_column(),
                editor.state.visible_columns(),
            )
        };

        paint_paper(bounds, window);
        paint_selection(
            &visible_text,
            &selected_range,
            marked_range.as_ref(),
            block_selection,
            bounds,
            scroll_column,
            rows_per_column,
            visible_columns,
            window,
        );
        if show_grid {
            paint_grid(bounds, rows_per_column, visible_columns, window);
        }
        paint_text(
            &visible_text,
            bounds,
            scroll_column,
            rows_per_column,
            visible_columns,
            window,
            cx,
        );
        if focus_handle.is_focused(window) {
            paint_cursor(
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

pub(crate) fn paint_paper(bounds: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(fill(bounds, rgb(PAPER_BACKGROUND)));
}

pub(crate) fn paint_grid(
    bounds: Bounds<Pixels>,
    rows_per_column: usize,
    visible_columns: usize,
    window: &mut Window,
) {
    for column in 0..visible_columns {
        let column_left = board_x_for_visible_column(bounds.left(), column);
        let column_right = column_left + px(CELL_SIZE);
        let has_ruby_gutter = column + 1 < visible_columns;

        window.paint_quad(fill(
            Bounds::new(
                point(column_left, bounds.top()),
                size(px(1.0), bounds.size.height),
            ),
            rgb(GRID_LINE),
        ));
        window.paint_quad(fill(
            Bounds::new(
                point(column_right, bounds.top()),
                size(px(1.0), bounds.size.height),
            ),
            rgb(GRID_LINE),
        ));

        for row in 0..=rows_per_column {
            let y = bounds.top() + px(row as f32 * CELL_SIZE);
            window.paint_quad(fill(
                Bounds::new(point(column_left, y), size(px(CELL_SIZE), px(1.0))),
                rgb(GRID_LINE),
            ));
        }

        if has_ruby_gutter {
            window.paint_quad(fill(
                Bounds::new(
                    point(column_right, bounds.top()),
                    size(px(RUBY_GUTTER_SIZE), px(1.0)),
                ),
                rgb(GRID_LINE),
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(column_right, bounds.bottom() - px(1.0)),
                    size(px(RUBY_GUTTER_SIZE), px(1.0)),
                ),
                rgb(GRID_LINE),
            ));
        }
    }
}

pub(crate) fn paint_selection(
    visible_text: &[CellText],
    selected_range: &Range<usize>,
    marked_range: Option<&Range<usize>>,
    block_selection: Option<crate::editor_state::BlockSelection>,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    rows_per_column: usize,
    visible_columns: usize,
    window: &mut Window,
) {
    if let Some(block_selection) = block_selection {
        for logical_index in block_selection_indices(
            block_selection.anchor_cell,
            block_selection.cursor_cell,
            rows_per_column,
        ) {
            let Some(cell_bounds) = cell_bounds_for_logical_index(
                bounds,
                logical_index,
                scroll_column,
                rows_per_column,
                visible_columns,
            ) else {
                continue;
            };
            window.paint_quad(fill(cell_bounds, rgba(SELECTION_BACKGROUND)));
        }
    }

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
            window.paint_quad(fill(cell_bounds, rgba(SELECTION_BACKGROUND)));
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
                rgb(TEXT_PRIMARY),
            ));
        }
    }
}

pub(crate) fn paint_text(
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
            paint_attached_punctuation(cell_text, cell_bounds, window, cx);
        } else {
            paint_cell_text(cell_text, cell_bounds, window, cx);
        }
    }
}

fn paint_cell_text(
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
        color: rgb(TEXT_PRIMARY).into(),
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
    line.paint(
        text_origin,
        line_height,
        TextAlign::Center,
        None,
        window,
        cx,
    )
    .ok();
}

fn paint_attached_punctuation(
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
        color: rgb(TEXT_PRIMARY).into(),
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
    line.paint(
        text_origin,
        line_height,
        TextAlign::Center,
        None,
        window,
        cx,
    )
    .ok();
}

pub(crate) fn paint_cursor(
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
        rgb(TEXT_PRIMARY),
    ));
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

pub(crate) fn block_selection_indices(
    anchor_cell: usize,
    cursor_cell: usize,
    rows_per_column: usize,
) -> impl Iterator<Item = usize> {
    let rows_per_column = rows_per_column.max(1);
    let anchor_row = anchor_cell % rows_per_column;
    let anchor_column = anchor_cell / rows_per_column;
    let cursor_row = cursor_cell % rows_per_column;
    let cursor_column = cursor_cell / rows_per_column;
    let row_start = anchor_row.min(cursor_row);
    let row_end = anchor_row.max(cursor_row);
    let column_start = anchor_column.min(cursor_column);
    let column_end = anchor_column.max(cursor_column);

    (column_start..=column_end).flat_map(move |column| {
        (row_start..=row_end).map(move |row| column * rows_per_column + row)
    })
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
            board_x_for_visible_column(board_bounds.left(), column),
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

    let local_x = position.x - board_bounds.left();
    let stride = px(CELL_SIZE + RUBY_GUTTER_SIZE);
    let column = (local_x / stride)
        .floor()
        .clamp(0.0, (visible_columns - 1) as f32) as usize;
    let column_offset = local_x - px(column as f32 * (CELL_SIZE + RUBY_GUTTER_SIZE));
    if column_offset > px(CELL_SIZE) {
        return None;
    }
    let row = ((position.y - board_bounds.top()) / px(CELL_SIZE))
        .floor()
        .clamp(0.0, (rows_per_column - 1) as f32) as usize;
    let column_from_right = visible_columns - 1 - column;
    Some((first_visible_column + column_from_right) * rows_per_column + row)
}

pub(crate) fn board_width_for_columns(visible_columns: usize) -> Pixels {
    if visible_columns == 0 {
        return Pixels::ZERO;
    }

    px(visible_columns as f32 * CELL_SIZE
        + visible_columns.saturating_sub(1) as f32 * RUBY_GUTTER_SIZE)
}

pub(crate) fn board_x_for_visible_column(board_left: Pixels, column: usize) -> Pixels {
    board_left + px(column as f32 * (CELL_SIZE + RUBY_GUTTER_SIZE))
}

pub(crate) fn visible_columns_for_window_width(width: Pixels) -> usize {
    (((width + px(RUBY_GUTTER_SIZE)) / px(CELL_SIZE + RUBY_GUTTER_SIZE)).floor() as usize)
        .saturating_sub(2)
        .max(1)
}

pub(crate) fn rows_per_column_for_window_height(height: Pixels) -> usize {
    ((height / px(CELL_SIZE)).floor() as usize)
        .saturating_sub(AUTOMATIC_ROWS_RESERVED_CELLS)
        .clamp(1, AppSettings::max_rows_per_column())
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

    #[test]
    fn cell_bounds_leave_ruby_gutter_between_columns() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(200.0)));

        let left_column = cell_bounds_for_logical_index(
            bounds,
            DEFAULT_ROWS_PER_COLUMN,
            0,
            DEFAULT_ROWS_PER_COLUMN,
            2,
        )
        .unwrap();
        let right_column =
            cell_bounds_for_logical_index(bounds, 0, 0, DEFAULT_ROWS_PER_COLUMN, 2).unwrap();

        assert_eq!(left_column.left(), px(0.0));
        assert_eq!(right_column.left(), px(CELL_SIZE + RUBY_GUTTER_SIZE));
    }

    #[test]
    fn click_in_ruby_gutter_does_not_target_main_cell() {
        let bounds = Bounds::new(
            point(px(0.0), px(0.0)),
            size(board_width_for_columns(2), px(200.0)),
        );
        let gutter_point = point(px(CELL_SIZE + RUBY_GUTTER_SIZE / 2.0), px(8.0));

        assert_eq!(
            logical_index_for_point(bounds, gutter_point, 0, DEFAULT_ROWS_PER_COLUMN, 2),
            None
        );
    }
}
