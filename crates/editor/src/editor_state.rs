use std::ops::Range;
use std::sync::Arc;

use gpui::App;
use rope::{CellText, TextRope};
use settings::AppSettings;

use crate::editor::{DEFAULT_VISIBLE_COLUMNS, RUBY_GUTTER_RATIO};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorViewState {
    pub(crate) selected_range: Range<usize>,
    pub(crate) selection_reversed: bool,
    pub(crate) cursor_cell: usize,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) scroll_column: usize,
    pub(crate) scroll_row: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockSelection {
    pub(crate) anchor_cell: usize,
    pub(crate) cursor_cell: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditOperation {
    pub(crate) start: usize,
    pub(crate) removed_text: String,
    pub(crate) inserted_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditTransaction {
    pub(crate) before: EditorViewState,
    pub(crate) after: EditorViewState,
    pub(crate) edits: Vec<EditOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingTransaction {
    pub(crate) before: EditorViewState,
    pub(crate) edits: Vec<EditOperation>,
}

#[derive(Default)]
pub(crate) struct EditorHistory {
    pub(crate) undo_stack: Vec<EditTransaction>,
    pub(crate) redo_stack: Vec<EditTransaction>,
    pub(crate) active_transaction: Option<PendingTransaction>,
}

#[derive(Clone)]
struct VisibleTextCache {
    draft_revision: u64,
    scroll_column: usize,
    scroll_row: usize,
    visible_columns: usize,
    visible_rows: usize,
    cells: Arc<[CellText]>,
}

pub(crate) struct EditorState {
    pub(crate) draft: TextRope,
    pub(crate) draft_revision: u64,
    pub(crate) cell_size: f32,
    pub(crate) rows_per_column: usize,
    pub(crate) selected_range: Range<usize>,
    pub(crate) selection_reversed: bool,
    pub(crate) cursor_cell: usize,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) block_selection: Option<BlockSelection>,
    pub(crate) scroll_column: usize,
    pub(crate) scroll_row: usize,
    pub(crate) scroll_remainder_columns: f32,
    pub(crate) visible_columns: usize,
    pub(crate) max_visible_rows: usize,
    pub(crate) text_input_enabled: bool,
    pub(crate) history: EditorHistory,
    visible_text_cache: Option<VisibleTextCache>,
}

impl EditorState {
    pub(crate) fn new(cx: &App) -> Self {
        let rows_per_column = AppSettings::global(cx)
            .rows_per_column
            .unwrap_or_else(AppSettings::default_rows_per_column);

        Self {
            draft: TextRope::new_with_rows(rows_per_column),
            draft_revision: 0,
            cell_size: AppSettings::global(cx).cell_size as f32,
            rows_per_column,
            selected_range: 0..0,
            selection_reversed: false,
            cursor_cell: 0,
            marked_range: None,
            block_selection: None,
            scroll_column: 0,
            scroll_row: 0,
            scroll_remainder_columns: 0.0,
            visible_columns: DEFAULT_VISIBLE_COLUMNS,
            max_visible_rows: rows_per_column,
            text_input_enabled: true,
            history: EditorHistory::default(),
            visible_text_cache: None,
        }
    }

    pub(crate) fn used_cells(&self) -> usize {
        self.draft.len_display_cells()
    }

    pub(crate) fn cell_size(&self) -> f32 {
        self.cell_size
    }

    pub(crate) fn ruby_gutter_size(&self) -> f32 {
        self.cell_size * RUBY_GUTTER_RATIO
    }

    pub(crate) fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    pub(crate) fn visible_text(&mut self) -> Arc<[CellText]> {
        let visible_rows = self.visible_rows();
        if let Some(cache) = &self.visible_text_cache
            && cache.draft_revision == self.draft_revision
            && cache.scroll_column == self.scroll_column
            && cache.scroll_row == self.scroll_row
            && cache.visible_columns == self.visible_columns
            && cache.visible_rows == visible_rows
        {
            return cache.cells.clone();
        }

        let mut cells = Vec::with_capacity(visible_rows * self.visible_columns());
        for column in self.scroll_column..self.scroll_column + self.visible_columns() {
            let start_index = column * self.rows_per_column() + self.scroll_row;
            cells.extend(self.draft.visible_cells(start_index, visible_rows));
        }
        let cells: Arc<[CellText]> = cells.into();
        self.visible_text_cache = Some(VisibleTextCache {
            draft_revision: self.draft_revision,
            scroll_column: self.scroll_column,
            scroll_row: self.scroll_row,
            visible_columns: self.visible_columns,
            visible_rows,
            cells: cells.clone(),
        });
        cells
    }

    pub(crate) fn rows_per_column(&self) -> usize {
        self.rows_per_column
    }

    pub(crate) fn visible_columns(&self) -> usize {
        self.visible_columns
    }

    pub(crate) fn update_visible_columns(&mut self, visible_columns: usize) {
        self.visible_columns = visible_columns.max(1);
        self.ensure_cursor_visible();
    }

    pub(crate) fn update_max_visible_rows(&mut self, visible_rows: usize) {
        self.max_visible_rows = visible_rows.clamp(1, self.rows_per_column());
        self.ensure_cursor_visible();
    }

    pub(crate) fn update_rows_per_column(&mut self, rows_per_column: usize) {
        let rows_per_column = rows_per_column.clamp(1, AppSettings::max_rows_per_column());
        if self.rows_per_column == rows_per_column {
            return;
        }

        let cursor_offset = self.cursor_offset();
        self.rows_per_column = rows_per_column;
        self.draft.set_rows_per_column(rows_per_column);
        self.bump_draft_revision();
        self.cursor_cell = self.display_cell_for_byte(cursor_offset);
        self.ensure_cursor_visible();
    }

    pub(crate) fn update_cell_size(&mut self, cell_size: f32) {
        let cell_size = cell_size.max(1.0);
        if (self.cell_size - cell_size).abs() < f32::EPSILON {
            return;
        }

        self.cell_size = cell_size;
    }

    pub(crate) fn update_hanging_punctuation(&mut self, enabled: bool) {
        if self.draft.hanging_punctuation() == enabled {
            return;
        }

        let cursor_offset = self.cursor_offset();
        self.draft.set_hanging_punctuation(enabled);
        self.bump_draft_revision();
        self.cursor_cell = self.display_cell_for_byte(cursor_offset);
        self.ensure_cursor_visible();
    }

    pub(crate) fn cursor_column(&self) -> usize {
        self.cursor_cell / self.rows_per_column()
    }

    pub(crate) fn cursor_row(&self) -> usize {
        self.cursor_cell % self.rows_per_column()
    }

    pub(crate) fn max_scroll_column(&self) -> usize {
        self.used_cells()
            .div_ceil(self.rows_per_column())
            .max(1)
            .saturating_sub(self.visible_columns())
    }

    pub(crate) fn visible_rows(&self) -> usize {
        self.max_visible_rows.min(self.rows_per_column()).max(1)
    }

    pub(crate) fn max_scroll_row(&self) -> usize {
        self.rows_per_column().saturating_sub(self.visible_rows())
    }

    pub(crate) fn clamp_scroll_row(&mut self) {
        self.scroll_row = self.scroll_row.min(self.max_scroll_row());
    }

    pub(crate) fn ensure_cursor_visible(&mut self) {
        let cursor_column = self.cursor_column();
        if cursor_column < self.scroll_column {
            self.scroll_column = cursor_column;
        } else if cursor_column >= self.scroll_column + self.visible_columns() {
            self.scroll_column = cursor_column + 1 - self.visible_columns();
        }

        let cursor_row = self.cursor_row();
        if cursor_row < self.scroll_row {
            self.scroll_row = cursor_row;
        } else if cursor_row >= self.scroll_row + self.visible_rows() {
            self.scroll_row = cursor_row + 1 - self.visible_rows();
        }
        self.clamp_scroll_row();
    }

    pub(crate) fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.draft.byte_to_utf16(range.start)..self.draft.byte_to_utf16(range.end)
    }

    pub(crate) fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.draft.utf16_to_byte(range_utf16.start)..self.draft.utf16_to_byte(range_utf16.end)
    }

    pub(crate) fn display_cell_for_byte(&self, byte_offset: usize) -> usize {
        self.draft.display_cell_for_byte(byte_offset)
    }

    pub(crate) fn previous_boundary(&self, offset: usize) -> usize {
        let grapheme_index = self.draft.grapheme_index_for_byte(offset);
        if grapheme_index == 0 {
            0
        } else {
            self.draft
                .byte_offset_for_grapheme_index(grapheme_index - 1)
        }
    }

    pub(crate) fn next_boundary(&self, offset: usize) -> usize {
        self.draft
            .byte_offset_for_grapheme_index(self.draft.grapheme_index_for_byte(offset) + 1)
    }

    pub(crate) fn materialize_cursor_cell_for_insert(
        &mut self,
        range: Range<usize>,
    ) -> Range<usize> {
        if !range.is_empty() {
            return range;
        }

        let offset = self.draft.materialize_display_cell(self.cursor_cell);
        self.bump_draft_revision();
        offset..offset
    }

    pub(crate) fn editing_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone())
    }

    pub(crate) fn byte_offset_for_display_cell(&self, display_cell_index: usize) -> usize {
        self.draft.byte_offset_for_display_cell(display_cell_index)
    }

    pub(crate) fn set_cursor_from_offset(&mut self, cursor_offset: usize) {
        self.selected_range = cursor_offset..cursor_offset;
        self.selection_reversed = false;
        self.cursor_cell = self.display_cell_for_byte(cursor_offset);
        self.marked_range = None;
        self.block_selection = None;
        self.ensure_cursor_visible();
    }

    pub(crate) fn selected_utf16_selection(&self) -> gpui::UTF16Selection {
        gpui::UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        }
    }

    pub(crate) fn marked_range_utf16(&self) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    pub(crate) fn view_state(&self) -> EditorViewState {
        EditorViewState {
            selected_range: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
            cursor_cell: self.cursor_cell,
            marked_range: self.marked_range.clone(),
            scroll_column: self.scroll_column,
            scroll_row: self.scroll_row,
        }
    }

    pub(crate) fn restore_view_state(&mut self, state: EditorViewState) {
        self.selected_range = state.selected_range;
        self.selection_reversed = state.selection_reversed;
        self.cursor_cell = state.cursor_cell;
        self.marked_range = state.marked_range;
        self.block_selection = None;
        self.scroll_column = state.scroll_column.min(self.max_scroll_column());
        self.scroll_row = state.scroll_row.min(self.max_scroll_row());
        self.scroll_remainder_columns = 0.0;
        self.ensure_cursor_visible();
    }

    pub(crate) fn bump_draft_revision(&mut self) {
        self.draft_revision = self.draft_revision.wrapping_add(1);
        self.visible_text_cache = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state(text: &str) -> EditorState {
        let rows_per_column = 4;
        EditorState {
            draft: TextRope::from_str_with_rows(text, rows_per_column),
            draft_revision: 0,
            cell_size: 28.0,
            rows_per_column,
            selected_range: 0..0,
            selection_reversed: false,
            cursor_cell: 0,
            marked_range: None,
            block_selection: None,
            scroll_column: 0,
            scroll_row: 0,
            scroll_remainder_columns: 0.0,
            visible_columns: 2,
            max_visible_rows: rows_per_column,
            text_input_enabled: true,
            history: EditorHistory::default(),
            visible_text_cache: None,
        }
    }

    #[test]
    fn visible_text_cache_hits_when_viewport_is_unchanged() {
        let mut state = test_state("天地玄黄");

        let first = state.visible_text();
        state.cursor_cell = 1;
        let second = state.visible_text();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn visible_text_cache_misses_after_scroll_or_text_change() {
        let mut state = test_state("天地玄黄");

        let first = state.visible_text();
        state.scroll_column = 1;
        let after_scroll = state.visible_text();
        assert!(!Arc::ptr_eq(&first, &after_scroll));

        state.scroll_column = 0;
        state.draft.replace_range(0..0, "文");
        state.bump_draft_revision();
        let after_edit = state.visible_text();

        assert_eq!(after_edit[0].text, "文");
    }
}
