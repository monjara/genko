use gpui::{Bounds, Pixels, point, px, size};
use settings::{AppSettings, ColumnNumberMode};

const AUTOMATIC_ROWS_RESERVED_CELLS: usize = 4;

pub(crate) fn column_number_header_height(mode: ColumnNumberMode, cell_size: f32) -> Pixels {
    if mode == ColumnNumberMode::Hidden {
        Pixels::ZERO
    } else {
        px((cell_size * 0.8).round().max(18.0))
    }
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

pub(crate) fn visible_columns_for_window_width(
    width: Pixels,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> usize {
    ((width / px(cell_size + ruby_gutter_size)).floor() as usize)
        .saturating_sub(2)
        .max(1)
}

pub(crate) fn rows_per_column_for_window_height(height: Pixels, cell_size: f32) -> usize {
    ((height / px(cell_size)).floor() as usize)
        .saturating_sub(AUTOMATIC_ROWS_RESERVED_CELLS)
        .clamp(1, AppSettings::default_rows_per_column())
}

pub(crate) fn content_height_for_window_height(
    height: Pixels,
    mode: ColumnNumberMode,
    cell_size: f32,
) -> Pixels {
    (height - column_number_header_height(mode, cell_size)).max(Pixels::ZERO)
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

fn board_x_for_visible_column(
    board_left: Pixels,
    column: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Pixels {
    board_left + px(column as f32 * (cell_size + ruby_gutter_size))
}
