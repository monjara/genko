pub(crate) mod command_types;
pub(crate) mod commands;
mod default_input;
mod history;
pub(crate) mod layout;
pub(crate) mod motions;
mod selection;

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    App, Bounds, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable, IntoElement,
    Pixels, Render, Size, UTF16Selection, Window, actions, px,
};
use rich_text::{RichTextDocumentMeta, RichTextEdit, RichTextKind};
use rope::{CellText, TextRope, utf16_to_byte_in_text};
use settings::AppSettings;

use self::{history as editor_history, selection as editor_selection};
use crate::editor::layout::{
    cell_size_for_window_height, content_height_for_window_height,
    rows_per_column_for_window_height, visible_columns_for_window_width,
};
use crate::editor_canvas::GridPathCache;
use crate::search::{SearchState, find_matches, nearest_match_index};

pub(crate) const DEFAULT_VISIBLE_COLUMNS: usize = 20;
pub(crate) const DEFAULT_CELL_SIZE: f32 = 32.0;
pub(crate) const DEFAULT_RUBY_GUTTER_SIZE: f32 = 14.0;
pub(crate) const RUBY_GUTTER_RATIO: f32 = DEFAULT_RUBY_GUTTER_SIZE / DEFAULT_CELL_SIZE;
const IME_ANCHOR_WIDTH: f32 = 2.0;
const IME_ANCHOR_INSET: f32 = 3.0;
const IME_CANDIDATE_GAP: f32 = 16.0;

pub(crate) fn init(_cx: &mut App) {}

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

#[derive(Clone, Debug)]
pub struct RubyEditRequest {
    pub range: Range<usize>,
    pub bounds: Bounds<Pixels>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PageBreakMenuRequest {
    pub column: usize,
    pub bounds: Bounds<Pixels>,
    pub kind: PageBreakMenuKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageBreakMenuKind {
    Set,
    Remove,
}

#[derive(Clone, Debug)]
pub enum Event {
    RubyEditRequested(RubyEditRequest),
    PageBreakMenuRequested(PageBreakMenuRequest),
    PageBreakMoved {
        from_column: usize,
        to_column: usize,
    },
}

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
    pub(crate) affects_rich_text: bool,
}

impl EditOperation {
    pub(crate) fn start(&self) -> usize {
        self.start
    }

    pub(crate) fn removed_text(&self) -> &str {
        self.removed_text.as_str()
    }

    pub(crate) fn inserted_text(&self) -> &str {
        self.inserted_text.as_str()
    }

    pub(crate) fn affects_rich_text(&self) -> bool {
        self.affects_rich_text
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppliedEditBatch {
    revision: u64,
    edits: Vec<EditOperation>,
}

impl AppliedEditBatch {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn edits(&self) -> &[EditOperation] {
        &self.edits
    }
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

#[derive(Clone, Copy, Debug)]
pub struct PlainTextLoadSettings {
    rows_per_column: usize,
    hanging_punctuation: bool,
}

pub struct PreparedPlainText {
    text: String,
    draft: TextRope,
}

impl PreparedPlainText {
    pub fn new(text: String, settings: PlainTextLoadSettings) -> Self {
        let mut draft = TextRope::from_str_with_rows(&text, settings.rows_per_column);
        draft.set_hanging_punctuation(settings.hanging_punctuation);
        Self { text, draft }
    }
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
    visible_text_cache: Option<VisibleTextCache>,
    last_applied_edit_batch: Option<AppliedEditBatch>,
    rich_text_meta: RichTextDocumentMeta,
    rich_text_synced_revision: u64,
    rich_text_synced_text: String,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) last_board_bounds: Option<Bounds<Pixels>>,
    pub(crate) grid_path_cache: Option<GridPathCache>,
    last_viewport_size: Option<Size<Pixels>>,
    mouse_selection_anchor_cell: Option<usize>,
    page_break_drag_column: Option<usize>,
    is_mouse_selecting: bool,
    pub(crate) search_state: Option<SearchState>,
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
            visible_text_cache: None,
            last_applied_edit_batch: None,
            rich_text_meta: RichTextDocumentMeta::default(),
            rich_text_synced_revision: 0,
            rich_text_synced_text: String::new(),
            focus_handle: cx.focus_handle(),
            grid_path_cache: None,
            last_board_bounds: None,
            last_viewport_size: None,
            mouse_selection_anchor_cell: None,
            page_break_drag_column: None,
            is_mouse_selecting: false,
            search_state: None,
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

    pub(crate) fn rich_text_meta(&self) -> &RichTextDocumentMeta {
        &self.rich_text_meta
    }

    pub fn plain_text_load_settings(&self) -> PlainTextLoadSettings {
        PlainTextLoadSettings {
            rows_per_column: self.rows_per_column,
            hanging_punctuation: self.draft.hanging_punctuation(),
        }
    }

    pub fn rich_text_meta_snapshot(&self) -> RichTextDocumentMeta {
        self.rich_text_meta.clone()
    }

    pub fn set_rich_text_meta(
        &mut self,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        self.rich_text_meta = rich_text_meta;
        self.rich_text_synced_revision = self.draft_revision;
        self.rich_text_synced_text = self.snapshot_text();
        cx.notify();
    }

    pub fn apply_rich_text_kind(&mut self, kind: RichTextKind, cx: &mut Context<Self>) {
        self.sync_rich_text_meta_after_text_change();
        let range = self.selected_byte_range();
        self.rich_text_meta.toggle_mark(range, kind);
        self.rich_text_synced_revision = self.draft_revision;
        cx.notify();
    }

    pub fn set_ruby(&mut self, range: Range<usize>, text: String, cx: &mut Context<Self>) {
        self.sync_rich_text_meta_after_text_change();
        self.rich_text_meta.set_ruby(range, text);
        self.rich_text_synced_revision = self.draft_revision;
        cx.notify();
    }

    pub fn set_page_break_column(&mut self, column: usize, offset: usize, cx: &mut Context<Self>) {
        self.sync_rich_text_meta_after_text_change();
        self.rich_text_meta.set_page_break_column(column, offset);
        self.rich_text_synced_revision = self.draft_revision;
        cx.notify();
    }

    pub fn move_page_break_column(
        &mut self,
        from_column: usize,
        to_column: usize,
        offset: usize,
        cx: &mut Context<Self>,
    ) {
        self.sync_rich_text_meta_after_text_change();
        self.rich_text_meta
            .move_page_break_column(from_column, to_column, offset);
        self.rich_text_synced_revision = self.draft_revision;
        cx.notify();
    }

    pub fn remove_page_break_column(&mut self, column: usize, cx: &mut Context<Self>) {
        self.sync_rich_text_meta_after_text_change();
        self.rich_text_meta.remove_page_break_column(column);
        self.rich_text_synced_revision = self.draft_revision;
        cx.notify();
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
        editor_selection::previous_boundary(self, offset)
    }

    pub(crate) fn next_boundary(&self, offset: usize) -> usize {
        editor_selection::next_boundary(self, offset)
    }

    fn materialize_cursor_cell_for_insert(&mut self, range: Range<usize>) -> Range<usize> {
        if !range.is_empty() {
            return range;
        }

        let previous_len = self.draft.len_bytes();
        let offset = self.draft.materialize_display_cell(self.cursor_cell);
        let current_len = self.draft.len_bytes();
        self.bump_draft_revision();

        if current_len > previous_len {
            let inserted_len = current_len - previous_len;
            let inserted_start = offset - inserted_len;
            let inserted_text = self.draft.slice(inserted_start..offset);
            if let Some(transaction) = self.history.active_transaction.as_mut() {
                transaction.edits.push(EditOperation {
                    start: inserted_start,
                    removed_text: String::new(),
                    inserted_text,
                    affects_rich_text: true,
                });
            }
        }

        offset..offset
    }

    pub(crate) fn editing_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        editor_selection::editing_range(self, range_utf16)
    }

    pub fn byte_offset_for_display_cell(&self, display_cell_index: usize) -> usize {
        self.draft.byte_offset_for_display_cell(display_cell_index)
    }

    pub(crate) fn materialize_display_cell(&mut self, display_cell_index: usize) -> usize {
        editor_selection::materialize_display_cell(self, display_cell_index)
    }

    pub(crate) fn ruby_edit_request_for_point(
        &self,
        position: gpui::Point<Pixels>,
    ) -> Option<RubyEditRequest> {
        editor_selection::ruby_edit_request_for_point(self, position)
    }

    pub(crate) fn page_break_menu_request_for_point(
        &self,
        position: gpui::Point<Pixels>,
    ) -> Option<PageBreakMenuRequest> {
        editor_selection::page_break_menu_request_for_point(self, position)
    }

    pub(crate) fn page_break_drag_column_for_point(
        &self,
        position: gpui::Point<Pixels>,
    ) -> Option<usize> {
        editor_selection::page_break_drag_column_for_point(self, position)
    }

    pub(crate) fn page_break_context_menu_request_for_point(
        &self,
        position: gpui::Point<Pixels>,
    ) -> Option<PageBreakMenuRequest> {
        editor_selection::page_break_context_menu_request_for_point(self, position)
    }

    pub(crate) fn page_break_drop_column_for_point(
        &self,
        position: gpui::Point<Pixels>,
    ) -> Option<usize> {
        editor_selection::page_break_drop_column_for_point(self, position)
    }

    pub(crate) fn set_cursor_from_offset(&mut self, cursor_offset: usize) {
        editor_selection::set_cursor_from_offset(self, cursor_offset);
    }

    pub(crate) fn selected_utf16_selection(&self) -> UTF16Selection {
        editor_selection::selected_utf16_selection(self)
    }

    pub(crate) fn marked_range_utf16(&self) -> Option<Range<usize>> {
        editor_selection::marked_range_utf16(self)
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

    pub(crate) fn sync_rich_text_meta_after_text_change(&mut self) {
        if self.draft_revision <= self.rich_text_synced_revision {
            return;
        }

        let current_text = self.snapshot_text();
        if current_text == self.rich_text_synced_text {
            self.rich_text_synced_revision = self.draft_revision;
            return;
        }

        let rich_text_edits = self
            .last_applied_edit_batch
            .as_ref()
            .filter(|edit_batch| edit_batch.revision() == self.draft_revision)
            .map(|edit_batch| {
                edit_batch
                    .edits()
                    .iter()
                    .map(|edit| {
                        RichTextEdit::new(
                            edit.start(),
                            edit.removed_text(),
                            edit.inserted_text(),
                            edit.affects_rich_text(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        rich_text::sync_meta_after_text_change(
            &mut self.rich_text_meta,
            self.rich_text_synced_text.as_str(),
            current_text.as_str(),
            rich_text_edits.as_slice(),
        );
        self.rich_text_synced_revision = self.draft_revision;
        self.rich_text_synced_text = current_text;
    }

    pub(crate) fn update_viewport_size(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
        let needs_viewport_sync = self.last_viewport_size != Some(size);
        if needs_viewport_sync {
            self.update_hanging_punctuation(AppSettings::global(cx).hanging_punctuation);

            let fitted_cell_size = cell_size_for_window_height(
                size.height,
                AppSettings::global(cx).column_number_mode,
            );
            self.update_cell_size(fitted_cell_size);

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

    pub fn rope(&self) -> &TextRope {
        &self.draft
    }

    // --- 検索 ---

    pub fn open_search(&mut self, cx: &mut Context<Self>) {
        if self.search_state.is_none() {
            self.search_state = Some(SearchState::new(self.draft_revision));
            cx.notify();
        }
    }

    pub fn close_search(&mut self, cx: &mut Context<Self>) {
        if self.search_state.take().is_some() {
            cx.notify();
        }
    }

    pub fn update_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.search_state.is_none() {
            return;
        }
        let matches = if query.is_empty() {
            Vec::new()
        } else {
            let text = self.snapshot_text();
            find_matches(&text, query)
        };
        let current_match = nearest_match_index(&matches, self.cursor_offset());
        let navigate_range = current_match.and_then(|i| matches.get(i)).cloned();
        let revision = self.draft_revision;

        if let Some(state) = &mut self.search_state {
            state.query = query.to_string();
            state.matches = matches;
            state.current_match = current_match;
            state.synced_at_revision = revision;
        }

        if let Some(range) = navigate_range {
            self.select_match_range(range, cx);
        } else {
            cx.notify();
        }
    }

    // テキスト変更後にマッチ一覧を再計算する（ナビゲーションなし）。
    // VimController がドキュメント変更を検知したときに呼ぶ。
    pub fn resync_search_matches(&mut self, cx: &mut Context<Self>) {
        let (query, synced_rev) = match &self.search_state {
            None => return,
            Some(state) if state.query.is_empty() => return,
            Some(state) if state.synced_at_revision == self.draft_revision => return,
            Some(state) => (state.query.clone(), state.synced_at_revision),
        };
        let _ = synced_rev;

        let text = self.snapshot_text();
        let new_matches = find_matches(&text, &query);
        let revision = self.draft_revision;

        let Some(state) = &mut self.search_state else {
            return;
        };
        if let Some(idx) = state.current_match {
            if idx >= new_matches.len() {
                state.current_match = if new_matches.is_empty() {
                    None
                } else {
                    Some(new_matches.len() - 1)
                };
            }
        }
        state.matches = new_matches;
        state.synced_at_revision = revision;
        cx.notify();
    }

    pub fn find_next(&mut self, cx: &mut Context<Self>) {
        let (next_idx, range) = {
            let Some(state) = &self.search_state else {
                return;
            };
            if state.matches.is_empty() {
                return;
            }
            let next_idx = match state.current_match {
                None => 0,
                Some(idx) => (idx + 1) % state.matches.len(),
            };
            let range = state.matches[next_idx].clone();
            (next_idx, range)
        };
        self.search_state.as_mut().unwrap().current_match = Some(next_idx);
        self.select_match_range(range, cx);
    }

    pub fn find_previous(&mut self, cx: &mut Context<Self>) {
        let (prev_idx, range) = {
            let Some(state) = &self.search_state else {
                return;
            };
            if state.matches.is_empty() {
                return;
            }
            let prev_idx = match state.current_match {
                None | Some(0) => state.matches.len() - 1,
                Some(idx) => idx - 1,
            };
            let range = state.matches[prev_idx].clone();
            (prev_idx, range)
        };
        self.search_state.as_mut().unwrap().current_match = Some(prev_idx);
        self.select_match_range(range, cx);
    }

    fn select_match_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.selected_range = range.clone();
        self.selection_reversed = false;
        self.cursor_cell = self.display_cell_for_byte(range.end);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn search_state(&self) -> Option<&SearchState> {
        self.search_state.as_ref()
    }

    pub fn search_match_info(&self) -> (usize, Option<usize>) {
        match &self.search_state {
            None => (0, None),
            Some(state) => (state.matches.len(), state.current_match.map(|i| i + 1)),
        }
    }

    pub fn load_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let prepared = PreparedPlainText::new(text.to_string(), self.plain_text_load_settings());
        self.load_prepared_text(prepared, cx);
    }

    pub fn load_prepared_text(&mut self, prepared: PreparedPlainText, cx: &mut Context<Self>) {
        self.draft = prepared.draft;
        self.bump_draft_revision();
        self.history = EditorHistory::default();
        self.last_applied_edit_batch = None;
        self.rich_text_synced_revision = self.draft_revision;
        self.rich_text_synced_text = prepared.text;
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

    pub fn offset_after_cursor(&self) -> usize {
        editor_selection::offset_after_cursor(self)
    }

    pub fn move_cursor_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        editor_selection::move_cursor_by(self, delta, cx);
    }

    pub fn move_cursor_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        editor_selection::move_cursor_to_display_cell(self, cell_index, cx);
    }

    pub fn move_cursor_to_byte_offset(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        editor_selection::move_cursor_to_byte_offset(self, byte_offset, cx);
    }

    pub fn select_cursor_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        editor_selection::select_cursor_by(self, delta, cx);
    }

    pub fn select_visual_range(
        &mut self,
        anchor_cell: usize,
        cursor_cell: usize,
        cx: &mut Context<Self>,
    ) {
        editor_selection::select_visual_range(self, anchor_cell, cursor_cell, cx);
    }

    pub fn set_block_selection(
        &mut self,
        anchor_cell: usize,
        cursor_cell: usize,
        cx: &mut Context<Self>,
    ) {
        editor_selection::set_block_selection(self, anchor_cell, cursor_cell, cx);
    }

    pub fn clear_block_selection(&mut self, cx: &mut Context<Self>) {
        editor_selection::clear_block_selection(self, cx);
    }

    pub fn collapse_selection_to_cursor_offset(&mut self, cx: &mut Context<Self>) {
        editor_selection::collapse_selection_to_cursor_offset(self, cx);
    }

    pub fn collapse_selection_to_cursor_cell(&mut self, cx: &mut Context<Self>) {
        editor_selection::collapse_selection_to_cursor_cell(self, cx);
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
        editor_history::begin_transaction(self);
    }

    pub fn commit_transaction(&mut self, cx: &mut Context<Self>) -> bool {
        editor_history::commit_transaction(self, cx)
    }

    pub fn active_transaction_inserted_text(&self) -> Option<String> {
        editor_history::active_transaction_inserted_text(self)
    }

    pub fn last_transaction_inserted_text(&self) -> Option<String> {
        editor_history::last_transaction_inserted_text(self)
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) -> bool {
        editor_history::undo(self, cx)
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) -> bool {
        editor_history::redo(self, cx)
    }

    fn select_between_display_cells(
        &mut self,
        anchor_cell: usize,
        cursor_cell: usize,
        cx: &mut Context<Self>,
    ) {
        editor_selection::select_between_display_cells(self, anchor_cell, cursor_cell, cx);
    }

    fn selection_anchor_cell(&self) -> usize {
        editor_selection::selection_anchor_cell(self)
    }

    fn clamped_display_cell_for_point(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        editor_selection::clamped_display_cell_for_point(self, position)
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
                affects_rich_text: true,
            });
        }
        self.draft.replace_range(range.clone(), new_text);
        self.bump_draft_revision();
        let cursor = range.start + new_text.len();
        self.set_cursor_from_offset(cursor);
        if implicit_transaction {
            if !self.commit_transaction(cx) {
                cx.notify();
            }
        } else {
            cx.notify();
        }
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
                inserted_text: new_text.to_string(),
                affects_rich_text: true,
            });
        }
        let cursor = range.start + new_text.len();
        self.draft.replace_range_owned(range, new_text);
        self.bump_draft_revision();
        self.set_cursor_from_offset(cursor);
        if implicit_transaction {
            let committed = self.commit_transaction(cx);
            if !committed {
                cx.notify();
            }
        } else {
            cx.notify();
        }
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
        editor_selection::byte_offset_for_point(self, position)
    }

    fn bounds_for_byte_range(
        &self,
        range: Range<usize>,
        board_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        editor_selection::bounds_for_byte_range(self, range, board_bounds)
    }
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
                affects_rich_text: true,
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
            self.commit_transaction(cx);
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
        default_input::render(self, cx)
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<Event> for Editor {}
