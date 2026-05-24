use std::ops::Range;
use std::sync::Arc;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, MouseButton, MouseDownEvent, ParentElement,
    Pixels, Render, ScrollWheelEvent, Size, Styled, UTF16Selection, Window, actions, div, px,
};
use richtext::{ResolvedBlock, RichDocument};
use rope::{CellText, TextRope, utf16_to_byte_in_text};
use settings::AppSettings;

use crate::editor_canvas::{
    EditorCanvas, GridPathCache, cell_bounds_for_logical_index, content_height_for_window_height,
    logical_index_for_point, rows_per_column_for_window_height, visible_columns_for_window_width,
};

pub(crate) const DEFAULT_VISIBLE_COLUMNS: usize = 20;
pub(crate) const AUTOMATIC_ROWS_RESERVED_CELLS: usize = 4;
pub(crate) const DEFAULT_CELL_SIZE: f32 = 28.0;
pub(crate) const DEFAULT_RUBY_GUTTER_SIZE: f32 = 10.0;
pub(crate) const RUBY_GUTTER_RATIO: f32 = DEFAULT_RUBY_GUTTER_SIZE / DEFAULT_CELL_SIZE;
const IME_ANCHOR_WIDTH: f32 = 2.0;
const IME_ANCHOR_INSET: f32 = 3.0;
const IME_CANDIDATE_GAP: f32 = 16.0;

pub(crate) fn init(cx: &mut App) {
    const EDITOR_CONTEXT: Option<&str> = Some("vim_mode == insert || vim_mode == disabled");

    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, EDITOR_CONTEXT),
        KeyBinding::new("delete", Delete, EDITOR_CONTEXT),
        KeyBinding::new("up", Up, EDITOR_CONTEXT),
        KeyBinding::new("down", Down, EDITOR_CONTEXT),
        KeyBinding::new("left", Left, EDITOR_CONTEXT),
        KeyBinding::new("right", Right, EDITOR_CONTEXT),
        KeyBinding::new("shift-up", SelectUp, EDITOR_CONTEXT),
        KeyBinding::new("shift-down", SelectDown, EDITOR_CONTEXT),
        KeyBinding::new("shift-left", SelectLeft, EDITOR_CONTEXT),
        KeyBinding::new("shift-right", SelectRight, EDITOR_CONTEXT),
        KeyBinding::new("cmd-a", SelectAll, EDITOR_CONTEXT),
        KeyBinding::new("ctrl-a", SelectAll, EDITOR_CONTEXT),
        KeyBinding::new("cmd-v", Paste, EDITOR_CONTEXT),
        KeyBinding::new("ctrl-v", Paste, EDITOR_CONTEXT),
        KeyBinding::new("cmd-c", Copy, EDITOR_CONTEXT),
        KeyBinding::new("ctrl-c", Copy, EDITOR_CONTEXT),
        KeyBinding::new("cmd-x", Cut, EDITOR_CONTEXT),
        KeyBinding::new("ctrl-x", Cut, EDITOR_CONTEXT),
        KeyBinding::new("cmd-z", Undo, EDITOR_CONTEXT),
        KeyBinding::new("ctrl-z", Undo, EDITOR_CONTEXT),
        KeyBinding::new("cmd-shift-z", Redo, EDITOR_CONTEXT),
        KeyBinding::new("ctrl-u", Redo, EDITOR_CONTEXT),
        KeyBinding::new("enter", Enter, EDITOR_CONTEXT),
        KeyBinding::new("escape", ClearSelection, EDITOR_CONTEXT),
        KeyBinding::new("home", Home, EDITOR_CONTEXT),
        KeyBinding::new("end", End, EDITOR_CONTEXT),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, EDITOR_CONTEXT),
    ]);
}

actions!(
    soukou,
    [
        Backspace,
        Delete,
        Up,
        Down,
        Left,
        Right,
        SelectUp,
        SelectDown,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
        Enter,
        ClearSelection,
        ShowCharacterPalette,
    ]
);

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RichTextDecorations {
    pub inline_marks: Vec<richtext::InlineMark>,
    pub blocks: Vec<ResolvedBlock>,
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

pub(crate) struct Editor {
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
    pub(crate) richtext_decorations: RichTextDecorations,
    visible_text_cache: Option<VisibleTextCache>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) last_board_bounds: Option<Bounds<Pixels>>,
    pub(crate) grid_path_cache: Option<GridPathCache>,
    last_viewport_size: Option<Size<Pixels>>,
}
impl Editor {
    pub fn new(cx: &mut Context<Self>) -> Self {
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
            richtext_decorations: RichTextDecorations::default(),
            visible_text_cache: None,
            focus_handle: cx.focus_handle(),
            grid_path_cache: None,
            last_board_bounds: None,
            last_viewport_size: None,
        }
    }

    pub fn used_cells(&self) -> usize {
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

    pub fn byte_offset_for_display_cell(&self, display_cell_index: usize) -> usize {
        self.draft.byte_offset_for_display_cell(display_cell_index)
    }

    pub(crate) fn materialize_display_cell(&mut self, display_cell_index: usize) -> usize {
        let offset = self.draft.materialize_display_cell(display_cell_index);
        self.bump_draft_revision();
        offset
    }

    pub(crate) fn set_cursor_from_offset(&mut self, cursor_offset: usize) {
        self.selected_range = cursor_offset..cursor_offset;
        self.selection_reversed = false;
        self.cursor_cell = self.display_cell_for_byte(cursor_offset);
        self.marked_range = None;
        self.block_selection = None;
        self.ensure_cursor_visible();
    }

    pub(crate) fn selected_utf16_selection(&self) -> UTF16Selection {
        UTF16Selection {
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

    pub(crate) fn update_viewport_size(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
        let needs_viewport_sync = self.last_viewport_size != Some(size);
        if needs_viewport_sync {
            self.update_hanging_punctuation(AppSettings::global(cx).hanging_punctuation);

            self.update_cell_size(AppSettings::global(cx).cell_size as f32);

            self.update_visible_columns(visible_columns_for_window_width(
                size.width,
                self.cell_size(),
                self.ruby_gutter_size(),
            ));

            let content_height = content_height_for_window_height(
                size.height,
                AppSettings::global(cx).column_number_mode,
                self.cell_size(),
            );
            self.update_max_visible_rows(rows_per_column_for_window_height(
                content_height,
                self.cell_size(),
            ));
        }
        self.last_viewport_size = Some(size);
    }

    pub fn set_text_input_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.text_input_enabled == enabled {
            return;
        }

        self.text_input_enabled = enabled;
        if enabled {
            let cursor_offset = self.cursor_offset();
            self.selected_range = cursor_offset..cursor_offset;
            self.selection_reversed = false;
        }
        cx.notify();
    }

    pub fn cursor_cell(&self) -> usize {
        self.cursor_cell
    }

    pub fn cursor_byte_offset(&self) -> usize {
        self.cursor_offset()
    }

    pub fn rows_per_column(&self) -> usize {
        self.rows_per_column
    }

    pub fn snapshot_text(&self) -> String {
        self.draft.slice(0..self.draft.len_bytes())
    }

    pub fn draft_revision(&self) -> u64 {
        self.draft_revision
    }

    pub fn rope(&self) -> &TextRope {
        &self.draft
    }

    pub fn load_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let rows_per_column = self.rows_per_column;
        let hanging_punctuation = self.draft.hanging_punctuation();
        let mut draft = TextRope::from_str_with_rows(text, rows_per_column);
        draft.set_hanging_punctuation(hanging_punctuation);

        self.draft = draft;
        self.bump_draft_revision();
        self.history = EditorHistory::default();
        self.scroll_column = 0;
        self.scroll_row = 0;
        self.scroll_remainder_columns = 0.0;
        self.set_cursor_from_offset(0);
        cx.notify();
    }

    pub fn text_in_range(&self, range: Range<usize>) -> String {
        self.draft.slice(range)
    }

    pub fn selected_byte_range(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    pub fn selection_bounds(&self) -> Option<Bounds<Pixels>> {
        let board_bounds = self.last_board_bounds?;
        self.bounds_for_byte_range(self.selected_range.clone(), board_bounds)
    }

    pub fn set_richtext_document(
        &mut self,
        document: Option<&RichDocument>,
        cx: &mut Context<Self>,
    ) {
        self.richtext_decorations = document
            .map(|document| RichTextDecorations {
                inline_marks: document.spans.clone(),
                blocks: document.resolved_blocks(),
            })
            .unwrap_or_default();
        cx.notify();
    }

    pub fn offset_after_cursor(&self) -> usize {
        self.next_boundary(self.cursor_offset())
    }

    pub fn move_cursor_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        let target = self.cursor_cell.saturating_add_signed(delta);
        self.move_to_display_cell(target, cx);
    }

    pub fn move_cursor_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        self.move_to_display_cell(cell_index, cx);
    }

    pub fn move_cursor_to_byte_offset(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        let cell_index = self.display_cell_for_byte(byte_offset);
        self.move_to_display_cell(cell_index, cx);
    }

    pub fn select_cursor_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        let target = self.cursor_cell.saturating_add_signed(delta);
        self.select_to_display_cell(target, cx);
    }

    pub fn select_visual_range(
        &mut self,
        anchor_cell: usize,
        cursor_cell: usize,
        cx: &mut Context<Self>,
    ) {
        let start_cell = anchor_cell.min(cursor_cell);
        let end_cell = anchor_cell.max(cursor_cell);
        let start = self.draft.byte_offset_for_display_cell(start_cell);
        let end = self
            .next_boundary(self.draft.byte_offset_for_display_cell(end_cell))
            .max(start);
        self.selected_range = start..end;
        self.selection_reversed = cursor_cell < anchor_cell;
        self.cursor_cell = cursor_cell;
        self.block_selection = None;
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn set_block_selection(
        &mut self,
        anchor_cell: usize,
        cursor_cell: usize,
        cx: &mut Context<Self>,
    ) {
        let cursor_offset = self.byte_offset_for_display_cell(cursor_cell);
        self.selected_range = cursor_offset..cursor_offset;
        self.selection_reversed = false;
        self.cursor_cell = cursor_cell;
        self.marked_range = None;
        self.block_selection = Some(BlockSelection {
            anchor_cell,
            cursor_cell,
        });
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn clear_block_selection(&mut self, cx: &mut Context<Self>) {
        if self.block_selection.take().is_some() {
            cx.notify();
        }
    }

    pub fn collapse_selection_to_cursor_offset(&mut self, cx: &mut Context<Self>) {
        let cursor_offset = self.cursor_offset();
        self.selected_range = cursor_offset..cursor_offset;
        self.selection_reversed = false;
        self.marked_range = None;
        self.block_selection = None;
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn collapse_selection_to_cursor_cell(&mut self, cx: &mut Context<Self>) {
        let cursor_offset = self.byte_offset_for_display_cell(self.cursor_cell);
        self.set_cursor_from_offset(cursor_offset);
        cx.notify();
    }

    pub fn replace_byte_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_byte_range(range, new_text, cx);
    }

    pub fn begin_transaction(&mut self) {
        if self.history.active_transaction.is_none() {
            self.history.active_transaction = Some(PendingTransaction {
                before: self.view_state(),
                edits: Vec::new(),
            });
        }
    }

    pub fn commit_transaction(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pending) = self.history.active_transaction.take() else {
            return false;
        };
        let after = self.view_state();
        if pending.edits.is_empty() && pending.before == after {
            return false;
        }

        self.history.undo_stack.push(EditTransaction {
            before: pending.before,
            after,
            edits: pending.edits,
        });
        self.history.redo_stack.clear();
        cx.notify();
        true
    }

    pub fn active_transaction_inserted_text(&self) -> Option<String> {
        self.history
            .active_transaction
            .as_ref()
            .map(|transaction| transaction_inserted_text(&transaction.edits))
    }

    pub fn last_transaction_inserted_text(&self) -> Option<String> {
        self.history
            .undo_stack
            .last()
            .map(|transaction| transaction_inserted_text(&transaction.edits))
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(transaction) = self.history.undo_stack.pop() else {
            return false;
        };
        for edit in transaction.edits.iter().rev() {
            let inserted_end = edit.start + edit.inserted_text.len();
            self.draft.replace_range(
                inserted_end.saturating_sub(edit.inserted_text.len())..inserted_end,
                &edit.removed_text,
            );
        }
        self.bump_draft_revision();
        self.restore_view_state(transaction.before.clone());
        self.history.redo_stack.push(transaction);
        cx.notify();
        true
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(transaction) = self.history.redo_stack.pop() else {
            return false;
        };
        for edit in &transaction.edits {
            let removed_end = edit.start + edit.removed_text.len();
            self.draft.replace_range(
                removed_end.saturating_sub(edit.removed_text.len())..removed_end,
                &edit.inserted_text,
            );
        }
        self.bump_draft_revision();
        self.restore_view_state(transaction.after.clone());
        self.history.undo_stack.push(transaction);
        cx.notify();
        true
    }

    fn move_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        let offset = self.byte_offset_for_display_cell(cell_index);
        if self.cursor_cell == cell_index
            && self.selected_range.start == offset
            && self.selected_range.end == offset
            && !self.selection_reversed
            && self.block_selection.is_none()
        {
            return;
        }
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.cursor_cell = cell_index;
        self.block_selection = None;
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn select_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        let offset = self.byte_offset_for_display_cell(cell_index);
        let original_range = self.selected_range.clone();
        let original_reversed = self.selection_reversed;
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.cursor_cell = cell_index;
        self.block_selection = None;
        self.ensure_cursor_visible();
        if self.cursor_cell == cell_index
            && self.selected_range == original_range
            && self.selection_reversed == original_reversed
            && self.block_selection.is_none()
        {
            return;
        }
        cx.notify();
    }

    fn replace_text_in_byte_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        cx: &mut Context<Self>,
    ) {
        let implicit_transaction = self.history.active_transaction.is_none();
        if implicit_transaction {
            self.begin_transaction();
        }
        let range = if new_text.is_empty() {
            range
        } else {
            self.materialize_cursor_cell_for_insert(range)
        };
        let removed_text = self.draft.slice(range.clone());
        if let Some(transaction) = self.history.active_transaction.as_mut() {
            transaction.edits.push(EditOperation {
                start: range.start,
                removed_text,
                inserted_text: new_text.to_string(),
            });
        }
        self.draft.replace_range(range.clone(), new_text);
        self.bump_draft_revision();
        let cursor = range.start + new_text.len();
        self.set_cursor_from_offset(cursor);
        if implicit_transaction {
            let _ = self.commit_transaction(cx);
        }
        cx.notify();
    }

    fn replace_text_in_byte_range_owned(
        &mut self,
        range: Range<usize>,
        new_text: String,
        cx: &mut Context<Self>,
    ) {
        let implicit_transaction = self.history.active_transaction.is_none();
        if implicit_transaction {
            self.begin_transaction();
        }
        let range = if new_text.is_empty() {
            range
        } else {
            self.materialize_cursor_cell_for_insert(range)
        };
        let removed_text = self.draft.slice(range.clone());
        if let Some(transaction) = self.history.active_transaction.as_mut() {
            transaction.edits.push(EditOperation {
                start: range.start,
                removed_text,
                inserted_text: new_text.clone(),
            });
        }
        let cursor = range.start + new_text.len();
        self.draft.replace_range_owned(range, new_text);
        self.bump_draft_revision();
        self.set_cursor_from_offset(cursor);
        if implicit_transaction {
            let _ = self.commit_transaction(cx);
        }
        cx.notify();
    }

    fn scroll_columns_by(&mut self, delta_columns: isize, cx: &mut Context<Self>) {
        if delta_columns == 0 {
            return;
        }

        let previous_column = self.scroll_column;
        self.scroll_column = self
            .scroll_column
            .saturating_add_signed(delta_columns)
            .min(self.max_scroll_column());

        if self.scroll_column != previous_column {
            cx.notify();
        }
    }

    fn byte_offset_for_point(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        let bounds = self.last_board_bounds?;
        let index = logical_index_for_point(
            bounds,
            position,
            self.scroll_column,
            self.scroll_row,
            self.rows_per_column(),
            self.visible_columns(),
            self.visible_rows(),
            self.cell_size(),
            self.ruby_gutter_size(),
        )?;
        Some(self.draft.byte_offset_for_display_cell(index))
    }

    fn bounds_for_byte_range(
        &self,
        range: Range<usize>,
        board_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let logical_index = if range.is_empty() && range.start == self.selected_range.start {
            self.cursor_cell
        } else {
            self.display_cell_for_byte(range.start)
        };
        let cell_bounds = cell_bounds_for_logical_index(
            board_bounds,
            logical_index,
            self.scroll_column,
            self.scroll_row,
            self.rows_per_column(),
            self.visible_columns(),
            self.visible_rows(),
            self.cell_size(),
            self.ruby_gutter_size(),
        )?;
        Some(ime_anchor_bounds_for_cell(cell_bounds, board_bounds))
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            self.selected_range = previous..self.cursor_offset();
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
        invalidate_ime_position(window);
    }

    fn delete_forward(&mut self, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            self.selected_range = self.cursor_offset()..next;
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_forward(cx);
        invalidate_ime_position(window);
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor_by(-1, cx);
        invalidate_ime_position(window);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor_by(1, cx);
        invalidate_ime_position(window);
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor_by(self.rows_per_column() as isize, cx);
        invalidate_ime_position(window);
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor_by(-(self.rows_per_column() as isize), cx);
        invalidate_ime_position(window);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.select_cursor_by(-1, cx);
        invalidate_ime_position(window);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.select_cursor_by(1, cx);
        invalidate_ime_position(window);
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select_cursor_by(self.rows_per_column() as isize, cx);
        invalidate_ime_position(window);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select_cursor_by(-(self.rows_per_column() as isize), cx);
        invalidate_ime_position(window);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.draft.len_bytes();
        self.selection_reversed = false;
        self.cursor_cell = self.used_cells();
        self.block_selection = None;
        invalidate_ime_position(window);
        cx.notify();
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_display_cell(0, cx);
        invalidate_ime_position(window);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_display_cell(self.used_cells(), cx);
        invalidate_ime_position(window);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_byte_range_owned(self.selected_range.clone(), text, cx);
            invalidate_ime_position(window);
        }
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        let inserted_text = if AppSettings::global(cx).indent_on_enter {
            "\n "
        } else {
            "\n"
        };
        self.replace_text_in_byte_range(self.selected_range.clone(), inserted_text, cx);
        invalidate_ime_position(window);
    }

    fn clear_selection_action(
        &mut self,
        _: &ClearSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.block_selection.is_some() {
            self.clear_block_selection(cx);
            invalidate_ime_position(window);
            return;
        }

        if !self.selected_range.is_empty() {
            self.collapse_selection_to_cursor_offset(cx);
            invalidate_ime_position(window);
        }
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.draft.slice(self.selected_range.clone()),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.draft.slice(self.selected_range.clone()),
            ));
            self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
            invalidate_ime_position(window);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn undo_action(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.undo(cx) {
            invalidate_ime_position(window);
        }
    }

    fn redo_action(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if self.redo(cx) {
            invalidate_ime_position(window);
        }
    }

    fn on_board_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let Some(bounds) = self.last_board_bounds else {
            return;
        };
        if let Some(cell_index) = logical_index_for_point(
            bounds,
            event.position,
            self.scroll_column,
            self.scroll_row,
            self.rows_per_column(),
            self.visible_columns(),
            self.visible_rows(),
            self.cell_size(),
            self.ruby_gutter_size(),
        ) {
            if event.modifiers.shift {
                self.select_to_display_cell(cell_index, cx);
            } else {
                self.move_to_display_cell(cell_index, cx);
            }
            invalidate_ime_position(window);
        }
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(self.cell_size()));
        let column_delta = if delta.x == Pixels::ZERO {
            -(delta.y / px(self.cell_size()))
        } else {
            -(delta.x / px(self.cell_size()))
        };

        self.scroll_remainder_columns += column_delta;
        let whole_columns = self.scroll_remainder_columns.trunc() as isize;
        if whole_columns != 0 {
            self.scroll_remainder_columns -= whole_columns as f32;
            self.scroll_columns_by(whole_columns, cx);
        }
    }
}

fn transaction_inserted_text(edits: &[EditOperation]) -> String {
    let mut inserted_edits = edits.iter().filter(|edit| !edit.inserted_text.is_empty());
    let Some(first_edit) = inserted_edits.next() else {
        return String::new();
    };

    let mut region_start = first_edit.start;
    let mut region = first_edit.removed_text.clone();
    replace_region_text(
        &mut region,
        0..first_edit.removed_text.len(),
        &first_edit.inserted_text,
    );

    for edit in inserted_edits {
        let edit_start = edit.start;
        let edit_end = edit.start + edit.removed_text.len();
        if edit_start < region_start {
            let prefix_len = region_start - edit_start;
            let prefix = &edit.removed_text[..prefix_len.min(edit.removed_text.len())];
            region.insert_str(0, prefix);
            region_start = edit_start;
        }

        let current_region_end = region_start + region.len();
        if edit_start > current_region_end {
            break;
        }

        if edit_end > current_region_end {
            let suffix_start = edit
                .removed_text
                .len()
                .saturating_sub(edit_end - current_region_end);
            region.push_str(&edit.removed_text[suffix_start..]);
        }

        let local_start = edit_start.saturating_sub(region_start);
        let local_end = local_start + edit.removed_text.len();
        replace_region_text(&mut region, local_start..local_end, &edit.inserted_text);
    }

    region
}

fn replace_region_text(region: &mut String, range: Range<usize>, replacement: &str) {
    region.replace_range(range, replacement);
}

fn ime_anchor_bounds_for_cell(
    cell_bounds: Bounds<Pixels>,
    _board_bounds: Bounds<Pixels>,
) -> Bounds<Pixels> {
    let horizontal_gap = px(IME_ANCHOR_INSET + IME_CANDIDATE_GAP);
    let left = cell_bounds.right() + horizontal_gap;

    Bounds::new(
        gpui::point(left, cell_bounds.top() + px(IME_ANCHOR_INSET)),
        gpui::size(
            px(IME_ANCHOR_WIDTH),
            cell_bounds.size.height - px(IME_ANCHOR_INSET * 2.0),
        ),
    )
}

fn invalidate_ime_position(window: &mut Window) {
    window.invalidate_character_coordinates();
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.draft.slice(range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.selected_utf16_selection())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range_utf16()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        invalidate_ime_position(window);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.text_input_enabled {
            return;
        }

        let range = self.editing_range(range_utf16);
        self.replace_text_in_byte_range(range, text, cx);
        invalidate_ime_position(window);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.text_input_enabled {
            return;
        }

        let implicit_transaction = self.history.active_transaction.is_none();
        if implicit_transaction {
            self.begin_transaction();
        }
        let range = self.editing_range(range_utf16);
        let range = if new_text.is_empty() {
            range
        } else {
            self.materialize_cursor_cell_for_insert(range)
        };
        let removed_text = self.draft.slice(range.clone());
        if let Some(transaction) = self.history.active_transaction.as_mut() {
            transaction.edits.push(EditOperation {
                start: range.start,
                removed_text,
                inserted_text: new_text.to_string(),
            });
        }
        self.draft.replace_range(range.clone(), new_text);
        self.bump_draft_revision();

        let marked_end = range.start + new_text.len();
        self.marked_range = (!new_text.is_empty()).then_some(range.start..marked_end);
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| {
                let start = utf16_to_byte_in_text(new_text, range_utf16.start);
                let end = utf16_to_byte_in_text(new_text, range_utf16.end);
                range.start + start..range.start + end
            })
            .unwrap_or(marked_end..marked_end);
        self.selection_reversed = false;
        self.cursor_cell = self.display_cell_for_byte(self.cursor_offset());
        self.ensure_cursor_visible();
        if implicit_transaction {
            let _ = self.commit_transaction(cx);
        }
        invalidate_ime_position(window);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        board_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        self.bounds_for_byte_range(range, board_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let byte_offset = self.byte_offset_for_point(point)?;
        Some(self.draft.byte_to_utf16(byte_offset))
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle(cx))
            .key_context("Soukou")
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::undo_action))
            .on_action(cx.listener(Self::redo_action))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::clear_selection_action))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_board_mouse_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .cursor(CursorStyle::IBeam)
            .child(EditorCanvas::new(cx.entity()))
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
