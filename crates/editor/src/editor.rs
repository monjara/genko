use std::ops::Range;

use gpui::{
    actions, div, fill, point, prelude::*, px, rgb, rgba, size, App, Bounds, ClipboardItem,
    Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, IntoElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, ParentElement, Pixels, Render, ScrollWheelEvent, Style, Styled, TextAlign,
    TextRun, UTF16Selection, Window,
};
use rope::{utf16_to_byte_in_text, CellText, TextRope};
use settings::AppSettings;
use theme::{GRID_LINE, PAPER_BACKGROUND, SELECTION_BACKGROUND, TEXT_PRIMARY};
use vim::{VimMode, VimState};

const DEFAULT_VISIBLE_COLUMNS: usize = 20;
const AUTOMATIC_ROWS_RESERVED_CELLS: usize = 4;
const CELL_SIZE: f32 = 28.0;
const RUBY_GUTTER_SIZE: f32 = 10.0;

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
        VimEnterInsertMode,
        VimAppend,
        VimNormalMode,
        VimVisualMode,
        VimDeleteChar,
    ]
);

pub struct EditorElement {
    draft: TextRope,
    rows_per_column: usize,
    focus_handle: FocusHandle,
    selected_range: Range<usize>,
    selection_reversed: bool,
    cursor_cell: usize,
    marked_range: Option<Range<usize>>,
    last_board_bounds: Option<Bounds<Pixels>>,
    scroll_column: usize,
    scroll_remainder_columns: f32,
    visible_columns: usize,
    vim: VimState,
}

struct Editor {
    board: Entity<EditorElement>,
}

impl EditorElement {
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("up", Up, None),
            KeyBinding::new("down", Down, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("shift-up", SelectUp, None),
            KeyBinding::new("shift-down", SelectDown, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("cmd-a", SelectAll, None),
            KeyBinding::new("ctrl-a", SelectAll, None),
            KeyBinding::new("cmd-v", Paste, None),
            KeyBinding::new("ctrl-v", Paste, None),
            KeyBinding::new("cmd-c", Copy, None),
            KeyBinding::new("ctrl-c", Copy, None),
            KeyBinding::new("cmd-x", Cut, None),
            KeyBinding::new("ctrl-x", Cut, None),
            KeyBinding::new("enter", Enter, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
            KeyBinding::new("i", VimEnterInsertMode, Some("Genko && vim_mode == normal")),
            KeyBinding::new("a", VimAppend, Some("Genko && vim_mode == normal")),
            KeyBinding::new("escape", VimNormalMode, Some("Genko && vim_mode == insert")),
            KeyBinding::new("escape", VimNormalMode, Some("Genko && vim_mode == visual")),
            KeyBinding::new("v", VimVisualMode, Some("Genko && vim_mode == normal")),
            KeyBinding::new("v", VimNormalMode, Some("Genko && vim_mode == visual")),
            KeyBinding::new("h", Left, Some("Genko && vim_mode == normal")),
            KeyBinding::new("j", Down, Some("Genko && vim_mode == normal")),
            KeyBinding::new("k", Up, Some("Genko && vim_mode == normal")),
            KeyBinding::new("l", Right, Some("Genko && vim_mode == normal")),
            KeyBinding::new("h", Left, Some("Genko && vim_mode == visual")),
            KeyBinding::new("j", Down, Some("Genko && vim_mode == visual")),
            KeyBinding::new("k", Up, Some("Genko && vim_mode == visual")),
            KeyBinding::new("l", Right, Some("Genko && vim_mode == visual")),
            KeyBinding::new("x", VimDeleteChar, Some("Genko && vim_mode == normal")),
            KeyBinding::new("x", VimDeleteChar, Some("Genko && vim_mode == visual")),
        ]);
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let rows_per_column = AppSettings::global(cx)
            .rows_per_column
            .unwrap_or_else(AppSettings::default_rows_per_column);
        let draft = TextRope::new_with_rows(rows_per_column);
        let vim = VimState::new(AppSettings::global(cx).vim_mode);
        Self {
            draft,
            rows_per_column,
            focus_handle: cx.focus_handle(),
            selected_range: 0..0,
            selection_reversed: false,
            cursor_cell: 0,
            marked_range: None,
            last_board_bounds: None,
            scroll_column: 0,
            scroll_remainder_columns: 0.0,
            visible_columns: DEFAULT_VISIBLE_COLUMNS,
            vim,
        }
    }

    pub(crate) fn used_cells(&self) -> usize {
        self.draft.len_display_cells()
    }

    pub(crate) fn scroll_column(&self) -> usize {
        self.scroll_column
    }

    pub(crate) fn visible_columns(&self) -> usize {
        self.visible_columns
    }

    pub(crate) fn total_columns(&self) -> usize {
        let document_columns = self.used_cells().div_ceil(self.rows_per_column()).max(1);
        document_columns.max(self.cursor_column() + 1)
    }

    pub(crate) fn vim_status_label(&self, cx: &App) -> &'static str {
        self.vim.status_label(AppSettings::global(cx).vim_mode)
    }

    pub fn update_viewport_size(&mut self, size: gpui::Size<Pixels>, cx: &App) {
        self.update_visible_columns(visible_columns_for_window_width(size.width));
        if AppSettings::global(cx).rows_per_column.is_none() {
            self.update_rows_per_column(rows_per_column_for_window_height(size.height));
        }
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn vim_key_context(&self, cx: &mut Context<Self>) -> &'static str {
        self.vim.key_context(AppSettings::global(cx).vim_mode)
    }

    fn is_vim_command_mode(&self, cx: &mut Context<Self>) -> bool {
        self.vim.is_command_mode(AppSettings::global(cx).vim_mode)
    }

    fn move_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        let offset = self.draft.byte_offset_for_display_cell(cell_index);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.cursor_cell = cell_index;
        self.vim.set_visual_anchor_cell(None);
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn select_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        let offset = self.draft.byte_offset_for_display_cell(cell_index);
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
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn update_visual_selection(&mut self) {
        let Some(anchor_cell) = self.vim.visual_anchor_cell() else {
            return;
        };
        let start_cell = anchor_cell.min(self.cursor_cell);
        let end_cell = anchor_cell.max(self.cursor_cell);
        let start = self.draft.byte_offset_for_display_cell(start_cell);
        let end = self
            .next_boundary(self.draft.byte_offset_for_display_cell(end_cell))
            .max(start);
        self.selected_range = start..end;
        self.selection_reversed = self.cursor_cell < anchor_cell;
    }

    fn vim_select_to_cell_delta(&mut self, delta: isize, cx: &mut Context<Self>) {
        let target = self.cursor_cell.saturating_add_signed(delta);
        self.cursor_cell = target;
        self.update_visual_selection();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        let grapheme_index = self.draft.grapheme_index_for_byte(offset);
        if grapheme_index == 0 {
            0
        } else {
            self.draft
                .byte_offset_for_grapheme_index(grapheme_index - 1)
        }
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.draft
            .byte_offset_for_grapheme_index(self.draft.grapheme_index_for_byte(offset) + 1)
    }

    fn visible_text(&self) -> Vec<CellText> {
        let first_visible_index = self.first_visible_cell_index();
        self.draft
            .visible_cells(first_visible_index, self.visible_cell_capacity())
    }

    fn rows_per_column(&self) -> usize {
        self.rows_per_column
    }

    fn update_visible_columns(&mut self, visible_columns: usize) {
        self.visible_columns = visible_columns.max(1);
        self.ensure_cursor_visible();
    }

    fn update_rows_per_column(&mut self, rows_per_column: usize) {
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

    fn visible_cell_capacity(&self) -> usize {
        self.rows_per_column() * self.visible_columns()
    }

    fn first_visible_cell_index(&self) -> usize {
        self.scroll_column * self.rows_per_column()
    }

    fn cursor_column(&self) -> usize {
        self.cursor_cell / self.rows_per_column()
    }

    fn max_scroll_column(&self) -> usize {
        self.total_columns().saturating_sub(self.visible_columns())
    }

    fn clamp_scroll_column(&mut self) {
        self.scroll_column = self.scroll_column.min(self.max_scroll_column());
    }

    fn ensure_cursor_visible(&mut self) {
        let cursor_column = self.cursor_column();
        if cursor_column < self.scroll_column {
            self.scroll_column = cursor_column;
        } else if cursor_column >= self.scroll_column + self.visible_columns() {
            self.scroll_column = cursor_column + 1 - self.visible_columns();
        }
        self.clamp_scroll_column();
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

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.draft.byte_to_utf16(range.start)..self.draft.byte_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.draft.utf16_to_byte(range_utf16.start)..self.draft.utf16_to_byte(range_utf16.end)
    }

    fn display_cell_for_byte(&self, byte_offset: usize) -> usize {
        self.draft.display_cell_for_byte(byte_offset)
    }

    fn materialize_cursor_cell_for_insert(&mut self, range: Range<usize>) -> Range<usize> {
        if !range.is_empty() {
            return range;
        }

        let offset = self.draft.materialize_display_cell(self.cursor_cell);
        offset..offset
    }

    fn replace_text_in_byte_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        cx: &mut Context<Self>,
    ) {
        let range = if new_text.is_empty() {
            range
        } else {
            self.materialize_cursor_cell_for_insert(range)
        };
        self.draft.replace_range(range.clone(), new_text);
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.cursor_cell = self.display_cell_for_byte(cursor);
        self.marked_range = None;
        self.vim.set_visual_anchor_cell(None);
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn replace_text_in_byte_range_owned(
        &mut self,
        range: Range<usize>,
        new_text: String,
        cx: &mut Context<Self>,
    ) {
        let range = if new_text.is_empty() {
            range
        } else {
            self.materialize_cursor_cell_for_insert(range)
        };
        let cursor = range.start + new_text.len();
        self.draft.replace_range_owned(range, new_text);
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.cursor_cell = self.display_cell_for_byte(cursor);
        self.marked_range = None;
        self.vim.set_visual_anchor_cell(None);
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn editing_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone())
    }

    fn move_to_cell_delta(&mut self, delta: isize, cx: &mut Context<Self>) {
        let target = self.cursor_cell.saturating_add_signed(delta);
        self.move_to_display_cell(target, cx);
    }

    fn select_to_cell_delta(&mut self, delta: isize, cx: &mut Context<Self>) {
        let target = self.cursor_cell.saturating_add_signed(delta);
        self.select_to_display_cell(target, cx);
    }

    fn backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            self.selected_range = previous..self.cursor_offset();
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
    }

    fn delete_forward(&mut self, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            self.selected_range = self.cursor_offset()..next;
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
    }

    fn delete(&mut self, _: &Delete, _window: &mut Window, cx: &mut Context<Self>) {
        self.delete_forward(cx);
    }

    fn up(&mut self, _: &Up, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() == VimMode::Visual {
            self.vim_select_to_cell_delta(-1, cx);
            return;
        }
        self.move_to_cell_delta(-1, cx);
    }

    fn down(&mut self, _: &Down, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() == VimMode::Visual {
            self.vim_select_to_cell_delta(1, cx);
            return;
        }
        self.move_to_cell_delta(1, cx);
    }

    fn left(&mut self, _: &Left, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() == VimMode::Visual {
            self.vim_select_to_cell_delta(self.rows_per_column() as isize, cx);
            return;
        }
        self.move_to_cell_delta(self.rows_per_column() as isize, cx);
    }

    fn right(&mut self, _: &Right, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() == VimMode::Visual {
            self.vim_select_to_cell_delta(-(self.rows_per_column() as isize), cx);
            return;
        }
        self.move_to_cell_delta(-(self.rows_per_column() as isize), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to_cell_delta(-1, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to_cell_delta(1, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to_cell_delta(self.rows_per_column() as isize, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to_cell_delta(-(self.rows_per_column() as isize), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.draft.len_bytes();
        self.selection_reversed = false;
        self.cursor_cell = self.used_cells();
        cx.notify();
    }

    fn home(&mut self, _: &Home, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_display_cell(0, cx);
    }

    fn end(&mut self, _: &End, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_display_cell(self.used_cells(), cx);
    }

    fn paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_byte_range_owned(self.selected_range.clone(), text, cx);
        }
    }

    fn enter(&mut self, _: &Enter, _window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_byte_range(self.selected_range.clone(), "\n", cx);
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.draft.slice(self.selected_range.clone()),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.draft.slice(self.selected_range.clone()),
            ));
            self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
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

    fn vim_enter_insert_mode(
        &mut self,
        _: &VimEnterInsertMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vim.set_mode(VimMode::Insert);
        self.vim.set_visual_anchor_cell(None);
        self.selected_range = self.cursor_offset()..self.cursor_offset();
        self.selection_reversed = false;
        cx.notify();
    }

    fn vim_append(&mut self, _: &VimAppend, _window: &mut Window, cx: &mut Context<Self>) {
        self.vim.set_mode(VimMode::Insert);
        self.vim.set_visual_anchor_cell(None);
        self.move_to_cell_delta(1, cx);
    }

    fn vim_normal_mode(&mut self, _: &VimNormalMode, _window: &mut Window, cx: &mut Context<Self>) {
        if AppSettings::global(cx).vim_mode {
            let cursor_offset = if self.vim.mode() == VimMode::Visual {
                self.draft.byte_offset_for_display_cell(self.cursor_cell)
            } else {
                self.cursor_offset()
            };
            self.vim.set_mode(VimMode::Normal);
            self.vim.set_visual_anchor_cell(None);
            self.marked_range = None;
            self.selected_range = cursor_offset..cursor_offset;
            self.selection_reversed = false;
            self.cursor_cell = self.display_cell_for_byte(cursor_offset);
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    fn vim_visual_mode(&mut self, _: &VimVisualMode, _window: &mut Window, cx: &mut Context<Self>) {
        self.vim.set_mode(VimMode::Visual);
        self.vim.set_visual_anchor_cell(Some(self.cursor_cell));
        self.update_visual_selection();
        cx.notify();
    }

    fn vim_delete_char(&mut self, _: &VimDeleteChar, _window: &mut Window, cx: &mut Context<Self>) {
        self.delete_forward(cx);
        if AppSettings::global(cx).vim_mode {
            self.vim.set_mode(VimMode::Normal);
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
            self.rows_per_column(),
            self.visible_columns(),
        ) {
            if event.modifiers.shift {
                self.select_to_display_cell(cell_index, cx);
            } else {
                self.move_to_display_cell(cell_index, cx);
            }
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

        self.scroll_remainder_columns += column_delta;
        let whole_columns = self.scroll_remainder_columns.trunc() as isize;
        if whole_columns != 0 {
            self.scroll_remainder_columns -= whole_columns as f32;
            self.scroll_columns_by(whole_columns, cx);
        }
    }

    fn byte_offset_for_point(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        let bounds = self.last_board_bounds?;
        let index = logical_index_for_point(
            bounds,
            position,
            self.scroll_column,
            self.rows_per_column(),
            self.visible_columns(),
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
        cell_bounds_for_logical_index(
            board_bounds,
            logical_index,
            self.scroll_column,
            self.rows_per_column(),
            self.visible_columns(),
        )
    }

    fn paint_paper(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        window.paint_quad(fill(bounds, rgb(PAPER_BACKGROUND)));
    }

    fn paint_grid(
        &self,
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

    fn paint_selection(
        &self,
        visible_text: &[CellText],
        selected_range: &Range<usize>,
        marked_range: Option<&Range<usize>>,
        bounds: Bounds<Pixels>,
        scroll_column: usize,
        rows_per_column: usize,
        visible_columns: usize,
        window: &mut Window,
    ) {
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

    fn paint_text(
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
                Self::paint_attached_punctuation(cell_text, cell_bounds, window, cx);
            } else {
                Self::paint_cell_text(cell_text, cell_bounds, window, cx);
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
            gpui::TextAlign::Center,
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

    fn paint_cursor(
        &self,
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
}

impl EntityInputHandler for EditorElement {
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
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_vim_command_mode(cx) {
            return;
        }

        let range = self.editing_range(range_utf16);
        self.replace_text_in_byte_range(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_vim_command_mode(cx) {
            return;
        }

        let range = self.editing_range(range_utf16);
        let range = if new_text.is_empty() {
            range
        } else {
            self.materialize_cursor_cell_for_insert(range)
        };
        self.draft.replace_range(range.clone(), new_text);

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

impl Render for EditorElement {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle(cx))
            .key_context(self.vim_key_context(cx))
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
            .on_action(cx.listener(Self::vim_enter_insert_mode))
            .on_action(cx.listener(Self::vim_append))
            .on_action(cx.listener(Self::vim_normal_mode))
            .on_action(cx.listener(Self::vim_visual_mode))
            .on_action(cx.listener(Self::vim_delete_char))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_board_mouse_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .cursor(CursorStyle::IBeam)
            .child(Editor { board: cx.entity() })
    }
}

impl Focusable for EditorElement {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl IntoElement for Editor {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Editor {
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
        let board = self.board.read(cx);
        let mut style = Style::default();
        style.size.width = board_width_for_columns(board.visible_columns()).into();
        style.size.height = px(CELL_SIZE * board.rows_per_column() as f32).into();
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
        let focus_handle = self.board.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.board.clone()),
            cx,
        );
        self.board.update(cx, |board, _cx| {
            board.last_board_bounds = Some(bounds);
        });

        let show_grid = AppSettings::global(cx).show_grid_lines;
        let (
            visible_text,
            selected_range,
            marked_range,
            cursor_index,
            scroll_column,
            rows_per_column,
            visible_columns,
        ) = {
            let board = self.board.read(cx);
            (
                board.visible_text(),
                board.selected_range.clone(),
                board.marked_range.clone(),
                board.cursor_cell,
                board.scroll_column,
                board.rows_per_column(),
                board.visible_columns(),
            )
        };

        self.board.read(cx).paint_paper(bounds, window);
        self.board.read(cx).paint_selection(
            &visible_text,
            &selected_range,
            marked_range.as_ref(),
            bounds,
            scroll_column,
            rows_per_column,
            visible_columns,
            window,
        );
        if show_grid {
            self.board
                .read(cx)
                .paint_grid(bounds, rows_per_column, visible_columns, window);
        }
        EditorElement::paint_text(
            &visible_text,
            bounds,
            scroll_column,
            rows_per_column,
            visible_columns,
            window,
            cx,
        );
        if focus_handle.is_focused(window) {
            self.board.read(cx).paint_cursor(
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

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
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

fn board_width_for_columns(visible_columns: usize) -> Pixels {
    if visible_columns == 0 {
        return Pixels::ZERO;
    }

    px(visible_columns as f32 * CELL_SIZE
        + visible_columns.saturating_sub(1) as f32 * RUBY_GUTTER_SIZE)
}

fn board_x_for_visible_column(board_left: Pixels, column: usize) -> Pixels {
    board_left + px(column as f32 * (CELL_SIZE + RUBY_GUTTER_SIZE))
}

fn visible_columns_for_window_width(width: Pixels) -> usize {
    (((width + px(RUBY_GUTTER_SIZE)) / px(CELL_SIZE + RUBY_GUTTER_SIZE)).floor() as usize)
        .saturating_sub(2)
        .max(1)
}

fn rows_per_column_for_window_height(height: Pixels) -> usize {
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
