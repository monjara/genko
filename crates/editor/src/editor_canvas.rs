use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, Font, FontFeatures,
    GlobalElementId, IntoElement, LayoutId, Pixels, Style, TextAlign, TextRun, Window, fill, point,
    px, rgb, rgba, size,
};
use rope::CellText;
use settings::AppSettings;
use theme::{APP_FONT_FAMILY, GRID_LINE, PAPER_BACKGROUND, SELECTION_BACKGROUND, TEXT_PRIMARY};

use crate::{AUTOMATIC_ROWS_RESERVED_CELLS, Editor};

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
        style.size.width = board_width_for_columns(
            editor.state.visible_columns(),
            editor.state.cell_size(),
            editor.state.ruby_gutter_size(),
        )
        .into();
        style.size.height =
            px(editor.state.cell_size() * editor.state.visible_rows() as f32).into();
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
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
        ) = {
            let editor = self.editor.read(cx);
            (
                editor.state.visible_text(),
                editor.state.selected_range.clone(),
                editor.state.marked_range.clone(),
                editor.state.block_selection,
                editor.state.cursor_cell,
                editor.state.scroll_column,
                editor.state.scroll_row,
                editor.state.rows_per_column(),
                editor.state.visible_columns(),
                editor.state.visible_rows(),
                editor.state.cell_size(),
                editor.state.ruby_gutter_size(),
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
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
            window,
        );
        if show_grid {
            paint_grid(
                bounds,
                rows_per_column,
                visible_columns,
                scroll_row,
                visible_rows,
                cell_size,
                ruby_gutter_size,
                window,
            );
        }
        paint_text(
            &visible_text,
            bounds,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
            window,
            cx,
        );
        if focus_handle.is_focused(window) {
            paint_cursor(
                cursor_index,
                bounds,
                scroll_column,
                scroll_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
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
    _rows_per_column: usize,
    visible_columns: usize,
    _first_visible_row: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
) {
    let bottom_border_y = bounds.top() + px(visible_rows as f32 * cell_size);

    for column in 0..visible_columns {
        let column_left =
            board_x_for_visible_column(bounds.left(), column, cell_size, ruby_gutter_size);
        let column_right = column_left + px(cell_size);
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

        for row in 0..=visible_rows {
            let y = bounds.top() + px(row as f32 * cell_size);
            window.paint_quad(fill(
                Bounds::new(point(column_left, y), size(px(cell_size), px(1.0))),
                rgb(GRID_LINE),
            ));
        }

        if has_ruby_gutter {
            window.paint_quad(fill(
                Bounds::new(
                    point(column_right, bounds.top()),
                    size(px(ruby_gutter_size), px(1.0)),
                ),
                rgb(GRID_LINE),
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(column_right, bottom_border_y),
                    size(px(ruby_gutter_size), px(1.0)),
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
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
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
                first_visible_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
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
                first_visible_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
            ) else {
                continue;
            };
            window.paint_quad(fill(cell_bounds, rgba(SELECTION_BACKGROUND)));
        } else if marked_range.is_some_and(|range| ranges_overlap(&cell_text.range, range)) {
            let Some(cell_bounds) = cell_bounds_for_logical_index(
                bounds,
                cell_text.logical_index,
                scroll_column,
                first_visible_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
            ) else {
                continue;
            };
            let underline_y = cell_bounds.bottom() - px(4.0);
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        cell_bounds.left() + px((cell_size * 0.18).round()),
                        underline_y,
                    ),
                    size(px((cell_size * 0.64).round()), px(2.0)),
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
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    for cell_text in visible_text {
        let Some(cell_bounds) = cell_bounds_for_logical_index(
            bounds,
            cell_text.logical_index,
            scroll_column,
            first_visible_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
        ) else {
            continue;
        };

        if cell_text.attached_to_previous {
            paint_attached_punctuation(cell_text, cell_bounds, window, cx);
        } else {
            paint_cell_text(cell_text, cell_bounds, cell_size, window, cx);
        }
    }
}

fn paint_cell_text(
    cell_text: &CellText,
    cell_bounds: Bounds<Pixels>,
    cell_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if is_corner_punctuation(&cell_text.text) {
        paint_corner_punctuation(cell_text, cell_bounds, window, cx, true);
        return;
    }

    let style = window.text_style();
    let font_size = px((cell_size * 0.75).round());
    let line_height = px((cell_size * 0.86).round());
    let run = TextRun {
        len: cell_text.text.len(),
        font: vertical_text_font(style.font()),
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
        cell_bounds.left() + (px(cell_size) - line.width) / 2.0,
        cell_bounds.top() + (px(cell_size) - line_height) / 2.0,
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
    let cell_size = cell_bounds.size.width.as_f32();
    if is_corner_punctuation(&cell_text.text) {
        paint_corner_punctuation(cell_text, cell_bounds, window, cx, false);
        return;
    }

    let style = window.text_style();
    let font_size = px((cell_size * 0.5).round());
    let line_height = px((cell_size * 0.57).round());
    let run = TextRun {
        len: cell_text.text.len(),
        font: vertical_text_font(style.font()),
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

fn is_corner_punctuation(text: &str) -> bool {
    matches!(text, "。" | "、")
}

fn paint_corner_punctuation(
    cell_text: &CellText,
    cell_bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
    align_top: bool,
) {
    let cell_size = cell_bounds.size.width.as_f32();
    let style = window.text_style();
    let font_size = px((cell_size * 0.5).round());
    let line_height = px((cell_size * 0.57).round());
    let run = TextRun {
        len: cell_text.text.len(),
        font: vertical_text_font(style.font()),
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
        cell_bounds.left() + (px(cell_size) - line.width) / 2.0,
        if align_top {
            cell_bounds.top() + (px(cell_size) - line_height) / 2.0
        } else {
            cell_bounds.bottom() - (px(cell_size) - line_height) / 2.0
        },
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

fn vertical_text_font(mut font: Font) -> Font {
    font.family = APP_FONT_FAMILY.into();
    font.features = FontFeatures::vertical_alternates();
    font
}

pub(crate) fn paint_cursor(
    cursor_index: usize,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
) {
    let Some(cell_bounds) = cell_bounds_for_logical_index(
        bounds,
        cursor_index,
        scroll_column,
        first_visible_row,
        rows_per_column,
        visible_columns,
        visible_rows,
        cell_size,
        ruby_gutter_size,
    ) else {
        return;
    };
    window.paint_quad(fill(
        Bounds::new(
            point(cell_bounds.left() + px(4.0), cell_bounds.top() + px(3.0)),
            size(px(cell_size - 8.0), px(2.0)),
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
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Option<Bounds<Pixels>> {
    let (row, column) = row_column_for_logical_index(
        logical_index,
        first_visible_column,
        rows_per_column,
        visible_columns,
    )?;
    if row < first_visible_row || row >= first_visible_row + visible_rows {
        return None;
    }
    Some(Bounds::new(
        point(
            board_x_for_visible_column(board_bounds.left(), column, cell_size, ruby_gutter_size),
            board_bounds.top() + px((row - first_visible_row) as f32 * cell_size),
        ),
        size(px(cell_size), px(cell_size)),
    ))
}

pub(crate) fn logical_index_for_point(
    board_bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
    first_visible_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Option<usize> {
    let rows_per_column = rows_per_column.max(1);
    let visible_columns = visible_columns.max(1);
    if !board_bounds.contains(&position) {
        return None;
    }

    let local_x = position.x - board_bounds.left();
    let stride = px(cell_size + ruby_gutter_size);
    let column = (local_x / stride)
        .floor()
        .clamp(0.0, (visible_columns - 1) as f32) as usize;
    let column_offset = local_x - px(column as f32 * (cell_size + ruby_gutter_size));
    if column_offset > px(cell_size) {
        return None;
    }
    let row = ((position.y - board_bounds.top()) / px(cell_size))
        .floor()
        .clamp(0.0, (visible_rows.saturating_sub(1)) as f32) as usize;
    let column_from_right = visible_columns - 1 - column;
    Some((first_visible_column + column_from_right) * rows_per_column + first_visible_row + row)
}

pub(crate) fn board_width_for_columns(
    visible_columns: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Pixels {
    if visible_columns == 0 {
        return Pixels::ZERO;
    }

    px(visible_columns as f32 * cell_size
        + visible_columns.saturating_sub(1) as f32 * ruby_gutter_size)
}

pub(crate) fn board_x_for_visible_column(
    board_left: Pixels,
    column: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Pixels {
    board_left + px(column as f32 * (cell_size + ruby_gutter_size))
}

pub(crate) fn visible_columns_for_window_width(
    width: Pixels,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> usize {
    (((width + px(ruby_gutter_size)) / px(cell_size + ruby_gutter_size)).floor() as usize)
        .saturating_sub(2)
        .max(1)
}

pub(crate) fn rows_per_column_for_window_height(height: Pixels, cell_size: f32) -> usize {
    ((height / px(cell_size)).floor() as usize)
        .saturating_sub(AUTOMATIC_ROWS_RESERVED_CELLS)
        .clamp(1, AppSettings::max_rows_per_column())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_ROWS_PER_COLUMN: usize = 16;
    const VISIBLE_COLUMNS: usize = 20;
    const TEST_CELL_SIZE: f32 = 28.0;
    const TEST_RUBY_GUTTER_SIZE: f32 = TEST_CELL_SIZE * crate::RUBY_GUTTER_RATIO;

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
            0,
            DEFAULT_ROWS_PER_COLUMN,
            2,
            DEFAULT_ROWS_PER_COLUMN,
            TEST_CELL_SIZE,
            TEST_RUBY_GUTTER_SIZE,
        )
        .unwrap();
        let right_column = cell_bounds_for_logical_index(
            bounds,
            0,
            0,
            0,
            DEFAULT_ROWS_PER_COLUMN,
            2,
            DEFAULT_ROWS_PER_COLUMN,
            TEST_CELL_SIZE,
            TEST_RUBY_GUTTER_SIZE,
        )
        .unwrap();

        assert_eq!(left_column.left(), px(0.0));
        assert_eq!(
            right_column.left(),
            px(TEST_CELL_SIZE + TEST_RUBY_GUTTER_SIZE)
        );
    }

    #[test]
    fn click_in_ruby_gutter_does_not_target_main_cell() {
        let bounds = Bounds::new(
            point(px(0.0), px(0.0)),
            size(
                board_width_for_columns(2, TEST_CELL_SIZE, TEST_RUBY_GUTTER_SIZE),
                px(200.0),
            ),
        );
        let gutter_point = point(px(TEST_CELL_SIZE + TEST_RUBY_GUTTER_SIZE / 2.0), px(8.0));

        assert_eq!(
            logical_index_for_point(
                bounds,
                gutter_point,
                0,
                0,
                DEFAULT_ROWS_PER_COLUMN,
                2,
                DEFAULT_ROWS_PER_COLUMN,
                TEST_CELL_SIZE,
                TEST_RUBY_GUTTER_SIZE,
            ),
            None
        );
    }
}
