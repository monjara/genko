use std::ops::Range;

use gpui::{Bounds, Context, Pixels, UTF16Selection, px};
use rich_text::RichTextKind;

use super::{BlockSelection, Editor, PageBreakMenuKind, PageBreakMenuRequest, RubyEditRequest};
use crate::editor::layout::{
    board_x_for_visible_column, cell_bounds_for_logical_index, logical_index_for_point,
};

pub(super) fn previous_boundary(editor: &Editor, offset: usize) -> usize {
    let grapheme_index = editor.draft.grapheme_index_for_byte(offset);
    if grapheme_index == 0 {
        0
    } else {
        editor
            .draft
            .byte_offset_for_grapheme_index(grapheme_index - 1)
    }
}

pub(super) fn next_boundary(editor: &Editor, offset: usize) -> usize {
    editor
        .draft
        .byte_offset_for_grapheme_index(editor.draft.grapheme_index_for_byte(offset) + 1)
}

pub(super) fn editing_range(editor: &Editor, range_utf16: Option<Range<usize>>) -> Range<usize> {
    range_utf16
        .as_ref()
        .map(|range| editor.range_from_utf16(range))
        .or_else(|| editor.marked_range.clone())
        .unwrap_or_else(|| editor.selected_range.clone())
}

pub(super) fn materialize_display_cell(editor: &mut Editor, display_cell_index: usize) -> usize {
    let offset = editor.draft.materialize_display_cell(display_cell_index);
    editor.bump_draft_revision();
    offset
}

pub(super) fn set_cursor_from_offset(editor: &mut Editor, cursor_offset: usize) {
    editor.selected_range = cursor_offset..cursor_offset;
    editor.selection_reversed = false;
    editor.cursor_cell = editor.display_cell_for_byte(cursor_offset);
    editor.marked_range = None;
    editor.block_selection = None;
    editor.ensure_cursor_visible();
}

pub(super) fn selected_utf16_selection(editor: &Editor) -> UTF16Selection {
    UTF16Selection {
        range: editor.range_to_utf16(&editor.selected_range),
        reversed: editor.selection_reversed,
    }
}

pub(super) fn marked_range_utf16(editor: &Editor) -> Option<Range<usize>> {
    editor
        .marked_range
        .as_ref()
        .map(|range| editor.range_to_utf16(range))
}

pub(super) fn offset_after_cursor(editor: &Editor) -> usize {
    next_boundary(editor, editor.cursor_offset())
}

pub(super) fn move_cursor_by(editor: &mut Editor, delta: isize, cx: &mut Context<Editor>) {
    let target = editor.cursor_cell.saturating_add_signed(delta);
    move_to_display_cell(editor, target, cx);
}

pub(super) fn move_cursor_to_display_cell(
    editor: &mut Editor,
    cell_index: usize,
    cx: &mut Context<Editor>,
) {
    move_to_display_cell(editor, cell_index, cx);
}

pub(super) fn move_cursor_to_byte_offset(
    editor: &mut Editor,
    byte_offset: usize,
    cx: &mut Context<Editor>,
) {
    let cell_index = editor.display_cell_for_byte(byte_offset);
    move_to_display_cell(editor, cell_index, cx);
}

pub(super) fn select_cursor_by(editor: &mut Editor, delta: isize, cx: &mut Context<Editor>) {
    let target = editor.cursor_cell.saturating_add_signed(delta);
    select_to_display_cell(editor, target, cx);
}

pub(super) fn select_visual_range(
    editor: &mut Editor,
    anchor_cell: usize,
    cursor_cell: usize,
    cx: &mut Context<Editor>,
) {
    let start_cell = anchor_cell.min(cursor_cell);
    let end_cell = anchor_cell.max(cursor_cell);
    let start = editor.draft.byte_offset_for_display_cell(start_cell);
    let end = next_boundary(editor, editor.draft.byte_offset_for_display_cell(end_cell)).max(start);
    editor.selected_range = start..end;
    editor.selection_reversed = cursor_cell < anchor_cell;
    editor.cursor_cell = cursor_cell;
    editor.block_selection = None;
    editor.ensure_cursor_visible();
    theme::note_cursor_activity(cx);
    cx.notify();
}

pub(super) fn set_block_selection(
    editor: &mut Editor,
    anchor_cell: usize,
    cursor_cell: usize,
    cx: &mut Context<Editor>,
) {
    let cursor_offset = editor.byte_offset_for_display_cell(cursor_cell);
    editor.selected_range = cursor_offset..cursor_offset;
    editor.selection_reversed = false;
    editor.cursor_cell = cursor_cell;
    editor.marked_range = None;
    editor.block_selection = Some(BlockSelection {
        anchor_cell,
        cursor_cell,
    });
    editor.ensure_cursor_visible();
    theme::note_cursor_activity(cx);
    cx.notify();
}

pub(super) fn clear_block_selection(editor: &mut Editor, cx: &mut Context<Editor>) {
    if editor.block_selection.take().is_some() {
        cx.notify();
    }
}

pub(super) fn collapse_selection_to_cursor_offset(editor: &mut Editor, cx: &mut Context<Editor>) {
    let cursor_offset = editor.cursor_offset();
    editor.selected_range = cursor_offset..cursor_offset;
    editor.selection_reversed = false;
    editor.marked_range = None;
    editor.block_selection = None;
    editor.ensure_cursor_visible();
    theme::note_cursor_activity(cx);
    cx.notify();
}

pub(super) fn collapse_selection_to_cursor_cell(editor: &mut Editor, cx: &mut Context<Editor>) {
    let cursor_offset = editor.byte_offset_for_display_cell(editor.cursor_cell);
    set_cursor_from_offset(editor, cursor_offset);
    theme::note_cursor_activity(cx);
    cx.notify();
}

pub(super) fn move_to_display_cell(
    editor: &mut Editor,
    cell_index: usize,
    cx: &mut Context<Editor>,
) {
    let offset = editor.byte_offset_for_display_cell(cell_index);
    if editor.cursor_cell == cell_index
        && editor.selected_range.start == offset
        && editor.selected_range.end == offset
        && !editor.selection_reversed
        && editor.block_selection.is_none()
    {
        return;
    }
    editor.selected_range = offset..offset;
    editor.selection_reversed = false;
    editor.cursor_cell = cell_index;
    editor.block_selection = None;
    editor.ensure_cursor_visible();
    theme::note_cursor_activity(cx);
    cx.notify();
}

pub(super) fn select_to_display_cell(
    editor: &mut Editor,
    cell_index: usize,
    cx: &mut Context<Editor>,
) {
    let offset = editor.byte_offset_for_display_cell(cell_index);
    let original_range = editor.selected_range.clone();
    let original_reversed = editor.selection_reversed;
    if editor.selection_reversed {
        editor.selected_range.start = offset;
    } else {
        editor.selected_range.end = offset;
    }
    if editor.selected_range.end < editor.selected_range.start {
        editor.selection_reversed = !editor.selection_reversed;
        editor.selected_range = editor.selected_range.end..editor.selected_range.start;
    }
    editor.cursor_cell = cell_index;
    editor.block_selection = None;
    editor.ensure_cursor_visible();
    if editor.cursor_cell == cell_index
        && editor.selected_range == original_range
        && editor.selection_reversed == original_reversed
        && editor.block_selection.is_none()
    {
        return;
    }
    theme::note_cursor_activity(cx);
    cx.notify();
}

pub(super) fn select_between_display_cells(
    editor: &mut Editor,
    anchor_cell: usize,
    cursor_cell: usize,
    cx: &mut Context<Editor>,
) {
    let anchor_offset = editor.byte_offset_for_display_cell(anchor_cell);
    let cursor_offset = editor.byte_offset_for_display_cell(cursor_cell);
    let original_range = editor.selected_range.clone();
    let original_reversed = editor.selection_reversed;
    let original_cursor = editor.cursor_cell;

    if cursor_offset < anchor_offset {
        editor.selected_range = cursor_offset..anchor_offset;
        editor.selection_reversed = true;
    } else {
        editor.selected_range = anchor_offset..cursor_offset;
        editor.selection_reversed = false;
    }
    editor.cursor_cell = cursor_cell;
    editor.block_selection = None;
    editor.ensure_cursor_visible();

    if editor.selected_range == original_range
        && editor.selection_reversed == original_reversed
        && editor.cursor_cell == original_cursor
    {
        return;
    }
    theme::note_cursor_activity(cx);
    cx.notify();
}

pub(super) fn selection_anchor_cell(editor: &Editor) -> usize {
    let anchor_offset = if editor.selection_reversed {
        editor.selected_range.end
    } else {
        editor.selected_range.start
    };
    editor.display_cell_for_byte(anchor_offset)
}

pub(super) fn clamped_display_cell_for_point(
    editor: &Editor,
    position: gpui::Point<Pixels>,
) -> Option<usize> {
    let bounds = editor.last_board_bounds?;
    let visible_columns = editor.visible_columns().max(1);
    let visible_rows = editor.visible_rows().max(1);
    let max_x = (bounds.right() - px(0.001)).max(bounds.left());
    let max_y = (bounds.bottom() - px(0.001)).max(bounds.top());
    let clamped_x = position.x.clamp(bounds.left(), max_x);
    let clamped_y = position.y.clamp(bounds.top(), max_y);
    let stride_value = editor.cell_size() + editor.ruby_gutter_size();
    let stride = px(stride_value);
    let local_x = clamped_x - bounds.left();
    let slot = (local_x / stride)
        .floor()
        .clamp(0.0, (visible_columns - 1) as f32) as usize;
    let slot_offset = local_x - px(slot as f32 * stride_value);
    let column = if slot_offset > px(editor.cell_size()) {
        let gutter_offset = slot_offset - px(editor.cell_size());
        let gutter_size = px(editor.ruby_gutter_size());
        if gutter_offset >= gutter_size / 2.0 && slot + 1 < visible_columns {
            slot + 1
        } else {
            slot
        }
    } else {
        slot
    };
    let row = ((clamped_y - bounds.top()) / px(editor.cell_size()))
        .floor()
        .clamp(0.0, (visible_rows - 1) as f32) as usize;
    let column_from_right = visible_columns - 1 - column;
    Some(
        (editor.scroll_column + column_from_right) * editor.rows_per_column()
            + editor.scroll_row
            + row,
    )
}

pub(super) fn byte_offset_for_point(
    editor: &Editor,
    position: gpui::Point<Pixels>,
) -> Option<usize> {
    let bounds = editor.last_board_bounds?;
    let index = logical_index_for_point(
        bounds,
        position,
        editor.scroll_column,
        editor.scroll_row,
        editor.rows_per_column(),
        editor.visible_columns(),
        editor.visible_rows(),
        editor.cell_size(),
        editor.ruby_gutter_size(),
    )?;
    Some(editor.draft.byte_offset_for_display_cell(index))
}

pub(super) fn ruby_edit_request_for_point(
    editor: &Editor,
    position: gpui::Point<Pixels>,
) -> Option<RubyEditRequest> {
    let bounds = editor.last_board_bounds?;
    if !bounds.contains(&position) {
        return None;
    }

    let visible_columns = editor.visible_columns().max(1);
    let visible_rows = editor.visible_rows().max(1);
    let cell_size = editor.cell_size();
    let ruby_gutter_size = editor.ruby_gutter_size();
    let local_x = position.x - bounds.left();
    let stride_value = cell_size + ruby_gutter_size;
    let slot = (local_x / px(stride_value))
        .floor()
        .clamp(0.0, (visible_columns - 1) as f32) as usize;
    let slot_offset = local_x - px(slot as f32 * stride_value);
    if slot_offset <= px(cell_size) || slot_offset > px(stride_value) {
        return None;
    }

    let row = ((position.y - bounds.top()) / px(cell_size))
        .floor()
        .clamp(0.0, (visible_rows.saturating_sub(1)) as f32) as usize;
    let column_from_right = visible_columns - 1 - slot;
    let logical_index = (editor.scroll_column + column_from_right) * editor.rows_per_column()
        + editor.scroll_row
        + row;
    let start = editor.draft.byte_offset_for_display_cell(logical_index);
    let range = if !editor.selected_range.is_empty()
        && editor.selected_range.start <= start
        && start < editor.selected_range.end
    {
        editor.selected_range.clone()
    } else {
        start..next_boundary(editor, start)
    };
    if range.is_empty() {
        return None;
    }

    let column_left = board_x_for_visible_column(bounds.left(), slot, cell_size, ruby_gutter_size);
    let request_bounds = Bounds::new(
        gpui::point(
            column_left + px(cell_size),
            bounds.top() + px(row as f32 * cell_size),
        ),
        gpui::size(px(ruby_gutter_size), px(cell_size)),
    );
    let text = editor
        .rich_text_meta
        .marks()
        .iter()
        .find_map(|mark| {
            if mark.range().start == range.start {
                if let rich_text::RichTextKind::Ruby { text } = mark.kind() {
                    return Some(text.clone());
                }
            }
            None
        })
        .unwrap_or_default();

    Some(RubyEditRequest {
        range,
        bounds: request_bounds,
        text,
    })
}

pub(super) fn page_break_menu_request_for_point(
    editor: &Editor,
    position: gpui::Point<Pixels>,
) -> Option<PageBreakMenuRequest> {
    let bounds = editor.last_board_bounds?;
    if !bounds.contains(&position) {
        return None;
    }
    if position.y < bounds.top() || position.y >= bounds.top() + px(editor.cell_size()) {
        return None;
    }

    let logical_index = logical_index_for_point(
        bounds,
        position,
        editor.scroll_column,
        editor.scroll_row,
        editor.rows_per_column(),
        editor.visible_columns(),
        editor.visible_rows(),
        editor.cell_size(),
        editor.ruby_gutter_size(),
    )?;
    let column = logical_index / editor.rows_per_column().max(1);
    let column_start_index = column * editor.rows_per_column().max(1) + editor.scroll_row;
    let cell_bounds = cell_bounds_for_logical_index(
        bounds,
        column_start_index,
        editor.scroll_column,
        editor.scroll_row,
        editor.rows_per_column(),
        editor.visible_columns(),
        editor.visible_rows(),
        editor.cell_size(),
        editor.ruby_gutter_size(),
    )?;

    Some(PageBreakMenuRequest {
        column,
        bounds: cell_bounds,
        kind: PageBreakMenuKind::Set,
    })
}

pub(super) fn page_break_context_menu_request_for_point(
    editor: &Editor,
    position: gpui::Point<Pixels>,
) -> Option<PageBreakMenuRequest> {
    let bounds = editor.last_board_bounds?;
    let column = page_break_drag_column_for_point(editor, position)?;
    let line_x = page_break_line_x(editor, bounds, column)?;
    Some(PageBreakMenuRequest {
        column,
        bounds: Bounds::new(
            gpui::point(line_x, position.y),
            gpui::size(px(1.0), px(editor.cell_size())),
        ),
        kind: PageBreakMenuKind::Remove,
    })
}

pub(super) fn page_break_drag_column_for_point(
    editor: &Editor,
    position: gpui::Point<Pixels>,
) -> Option<usize> {
    let bounds = editor.last_board_bounds?;
    if !bounds.contains(&position) {
        return None;
    }

    editor.rich_text_meta.marks().iter().find_map(|mark| {
        let RichTextKind::PageBreak { column } = mark.kind() else {
            return None;
        };
        let line_x = page_break_line_x(editor, bounds, *column)?;
        if (position.x - line_x).abs() <= px(5.0) {
            Some(*column)
        } else {
            None
        }
    })
}

pub(super) fn page_break_drop_column_for_point(
    editor: &Editor,
    position: gpui::Point<Pixels>,
) -> Option<usize> {
    let bounds = editor.last_board_bounds?;
    if position.y < bounds.top() || position.y > bounds.bottom() {
        return None;
    }

    let visible_columns = editor.visible_columns().max(1);
    let stride = px(editor.cell_size() + editor.ruby_gutter_size());
    let local_x = (position.x - bounds.left()).clamp(Pixels::ZERO, bounds.size.width);
    let slot = (local_x / stride)
        .floor()
        .clamp(0.0, visible_columns.saturating_sub(1) as f32) as usize;
    Some(editor.scroll_column + (visible_columns - 1 - slot))
}

fn page_break_line_x(editor: &Editor, bounds: Bounds<Pixels>, column: usize) -> Option<Pixels> {
    if column < editor.scroll_column {
        return None;
    }
    let visible_columns = editor.visible_columns().max(1);
    let column_from_right = column - editor.scroll_column;
    if column_from_right >= visible_columns {
        return None;
    }
    let slot = visible_columns - 1 - column_from_right;
    Some(board_x_for_visible_column(
        bounds.left(),
        slot,
        editor.cell_size(),
        editor.ruby_gutter_size(),
    ))
}

pub(super) fn bounds_for_byte_range(
    editor: &Editor,
    range: Range<usize>,
    board_bounds: Bounds<Pixels>,
) -> Option<Bounds<Pixels>> {
    let logical_index = if range.is_empty() && range.start == editor.selected_range.start {
        editor.cursor_cell
    } else {
        editor.display_cell_for_byte(range.start)
    };
    let cell_bounds = cell_bounds_for_logical_index(
        board_bounds,
        logical_index,
        editor.scroll_column,
        editor.scroll_row,
        editor.rows_per_column(),
        editor.visible_columns(),
        editor.visible_rows(),
        editor.cell_size(),
        editor.ruby_gutter_size(),
    )?;
    Some(super::ime_anchor_bounds_for_cell(cell_bounds, board_bounds))
}
