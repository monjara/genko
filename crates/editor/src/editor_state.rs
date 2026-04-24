use std::ops::Range;

use gpui::App;
use rope::{CellText, TextRope};
use settings::AppSettings;
use vim::{VimMode, VimState};

use crate::DEFAULT_VISIBLE_COLUMNS;

pub(crate) struct EditorState {
    pub(crate) draft: TextRope,
    pub(crate) rows_per_column: usize,
    pub(crate) selected_range: Range<usize>,
    pub(crate) selection_reversed: bool,
    pub(crate) cursor_cell: usize,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) scroll_column: usize,
    pub(crate) scroll_remainder_columns: f32,
    pub(crate) visible_columns: usize,
    pub(crate) vim: VimState,
}

impl EditorState {
    pub(crate) fn new(cx: &App) -> Self {
        let rows_per_column = AppSettings::global(cx)
            .rows_per_column
            .unwrap_or_else(AppSettings::default_rows_per_column);

        Self {
            draft: TextRope::new_with_rows(rows_per_column),
            rows_per_column,
            selected_range: 0..0,
            selection_reversed: false,
            cursor_cell: 0,
            marked_range: None,
            scroll_column: 0,
            scroll_remainder_columns: 0.0,
            visible_columns: DEFAULT_VISIBLE_COLUMNS,
            vim: VimState::new(AppSettings::global(cx).vim_mode),
        }
    }

    pub(crate) fn used_cells(&self) -> usize {
        self.draft.len_display_cells()
    }

    pub(crate) fn total_columns(&self) -> usize {
        let document_columns = self.used_cells().div_ceil(self.rows_per_column()).max(1);
        document_columns.max(self.cursor_column() + 1)
    }

    pub(crate) fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    pub(crate) fn vim_key_context(&self, cx: &App) -> &'static str {
        self.vim.key_context(AppSettings::global(cx).vim_mode)
    }

    pub(crate) fn is_vim_command_mode(&self, cx: &App) -> bool {
        self.vim.is_command_mode(AppSettings::global(cx).vim_mode)
    }

    pub(crate) fn visible_text(&self) -> Vec<CellText> {
        let first_visible_index = self.first_visible_cell_index();
        self.draft
            .visible_cells(first_visible_index, self.visible_cell_capacity())
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

    pub(crate) fn update_rows_per_column(&mut self, rows_per_column: usize) {
        let rows_per_column = rows_per_column.clamp(1, AppSettings::max_rows_per_column());
        if self.rows_per_column == rows_per_column {
            return;
        }

        let cursor_offset = self.cursor_offset();
        self.rows_per_column = rows_per_column;
        self.draft.set_rows_per_column(rows_per_column);
        self.cursor_cell = self.display_cell_for_byte(cursor_offset);
        if self.vim.mode() == VimMode::Visual {
            self.selected_range = cursor_offset..cursor_offset;
            self.selection_reversed = false;
            self.vim.set_mode(VimMode::Normal);
        }
        self.vim.set_visual_anchor_cell(None);
        self.ensure_cursor_visible();
    }

    pub(crate) fn visible_cell_capacity(&self) -> usize {
        self.rows_per_column() * self.visible_columns()
    }

    pub(crate) fn first_visible_cell_index(&self) -> usize {
        self.scroll_column * self.rows_per_column()
    }

    pub(crate) fn cursor_column(&self) -> usize {
        self.cursor_cell / self.rows_per_column()
    }

    pub(crate) fn max_scroll_column(&self) -> usize {
        self.total_columns().saturating_sub(self.visible_columns())
    }

    pub(crate) fn clamp_scroll_column(&mut self) {
        self.scroll_column = self.scroll_column.min(self.max_scroll_column());
    }

    pub(crate) fn ensure_cursor_visible(&mut self) {
        let cursor_column = self.cursor_column();
        if cursor_column < self.scroll_column {
            self.scroll_column = cursor_column;
        } else if cursor_column >= self.scroll_column + self.visible_columns() {
            self.scroll_column = cursor_column + 1 - self.visible_columns();
        }
        self.clamp_scroll_column();
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
        self.vim.set_visual_anchor_cell(None);
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
}
