use std::ops::Range;

use editor::Editor;

use crate::state::{BlockInsertKind, BlockRegister};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PastePosition {
    Before,
    After,
}

pub(crate) fn current_column_cell_range(
    cursor_cell: usize,
    rows_per_column: usize,
    used_cells: usize,
) -> Option<Range<usize>> {
    if used_cells == 0 || rows_per_column == 0 {
        return None;
    }

    let target_cell = if cursor_cell >= used_cells {
        used_cells.saturating_sub(1)
    } else {
        cursor_cell
    };
    let column_index = target_cell / rows_per_column;
    let start = column_index * rows_per_column;
    let end = (start + rows_per_column).min(used_cells);
    Some(start..end)
}

pub(crate) fn block_selection_byte_ranges(
    editor: &Editor,
    anchor_cell: usize,
    cursor_cell: usize,
) -> Vec<Range<usize>> {
    let mut ranges =
        block_selection_cell_indices(anchor_cell, cursor_cell, editor.rows_per_column())
            .into_iter()
            .map(|cell| {
                editor.byte_offset_for_display_cell(cell)
                    ..editor.byte_offset_for_display_cell(cell + 1)
            })
            .filter(|range| !range.is_empty())
            .collect::<Vec<_>>();
    ranges.sort_by(|left, right| left.start.cmp(&right.start).then(left.end.cmp(&right.end)));
    ranges.dedup();
    ranges
}

pub(crate) fn build_block_register(
    editor: &Editor,
    anchor_cell: usize,
    cursor_cell: usize,
) -> BlockRegister {
    let rows_per_column = editor.rows_per_column().max(1);
    let anchor_row = anchor_cell % rows_per_column;
    let anchor_column = anchor_cell / rows_per_column;
    let cursor_row = cursor_cell % rows_per_column;
    let cursor_column = cursor_cell / rows_per_column;
    let row_start = anchor_row.min(cursor_row);
    let row_end = anchor_row.max(cursor_row);
    let column_start = anchor_column.min(cursor_column);
    let column_end = anchor_column.max(cursor_column);
    let row_count = row_end - row_start + 1;
    let column_count = column_end - column_start + 1;
    let mut cells = Vec::with_capacity(row_count * column_count);

    for column in column_start..=column_end {
        for row in row_start..=row_end {
            let cell = column * rows_per_column + row;
            let range = editor.byte_offset_for_display_cell(cell)
                ..editor.byte_offset_for_display_cell(cell + 1);
            cells.push(if range.is_empty() {
                String::new()
            } else {
                editor.text_in_range(range)
            });
        }
    }

    BlockRegister {
        row_count,
        column_count,
        cells,
    }
}

pub(crate) fn build_block_register_from_cursor(
    editor: &Editor,
    cursor_cell: usize,
    row_count: usize,
    column_count: usize,
) -> BlockRegister {
    let cells = block_cell_indices_from_cursor(editor, cursor_cell, row_count, column_count)
        .into_iter()
        .map(|cell| {
            let range = editor.byte_offset_for_display_cell(cell)
                ..editor.byte_offset_for_display_cell(cell + 1);
            if range.is_empty() {
                String::new()
            } else {
                editor.text_in_range(range)
            }
        })
        .collect();
    BlockRegister {
        row_count,
        column_count,
        cells,
    }
}

pub(crate) fn block_insert_target_cells(
    editor: &Editor,
    anchor_cell: usize,
    cursor_cell: usize,
    kind: BlockInsertKind,
) -> Vec<usize> {
    let rows_per_column = editor.rows_per_column().max(1);
    let anchor_row = anchor_cell % rows_per_column;
    let anchor_column = anchor_cell / rows_per_column;
    let cursor_row = cursor_cell % rows_per_column;
    let cursor_column = cursor_cell / rows_per_column;
    let row_start = anchor_row.min(cursor_row);
    let row_end = anchor_row.max(cursor_row);
    let target_column = match kind {
        BlockInsertKind::Before => anchor_column.min(cursor_column),
        BlockInsertKind::After => anchor_column.max(cursor_column) + 1,
    };

    (row_start..=row_end)
        .map(|row| target_column * rows_per_column + row)
        .collect()
}

pub(crate) fn block_insert_target_cells_from_cursor(
    editor: &Editor,
    cursor_cell: usize,
    row_count: usize,
    kind: BlockInsertKind,
) -> Vec<usize> {
    let rows_per_column = editor.rows_per_column().max(1);
    let row_start = cursor_cell % rows_per_column;
    let column = cursor_cell / rows_per_column;
    let target_column = match kind {
        BlockInsertKind::Before => column,
        BlockInsertKind::After => column + 1,
    };

    (0..row_count)
        .map(|row_offset| target_column * rows_per_column + row_start + row_offset)
        .collect()
}

pub(crate) fn block_byte_ranges_from_cursor(
    editor: &Editor,
    cursor_cell: usize,
    row_count: usize,
    column_count: usize,
) -> Vec<Range<usize>> {
    let mut ranges = block_cell_indices_from_cursor(editor, cursor_cell, row_count, column_count)
        .into_iter()
        .map(|cell| {
            editor.byte_offset_for_display_cell(cell)..editor.byte_offset_for_display_cell(cell + 1)
        })
        .filter(|range| !range.is_empty())
        .collect::<Vec<_>>();
    ranges.sort_by(|left, right| left.start.cmp(&right.start).then(left.end.cmp(&right.end)));
    ranges.dedup();
    ranges
}

pub(crate) fn block_paste_operations(
    base_cell: usize,
    rows_per_column: usize,
    register: &BlockRegister,
) -> Vec<(usize, String)> {
    let mut operations = Vec::with_capacity(register.cells.len());
    for column_offset in 0..register.column_count {
        for row_offset in 0..register.row_count {
            let index = column_offset * register.row_count + row_offset;
            let text = register.cells.get(index).cloned().unwrap_or_default();
            operations.push((
                base_cell + column_offset * rows_per_column + row_offset,
                text,
            ));
        }
    }
    operations
}

fn block_selection_cell_indices(
    anchor_cell: usize,
    cursor_cell: usize,
    rows_per_column: usize,
) -> Vec<usize> {
    let rows_per_column = rows_per_column.max(1);
    let anchor_row = anchor_cell % rows_per_column;
    let anchor_column = anchor_cell / rows_per_column;
    let cursor_row = cursor_cell % rows_per_column;
    let cursor_column = cursor_cell / rows_per_column;
    let row_start = anchor_row.min(cursor_row);
    let row_end = anchor_row.max(cursor_row);
    let column_start = anchor_column.min(cursor_column);
    let column_end = anchor_column.max(cursor_column);
    let mut cells = Vec::new();
    for column in column_start..=column_end {
        for row in row_start..=row_end {
            cells.push(column * rows_per_column + row);
        }
    }
    cells
}

fn block_cell_indices_from_cursor(
    editor: &Editor,
    cursor_cell: usize,
    row_count: usize,
    column_count: usize,
) -> Vec<usize> {
    let rows_per_column = editor.rows_per_column().max(1);
    let row_start = cursor_cell % rows_per_column;
    let column_start = cursor_cell / rows_per_column;
    let mut cells = Vec::with_capacity(row_count * column_count);
    for column_offset in 0..column_count {
        for row_offset in 0..row_count {
            cells.push((column_start + column_offset) * rows_per_column + row_start + row_offset);
        }
    }
    cells
}
