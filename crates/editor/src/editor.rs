use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler, FocusHandle, Focusable,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Render, ScrollWheelEvent, Styled,
    UTF16Selection, Window, actions, div, prelude::*, px,
};
use rope::utf16_to_byte_in_text;
use settings::AppSettings;

use crate::{
    editor_canvas::{
        EditorCanvas, cell_bounds_for_logical_index, logical_index_for_point,
        rows_per_column_for_window_height, visible_columns_for_window_width,
    },
    editor_state::{
        BlockSelection, EditOperation, EditTransaction, EditorState, PendingTransaction,
    },
};

mod editor_canvas;
mod editor_state;

pub(crate) const DEFAULT_VISIBLE_COLUMNS: usize = 20;
pub(crate) const AUTOMATIC_ROWS_RESERVED_CELLS: usize = 4;
pub(crate) const CELL_SIZE: f32 = 28.0;
pub(crate) const RUBY_GUTTER_SIZE: f32 = 10.0;
const IME_ANCHOR_WIDTH: f32 = 2.0;
const IME_ANCHOR_INSET: f32 = 3.0;
const IME_CANDIDATE_GAP: f32 = 16.0;

pub fn init(cx: &mut App) {
    if AppSettings::global(cx).vim_mode {
        return;
    }
    cx.bind_keys([
        gpui::KeyBinding::new("backspace", Backspace, None),
        gpui::KeyBinding::new("delete", Delete, None),
        gpui::KeyBinding::new("up", Up, None),
        gpui::KeyBinding::new("down", Down, None),
        gpui::KeyBinding::new("left", Left, None),
        gpui::KeyBinding::new("right", Right, None),
        gpui::KeyBinding::new("shift-up", SelectUp, None),
        gpui::KeyBinding::new("shift-down", SelectDown, None),
        gpui::KeyBinding::new("shift-left", SelectLeft, None),
        gpui::KeyBinding::new("shift-right", SelectRight, None),
        gpui::KeyBinding::new("cmd-a", SelectAll, None),
        gpui::KeyBinding::new("ctrl-a", SelectAll, None),
        gpui::KeyBinding::new("cmd-v", Paste, None),
        gpui::KeyBinding::new("ctrl-v", Paste, None),
        gpui::KeyBinding::new("cmd-c", Copy, None),
        gpui::KeyBinding::new("ctrl-c", Copy, None),
        gpui::KeyBinding::new("cmd-x", Cut, None),
        gpui::KeyBinding::new("ctrl-x", Cut, None),
        gpui::KeyBinding::new("enter", Enter, None),
        gpui::KeyBinding::new("home", Home, None),
        gpui::KeyBinding::new("end", End, None),
        gpui::KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
    ]);
}

actions!(
    genko,
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
        Enter,
        ShowCharacterPalette,
    ]
);

pub struct Editor {
    pub(crate) state: EditorState,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) last_board_bounds: Option<Bounds<Pixels>>,
}
impl Editor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: EditorState::new(cx),
            focus_handle: cx.focus_handle(),
            last_board_bounds: None,
        }
    }

    pub fn update_viewport_size(&mut self, size: gpui::Size<Pixels>, cx: &App) {
        self.state
            .update_hanging_punctuation(AppSettings::global(cx).hanging_punctuation);
        self.state
            .update_visible_columns(visible_columns_for_window_width(size.width));
        if AppSettings::global(cx).rows_per_column.is_none() {
            self.state
                .update_rows_per_column(rows_per_column_for_window_height(size.height));
        }
    }

    pub fn set_text_input_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.state.text_input_enabled == enabled {
            return;
        }

        self.state.text_input_enabled = enabled;
        if enabled {
            let cursor_offset = self.state.cursor_offset();
            self.state.selected_range = cursor_offset..cursor_offset;
            self.state.selection_reversed = false;
        }
        cx.notify();
    }

    pub fn cursor_cell(&self) -> usize {
        self.state.cursor_cell
    }

    pub fn cursor_byte_offset(&self) -> usize {
        self.state.cursor_offset()
    }

    pub fn rows_per_column(&self) -> usize {
        self.state.rows_per_column()
    }

    pub fn used_cells(&self) -> usize {
        self.state.used_cells()
    }

    pub fn byte_offset_for_display_cell(&self, display_cell_index: usize) -> usize {
        self.state.byte_offset_for_display_cell(display_cell_index)
    }

    pub fn snapshot_text(&self) -> String {
        self.state.draft.slice(0..self.state.draft.len_bytes())
    }

    pub fn text_in_range(&self, range: Range<usize>) -> String {
        self.state.draft.slice(range)
    }

    pub fn selected_byte_range(&self) -> Range<usize> {
        self.state.selected_range.clone()
    }

    pub fn block_selection(&self) -> Option<(usize, usize)> {
        self.state
            .block_selection
            .map(|selection| (selection.anchor_cell, selection.cursor_cell))
    }

    pub fn offset_after_cursor(&self) -> usize {
        self.state.next_boundary(self.state.cursor_offset())
    }

    pub fn move_cursor_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        let target = self.state.cursor_cell.saturating_add_signed(delta);
        self.move_to_display_cell(target, cx);
    }

    pub fn move_cursor_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        self.move_to_display_cell(cell_index, cx);
    }

    pub fn move_cursor_to_byte_offset(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        let cell_index = self.state.display_cell_for_byte(byte_offset);
        self.move_to_display_cell(cell_index, cx);
    }

    pub fn select_cursor_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        let target = self.state.cursor_cell.saturating_add_signed(delta);
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
        let start = self.state.draft.byte_offset_for_display_cell(start_cell);
        let end = self
            .state
            .next_boundary(self.state.draft.byte_offset_for_display_cell(end_cell))
            .max(start);
        self.state.selected_range = start..end;
        self.state.selection_reversed = cursor_cell < anchor_cell;
        self.state.cursor_cell = cursor_cell;
        self.state.block_selection = None;
        self.state.ensure_cursor_visible();
        cx.notify();
    }

    pub fn set_block_selection(
        &mut self,
        anchor_cell: usize,
        cursor_cell: usize,
        cx: &mut Context<Self>,
    ) {
        let cursor_offset = self.state.byte_offset_for_display_cell(cursor_cell);
        self.state.selected_range = cursor_offset..cursor_offset;
        self.state.selection_reversed = false;
        self.state.cursor_cell = cursor_cell;
        self.state.marked_range = None;
        self.state.block_selection = Some(BlockSelection {
            anchor_cell,
            cursor_cell,
        });
        self.state.ensure_cursor_visible();
        cx.notify();
    }

    pub fn clear_block_selection(&mut self, cx: &mut Context<Self>) {
        if self.state.block_selection.take().is_some() {
            cx.notify();
        }
    }

    pub fn collapse_selection_to_cursor_offset(&mut self, cx: &mut Context<Self>) {
        let cursor_offset = self.state.cursor_offset();
        self.state.selected_range = cursor_offset..cursor_offset;
        self.state.selection_reversed = false;
        self.state.marked_range = None;
        self.state.block_selection = None;
        self.state.ensure_cursor_visible();
        cx.notify();
    }

    pub fn collapse_selection_to_cursor_cell(&mut self, cx: &mut Context<Self>) {
        let cursor_offset = self
            .state
            .byte_offset_for_display_cell(self.state.cursor_cell);
        self.state.set_cursor_from_offset(cursor_offset);
        cx.notify();
    }

    pub fn delete_forward_command(&mut self, cx: &mut Context<Self>) {
        self.delete_forward(cx);
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
        if self.state.history.active_transaction.is_none() {
            self.state.history.active_transaction = Some(PendingTransaction {
                before: self.state.view_state(),
                edits: Vec::new(),
            });
        }
    }

    pub fn commit_transaction(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pending) = self.state.history.active_transaction.take() else {
            return false;
        };
        let after = self.state.view_state();
        if pending.edits.is_empty() && pending.before == after {
            return false;
        }

        self.state.history.undo_stack.push(EditTransaction {
            before: pending.before,
            after,
            edits: pending.edits,
        });
        self.state.history.redo_stack.clear();
        cx.notify();
        true
    }

    pub fn cancel_transaction(&mut self) {
        self.state.history.active_transaction = None;
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(transaction) = self.state.history.undo_stack.pop() else {
            return false;
        };
        for edit in transaction.edits.iter().rev() {
            let inserted_end = edit.start + edit.inserted_text.len();
            self.state.draft.replace_range(
                inserted_end.saturating_sub(edit.inserted_text.len())..inserted_end,
                &edit.removed_text,
            );
        }
        self.state.restore_view_state(transaction.before.clone());
        self.state.history.redo_stack.push(transaction);
        cx.notify();
        true
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(transaction) = self.state.history.redo_stack.pop() else {
            return false;
        };
        for edit in &transaction.edits {
            let removed_end = edit.start + edit.removed_text.len();
            self.state.draft.replace_range(
                removed_end.saturating_sub(edit.removed_text.len())..removed_end,
                &edit.inserted_text,
            );
        }
        self.state.restore_view_state(transaction.after.clone());
        self.state.history.undo_stack.push(transaction);
        cx.notify();
        true
    }

    pub fn has_active_transaction(&self) -> bool {
        self.state.history.active_transaction.is_some()
    }

    fn move_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        let offset = self.state.byte_offset_for_display_cell(cell_index);
        self.state.selected_range = offset..offset;
        self.state.selection_reversed = false;
        self.state.cursor_cell = cell_index;
        self.state.block_selection = None;
        self.state.ensure_cursor_visible();
        cx.notify();
    }

    fn select_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        let offset = self.state.byte_offset_for_display_cell(cell_index);
        if self.state.selection_reversed {
            self.state.selected_range.start = offset;
        } else {
            self.state.selected_range.end = offset;
        }
        if self.state.selected_range.end < self.state.selected_range.start {
            self.state.selection_reversed = !self.state.selection_reversed;
            self.state.selected_range =
                self.state.selected_range.end..self.state.selected_range.start;
        }
        self.state.cursor_cell = cell_index;
        self.state.block_selection = None;
        self.state.ensure_cursor_visible();
        cx.notify();
    }

    fn replace_text_in_byte_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        cx: &mut Context<Self>,
    ) {
        let implicit_transaction = self.state.history.active_transaction.is_none();
        if implicit_transaction {
            self.begin_transaction();
        }
        let range = if new_text.is_empty() {
            range
        } else {
            self.state.materialize_cursor_cell_for_insert(range)
        };
        let removed_text = self.state.draft.slice(range.clone());
        if let Some(transaction) = self.state.history.active_transaction.as_mut() {
            transaction.edits.push(EditOperation {
                start: range.start,
                removed_text,
                inserted_text: new_text.to_string(),
            });
        }
        self.state.draft.replace_range(range.clone(), new_text);
        let cursor = range.start + new_text.len();
        self.state.set_cursor_from_offset(cursor);
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
        let implicit_transaction = self.state.history.active_transaction.is_none();
        if implicit_transaction {
            self.begin_transaction();
        }
        let range = if new_text.is_empty() {
            range
        } else {
            self.state.materialize_cursor_cell_for_insert(range)
        };
        let removed_text = self.state.draft.slice(range.clone());
        if let Some(transaction) = self.state.history.active_transaction.as_mut() {
            transaction.edits.push(EditOperation {
                start: range.start,
                removed_text,
                inserted_text: new_text.clone(),
            });
        }
        let cursor = range.start + new_text.len();
        self.state.draft.replace_range_owned(range, new_text);
        self.state.set_cursor_from_offset(cursor);
        if implicit_transaction {
            let _ = self.commit_transaction(cx);
        }
        cx.notify();
    }

    fn scroll_columns_by(&mut self, delta_columns: isize, cx: &mut Context<Self>) {
        if delta_columns == 0 {
            return;
        }

        let previous_column = self.state.scroll_column;
        self.state.scroll_column = self
            .state
            .scroll_column
            .saturating_add_signed(delta_columns)
            .min(self.state.max_scroll_column());

        if self.state.scroll_column != previous_column {
            cx.notify();
        }
    }

    fn byte_offset_for_point(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        let bounds = self.last_board_bounds?;
        let index = logical_index_for_point(
            bounds,
            position,
            self.state.scroll_column,
            self.state.rows_per_column(),
            self.state.visible_columns(),
        )?;
        Some(self.state.draft.byte_offset_for_display_cell(index))
    }

    fn bounds_for_byte_range(
        &self,
        range: Range<usize>,
        board_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let logical_index = if range.is_empty() && range.start == self.state.selected_range.start {
            self.state.cursor_cell
        } else {
            self.state.display_cell_for_byte(range.start)
        };
        let cell_bounds = cell_bounds_for_logical_index(
            board_bounds,
            logical_index,
            self.state.scroll_column,
            self.state.rows_per_column(),
            self.state.visible_columns(),
        )?;
        Some(ime_anchor_bounds_for_cell(cell_bounds, board_bounds))
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.selected_range.is_empty() {
            let previous = self.state.previous_boundary(self.state.cursor_offset());
            self.state.selected_range = previous..self.state.cursor_offset();
        }
        self.replace_text_in_byte_range(self.state.selected_range.clone(), "", cx);
        invalidate_ime_position(window);
    }

    fn delete_forward(&mut self, cx: &mut Context<Self>) {
        if self.state.selected_range.is_empty() {
            let next = self.state.next_boundary(self.state.cursor_offset());
            self.state.selected_range = self.state.cursor_offset()..next;
        }
        self.replace_text_in_byte_range(self.state.selected_range.clone(), "", cx);
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
        self.move_cursor_by(self.state.rows_per_column() as isize, cx);
        invalidate_ime_position(window);
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor_by(-(self.state.rows_per_column() as isize), cx);
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
        self.select_cursor_by(self.state.rows_per_column() as isize, cx);
        invalidate_ime_position(window);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select_cursor_by(-(self.state.rows_per_column() as isize), cx);
        invalidate_ime_position(window);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.state.selected_range = 0..self.state.draft.len_bytes();
        self.state.selection_reversed = false;
        self.state.cursor_cell = self.state.used_cells();
        self.state.block_selection = None;
        invalidate_ime_position(window);
        cx.notify();
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_display_cell(0, cx);
        invalidate_ime_position(window);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_display_cell(self.state.used_cells(), cx);
        invalidate_ime_position(window);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_byte_range_owned(self.state.selected_range.clone(), text, cx);
            invalidate_ime_position(window);
        }
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_byte_range(self.state.selected_range.clone(), "\n", cx);
        invalidate_ime_position(window);
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.state.draft.slice(self.state.selected_range.clone()),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.state.draft.slice(self.state.selected_range.clone()),
            ));
            self.replace_text_in_byte_range(self.state.selected_range.clone(), "", cx);
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
            self.state.scroll_column,
            self.state.rows_per_column(),
            self.state.visible_columns(),
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
        let delta = event.delta.pixel_delta(px(CELL_SIZE));
        let column_delta = if delta.x == Pixels::ZERO {
            -(delta.y / px(CELL_SIZE))
        } else {
            -(delta.x / px(CELL_SIZE))
        };

        self.state.scroll_remainder_columns += column_delta;
        let whole_columns = self.state.scroll_remainder_columns.trunc() as isize;
        if whole_columns != 0 {
            self.state.scroll_remainder_columns -= whole_columns as f32;
            self.scroll_columns_by(whole_columns, cx);
        }
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
        gpui::size(px(IME_ANCHOR_WIDTH), px(CELL_SIZE - IME_ANCHOR_INSET * 2.0)),
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
        let range = self.state.range_from_utf16(&range_utf16);
        actual_range.replace(self.state.range_to_utf16(&range));
        Some(self.state.draft.slice(range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.state.selected_utf16_selection())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.state.marked_range_utf16()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.marked_range = None;
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
        if !self.state.text_input_enabled {
            return;
        }

        let range = self.state.editing_range(range_utf16);
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
        if !self.state.text_input_enabled {
            return;
        }

        let implicit_transaction = self.state.history.active_transaction.is_none();
        if implicit_transaction {
            self.begin_transaction();
        }
        let range = self.state.editing_range(range_utf16);
        let range = if new_text.is_empty() {
            range
        } else {
            self.state.materialize_cursor_cell_for_insert(range)
        };
        let removed_text = self.state.draft.slice(range.clone());
        if let Some(transaction) = self.state.history.active_transaction.as_mut() {
            transaction.edits.push(EditOperation {
                start: range.start,
                removed_text,
                inserted_text: new_text.to_string(),
            });
        }
        self.state.draft.replace_range(range.clone(), new_text);

        let marked_end = range.start + new_text.len();
        self.state.marked_range = (!new_text.is_empty()).then_some(range.start..marked_end);
        self.state.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| {
                let start = utf16_to_byte_in_text(new_text, range_utf16.start);
                let end = utf16_to_byte_in_text(new_text, range_utf16.end);
                range.start + start..range.start + end
            })
            .unwrap_or(marked_end..marked_end);
        self.state.selection_reversed = false;
        self.state.cursor_cell = self.state.display_cell_for_byte(self.state.cursor_offset());
        self.state.ensure_cursor_visible();
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
        let range = self.state.range_from_utf16(&range_utf16);
        self.bounds_for_byte_range(range, board_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let byte_offset = self.byte_offset_for_point(point)?;
        Some(self.state.draft.byte_to_utf16(byte_offset))
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle(cx))
            .key_context("Genko")
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
            .on_action(cx.listener(Self::enter))
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
