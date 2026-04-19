use std::{ops::Range, sync::Arc};

use gpui::{
    App, Application, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent, ParentElement, Pixels, Render,
    ScrollWheelEvent, SharedString, Style, Styled, TextRun, UTF16Selection, Window, WindowBounds,
    WindowOptions, actions, div, fill, point, prelude::*, px, rgb, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

const ROWS: usize = 20;
const COLUMNS: usize = 20;
const CELL_SIZE: f32 = 28.0;
const VISIBLE_CELL_CAPACITY: usize = ROWS * COLUMNS;
const ROPE_LEAF_BYTES: usize = 1024;

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
        Quit,
    ]
);

struct GenkoApp {
    title: SharedString,
    draft: TextRope,
    focus_handle: FocusHandle,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_board_bounds: Option<Bounds<Pixels>>,
    scroll_column: usize,
    scroll_remainder_columns: f32,
}

impl GenkoApp {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            title: "Genko".into(),
            draft: TextRope::new(),
            focus_handle: cx.focus_handle(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_board_bounds: None,
            scroll_column: 0,
            scroll_remainder_columns: 0.0,
        }
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.draft.len_bytes());
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.draft.len_bytes());
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
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

    fn byte_offset_for_grapheme_index(&self, target_index: usize) -> usize {
        self.draft.byte_offset_for_grapheme_index(target_index)
    }

    fn grapheme_index_for_byte(&self, byte_offset: usize) -> usize {
        self.draft.grapheme_index_for_byte(byte_offset)
    }

    fn visible_text(&self) -> Vec<CellText> {
        let first_visible_index = self.first_visible_cell_index();
        self.draft
            .visible_cells(first_visible_index, VISIBLE_CELL_CAPACITY)
    }

    fn used_cells(&self) -> usize {
        self.draft.len_display_cells()
    }

    fn first_visible_cell_index(&self) -> usize {
        self.scroll_column * ROWS
    }

    fn cursor_column(&self) -> usize {
        self.display_cell_for_byte(self.cursor_offset()) / ROWS
    }

    fn total_columns(&self) -> usize {
        let document_columns = self.used_cells().div_ceil(ROWS).max(1);
        document_columns.max(self.cursor_column() + 1)
    }

    fn max_scroll_column(&self) -> usize {
        self.total_columns().saturating_sub(COLUMNS)
    }

    fn clamp_scroll_column(&mut self) {
        self.scroll_column = self.scroll_column.min(self.max_scroll_column());
    }

    fn ensure_cursor_visible(&mut self) {
        let cursor_column = self.cursor_column();
        if cursor_column < self.scroll_column {
            self.scroll_column = cursor_column;
        } else if cursor_column >= self.scroll_column + COLUMNS {
            self.scroll_column = cursor_column + 1 - COLUMNS;
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

    fn replace_text_in_byte_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        cx: &mut Context<Self>,
    ) {
        self.draft.replace_range(range.clone(), new_text);
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn replace_text_in_byte_range_owned(
        &mut self,
        range: Range<usize>,
        new_text: String,
        cx: &mut Context<Self>,
    ) {
        let cursor = range.start + new_text.len();
        self.draft.replace_range_owned(range, new_text);
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
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

    fn move_to_grapheme_delta(&mut self, delta: isize, cx: &mut Context<Self>) {
        let current = self.grapheme_index_for_byte(self.cursor_offset());
        let target = current.saturating_add_signed(delta);
        self.move_to(self.byte_offset_for_grapheme_index(target), cx);
    }

    fn select_to_grapheme_delta(&mut self, delta: isize, cx: &mut Context<Self>) {
        let current = self.grapheme_index_for_byte(self.cursor_offset());
        let target = current.saturating_add_signed(delta);
        self.select_to(self.byte_offset_for_grapheme_index(target), cx);
    }

    fn backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            self.selected_range = previous..self.cursor_offset();
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
    }

    fn delete(&mut self, _: &Delete, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            self.selected_range = self.cursor_offset()..next;
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
    }

    fn up(&mut self, _: &Up, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn down(&mut self, _: &Down, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn left(&mut self, _: &Left, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_grapheme_delta(ROWS as isize, cx);
    }

    fn right(&mut self, _: &Right, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_grapheme_delta(-(ROWS as isize), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to_grapheme_delta(ROWS as isize, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to_grapheme_delta(-(ROWS as isize), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.draft.len_bytes();
        self.selection_reversed = false;
        cx.notify();
    }

    fn home(&mut self, _: &Home, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.draft.len_bytes(), cx);
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

    fn on_board_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        if let Some(offset) = self.byte_offset_for_point(event.position) {
            if event.modifiers.shift {
                self.select_to(offset, cx);
            } else {
                self.move_to(offset, cx);
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
        let index = logical_index_for_point(bounds, position, self.scroll_column)?;
        Some(self.draft.byte_offset_for_display_cell(index))
    }

    fn bounds_for_byte_range(
        &self,
        range: Range<usize>,
        board_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let logical_index = self.display_cell_for_byte(range.start);
        cell_bounds_for_logical_index(board_bounds, logical_index, self.scroll_column)
    }

    fn render_header(&self) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .justify_between()
            .items_end()
            .text_color(rgb(0x2f241d))
            .child(
                div()
                    .text_2xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(self.title.clone()),
            )
            .child(div().text_sm().text_color(rgb(0x705a4a)).child(format!(
                "vertical / {} cells / columns {}-{} of {}",
                self.used_cells(),
                self.scroll_column + 1,
                (self.scroll_column + COLUMNS).min(self.total_columns()),
                self.total_columns()
            )))
    }
}

impl EntityInputHandler for GenkoApp {
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
        let range = self.editing_range(range_utf16);
        self.draft.replace_range(range.clone(), new_text);

        let marked_end = range.start + new_text.len();
        self.marked_range = (!new_text.is_empty()).then_some(range.start..marked_end);
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| {
                let start = utf16_to_byte_in_str(new_text, range_utf16.start);
                let end = utf16_to_byte_in_str(new_text, range_utf16.end);
                range.start + start..range.start + end
            })
            .unwrap_or(marked_end..marked_end);
        self.selection_reversed = false;
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

impl Render for GenkoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0xebe5d8))
            .flex()
            .items_center()
            .justify_center()
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .items_center()
                    .child(self.render_header())
                    .child(BoardElement { app: cx.entity() }),
            )
    }
}

impl Focusable for GenkoApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

struct CellText {
    logical_index: usize,
    text: String,
    range: Range<usize>,
}

struct BoardElement {
    app: Entity<GenkoApp>,
}

impl IntoElement for BoardElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BoardElement {
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
        let mut style = Style::default();
        style.size.width = px(CELL_SIZE * COLUMNS as f32).into();
        style.size.height = px(CELL_SIZE * ROWS as f32).into();
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
        let focus_handle = self.app.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.app.clone()),
            cx,
        );
        self.app.update(cx, |app, _cx| {
            app.last_board_bounds = Some(bounds);
        });

        self.paint_paper(bounds, window);

        let (visible_text, selected_range, marked_range, cursor_index, scroll_column) = {
            let app = self.app.read(cx);
            (
                app.visible_text(),
                app.selected_range.clone(),
                app.marked_range.clone(),
                app.display_cell_for_byte(app.cursor_offset()),
                app.scroll_column,
            )
        };

        self.paint_selection(
            &visible_text,
            &selected_range,
            marked_range.as_ref(),
            bounds,
            scroll_column,
            window,
        );
        self.paint_grid(bounds, window);
        self.paint_text(&visible_text, bounds, scroll_column, window, cx);
        if focus_handle.is_focused(window) {
            self.paint_cursor(cursor_index, bounds, scroll_column, window);
        }
    }
}

impl BoardElement {
    fn paint_paper(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        window.paint_quad(fill(bounds, rgb(0xfffbf2)));
    }

    fn paint_grid(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        for column in 0..=COLUMNS {
            let x = bounds.left() + px(column as f32 * CELL_SIZE);
            window.paint_quad(fill(
                Bounds::new(point(x, bounds.top()), size(px(1.0), bounds.size.height)),
                rgb(0xd94b4b),
            ));
        }

        for row in 0..=ROWS {
            let y = bounds.top() + px(row as f32 * CELL_SIZE);
            window.paint_quad(fill(
                Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(1.0))),
                rgb(0xd94b4b),
            ));
        }
    }

    fn paint_selection(
        &self,
        visible_text: &[CellText],
        selected_range: &Range<usize>,
        marked_range: Option<&Range<usize>>,
        bounds: Bounds<Pixels>,
        scroll_column: usize,
        window: &mut Window,
    ) {
        for cell_text in visible_text {
            if ranges_overlap(&cell_text.range, selected_range) {
                let Some(cell_bounds) =
                    cell_bounds_for_logical_index(bounds, cell_text.logical_index, scroll_column)
                else {
                    continue;
                };
                window.paint_quad(fill(cell_bounds, rgba(0x2f6fff30)));
            } else if marked_range.is_some_and(|range| ranges_overlap(&cell_text.range, range)) {
                let Some(cell_bounds) =
                    cell_bounds_for_logical_index(bounds, cell_text.logical_index, scroll_column)
                else {
                    continue;
                };
                let underline_y = cell_bounds.bottom() - px(4.0);
                window.paint_quad(fill(
                    Bounds::new(
                        point(cell_bounds.left() + px(5.0), underline_y),
                        size(px(CELL_SIZE - 10.0), px(2.0)),
                    ),
                    rgb(0x2f241d),
                ));
            }
        }
    }

    fn paint_text(
        &self,
        visible_text: &[CellText],
        bounds: Bounds<Pixels>,
        scroll_column: usize,
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = window.text_style();
        let font_size = px(21.0);
        let line_height = px(24.0);

        for cell_text in visible_text {
            let run = TextRun {
                len: cell_text.text.len(),
                font: style.font(),
                color: rgb(0x2f241d).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                cell_text.text.clone().into(),
                font_size,
                &[run],
                None,
            );
            let Some(cell_bounds) =
                cell_bounds_for_logical_index(bounds, cell_text.logical_index, scroll_column)
            else {
                continue;
            };
            let text_origin = point(
                cell_bounds.left() + (px(CELL_SIZE) - line.width) / 2.0,
                cell_bounds.top() + (px(CELL_SIZE) - line_height) / 2.0,
            );
            line.paint(text_origin, line_height, window, cx).ok();
        }
    }

    fn paint_cursor(
        &self,
        cursor_index: usize,
        bounds: Bounds<Pixels>,
        scroll_column: usize,
        window: &mut Window,
    ) {
        let Some(cell_bounds) = cell_bounds_for_logical_index(bounds, cursor_index, scroll_column)
        else {
            return;
        };
        window.paint_quad(fill(
            Bounds::new(
                point(cell_bounds.left() + px(4.0), cell_bounds.top() + px(3.0)),
                size(px(CELL_SIZE - 8.0), px(2.0)),
            ),
            rgb(0x2f241d),
        ));
    }
}

#[derive(Clone, Debug, Default)]
struct TextRope {
    root: Option<Box<RopeNode>>,
}

impl TextRope {
    fn new() -> Self {
        Self::default()
    }

    fn len_bytes(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.bytes())
    }

    fn len_graphemes(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.graphemes())
    }

    fn len_display_cells(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.cell_advance(0))
    }

    #[cfg(test)]
    fn height(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.height())
    }

    #[cfg(test)]
    fn assert_balanced(&self) {
        if let Some(root) = self.root.as_ref() {
            root.assert_balanced();
        }
    }

    #[cfg(test)]
    fn shared_leaf_count(&self) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.shared_leaf_count())
    }

    fn from_str(text: &str) -> Self {
        Self {
            root: RopeNode::from_str(text),
        }
    }

    #[cfg(test)]
    fn from_string(text: String) -> Self {
        Self {
            root: RopeNode::from_string(text),
        }
    }

    fn to_string(&self) -> String {
        let mut output = String::with_capacity(self.len_bytes());
        if let Some(root) = self.root.as_ref() {
            root.push_to_string(&mut output);
        }
        output
    }

    fn slice(&self, range: Range<usize>) -> String {
        let mut output = String::with_capacity(range.end.saturating_sub(range.start));
        if let Some(root) = self.root.as_ref() {
            root.push_range(range, 0, &mut output);
        }
        output
    }

    fn visible_cells(&self, start_index: usize, max_count: usize) -> Vec<CellText> {
        let mut cells = Vec::with_capacity(max_count.min(self.len_display_cells()));
        if let Some(root) = self.root.as_ref() {
            root.push_visible_cells(
                start_index..start_index.saturating_add(max_count),
                0,
                0,
                &mut cells,
            );
        }
        cells
    }

    #[cfg(test)]
    fn visible_graphemes(&self, start_index: usize, max_count: usize) -> Vec<CellText> {
        self.visible_cells(start_index, max_count)
    }

    fn replace_range(&mut self, range: Range<usize>, text: &str) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len_bytes());

        if range.start == range.end
            && range.end == self.len_bytes()
            && self.try_append_to_last_leaf(text)
        {
            return;
        }

        let root = self.root.take();
        let (left, rest) = split_node(root, range.start);
        let (_, right) = split_node(rest, range.end - range.start);
        self.root = concat_nodes(concat_nodes(left, RopeNode::from_str(text)), right);
    }

    fn replace_range_owned(&mut self, range: Range<usize>, text: String) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len_bytes());

        if range.start == range.end
            && range.end == self.len_bytes()
            && self.root.is_some()
            && self.try_append_to_last_leaf(&text)
        {
            return;
        }

        let inserted = RopeNode::from_string(text);
        let root = self.root.take();
        let (left, rest) = split_node(root, range.start);
        let (_, right) = split_node(rest, range.end - range.start);
        self.root = concat_nodes(concat_nodes(left, inserted), right);
    }

    fn try_append_to_last_leaf(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return true;
        }

        if self.root.is_none() {
            self.root = RopeNode::from_str(text);
            return true;
        }

        if text.len() > ROPE_LEAF_BYTES {
            return false;
        }

        self.root
            .as_mut()
            .is_some_and(|root| root.try_append_to_last_leaf(text))
    }

    fn byte_to_utf16(&self, byte_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.byte_to_utf16(byte_offset))
    }

    fn utf16_to_byte(&self, utf16_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.utf16_to_byte(utf16_offset))
    }

    fn byte_offset_for_grapheme_index(&self, grapheme_index: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.grapheme_to_byte(grapheme_index))
    }

    fn grapheme_index_for_byte(&self, byte_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.byte_to_grapheme(byte_offset))
    }

    fn display_cell_for_byte(&self, byte_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.byte_to_display_cell(byte_offset, 0))
    }

    fn byte_offset_for_display_cell(&self, display_cell_index: usize) -> usize {
        self.root.as_ref().map_or(0, |node| {
            node.display_cell_to_byte(display_cell_index, 0, 0)
        })
    }
}

#[derive(Clone, Debug)]
enum RopeNode {
    Leaf {
        text: RopeLeafText,
        bytes: usize,
        utf16: usize,
        graphemes: usize,
        cell_advances: [usize; ROWS],
        height: usize,
    },
    Branch {
        left: Box<RopeNode>,
        right: Box<RopeNode>,
        bytes: usize,
        utf16: usize,
        graphemes: usize,
        cell_advances: [usize; ROWS],
        height: usize,
    },
}

#[derive(Clone, Debug)]
enum RopeLeafText {
    Owned(String),
    Shared {
        source: Arc<String>,
        range: Range<usize>,
    },
}

impl RopeLeafText {
    fn as_str(&self) -> &str {
        match self {
            Self::Owned(text) => text,
            Self::Shared { source, range } => &source[range.clone()],
        }
    }

    fn split_at(self, byte_offset: usize) -> (Option<Box<RopeNode>>, Option<Box<RopeNode>>) {
        match self {
            Self::Owned(mut text) => {
                let right = text.split_off(byte_offset);
                (RopeNode::from_string(text), RopeNode::from_string(right))
            }
            Self::Shared { source, range } => {
                let split_offset = range.start + byte_offset;
                (
                    RopeNode::shared_leaf(source.clone(), range.start..split_offset),
                    RopeNode::shared_leaf(source, split_offset..range.end),
                )
            }
        }
    }
}

impl RopeNode {
    fn from_str(text: &str) -> Option<Box<Self>> {
        if text.is_empty() {
            return None;
        }

        let chunks = chunk_string(text);
        build_balanced(chunks)
    }

    fn from_string(text: String) -> Option<Box<Self>> {
        if text.is_empty() {
            return None;
        }

        if text.len() <= ROPE_LEAF_BYTES {
            return Some(Self::leaf(text));
        }

        build_balanced_nodes(chunk_shared_string(Arc::new(text)))
    }

    fn leaf(text: String) -> Box<Self> {
        Self::leaf_text(RopeLeafText::Owned(text))
    }

    fn shared_leaf(source: Arc<String>, range: Range<usize>) -> Option<Box<Self>> {
        if range.is_empty() {
            None
        } else {
            Some(Self::leaf_text(RopeLeafText::Shared { source, range }))
        }
    }

    fn leaf_text(text: RopeLeafText) -> Box<Self> {
        let (bytes, utf16, graphemes) = {
            let text = text.as_str();
            (
                text.len(),
                text.encode_utf16().count(),
                text.graphemes(true).count(),
            )
        };
        let cell_advances = cell_advances_for_text(text.as_str(), graphemes);
        Box::new(Self::Leaf {
            text,
            bytes,
            utf16,
            graphemes,
            cell_advances,
            height: 1,
        })
    }

    fn bytes(&self) -> usize {
        match self {
            Self::Leaf { bytes, .. } | Self::Branch { bytes, .. } => *bytes,
        }
    }

    fn utf16(&self) -> usize {
        match self {
            Self::Leaf { utf16, .. } | Self::Branch { utf16, .. } => *utf16,
        }
    }

    fn graphemes(&self) -> usize {
        match self {
            Self::Leaf { graphemes, .. } | Self::Branch { graphemes, .. } => *graphemes,
        }
    }

    fn cell_advance(&self, start_row: usize) -> usize {
        match self {
            Self::Leaf { cell_advances, .. } | Self::Branch { cell_advances, .. } => {
                cell_advances[start_row]
            }
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Leaf { height, .. } | Self::Branch { height, .. } => *height,
        }
    }

    fn try_append_to_last_leaf(&mut self, appended_text: &str) -> bool {
        match self {
            Self::Leaf {
                text: RopeLeafText::Owned(text),
                bytes,
                utf16,
                graphemes,
                cell_advances,
                ..
            } => {
                if text.len() + appended_text.len() > ROPE_LEAF_BYTES {
                    return false;
                }

                text.push_str(appended_text);
                *bytes = text.len();
                *utf16 = text.encode_utf16().count();
                *graphemes = text.graphemes(true).count();
                *cell_advances = cell_advances_for_text(text, *graphemes);
                true
            }
            Self::Leaf { .. } => false,
            Self::Branch {
                left,
                right,
                bytes,
                utf16,
                graphemes,
                cell_advances,
                height,
            } => {
                if !right.try_append_to_last_leaf(appended_text) {
                    return false;
                }

                *bytes = left.bytes() + right.bytes();
                *utf16 = left.utf16() + right.utf16();
                *graphemes = left.graphemes() + right.graphemes();
                *cell_advances = compose_cell_advances(&left, &right);
                *height = left.height().max(right.height()) + 1;
                true
            }
        }
    }

    fn push_to_string(&self, output: &mut String) {
        match self {
            Self::Leaf { text, .. } => output.push_str(text.as_str()),
            Self::Branch { left, right, .. } => {
                left.push_to_string(output);
                right.push_to_string(output);
            }
        }
    }

    fn push_range(&self, range: Range<usize>, node_start: usize, output: &mut String) {
        let node_end = node_start + self.bytes();
        if range.end <= node_start || range.start >= node_end {
            return;
        }

        match self {
            Self::Leaf { text, .. } => {
                let local_start = range.start.saturating_sub(node_start);
                let local_end = (range.end.min(node_end)) - node_start;
                output.push_str(&text.as_str()[local_start..local_end]);
            }
            Self::Branch { left, right, .. } => {
                left.push_range(range.clone(), node_start, output);
                right.push_range(range, node_start + left.bytes(), output);
            }
        }
    }

    fn push_visible_cells(
        &self,
        target_range: Range<usize>,
        node_byte_start: usize,
        node_cell_start: usize,
        output: &mut Vec<CellText>,
    ) {
        let node_cell_end = node_cell_start + self.cell_advance(node_cell_start % ROWS);
        if target_range.end <= node_cell_start || target_range.start >= node_cell_end {
            return;
        }

        match self {
            Self::Leaf { text, .. } => {
                let text = text.as_str();
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.grapheme_indices(true) {
                    if grapheme == "\n" {
                        cell_index = next_line_cell_index(cell_index);
                        continue;
                    }

                    if target_range.contains(&cell_index) {
                        let byte_start = node_byte_start + local_byte_start;
                        output.push(CellText {
                            logical_index: cell_index,
                            text: grapheme.to_string(),
                            range: byte_start..byte_start + grapheme.len(),
                        });
                    }

                    cell_index += 1;
                }
            }
            Self::Branch { left, right, .. } => {
                left.push_visible_cells(
                    target_range.clone(),
                    node_byte_start,
                    node_cell_start,
                    output,
                );
                let right_cell_start = node_cell_start + left.cell_advance(node_cell_start % ROWS);
                right.push_visible_cells(
                    target_range,
                    node_byte_start + left.bytes(),
                    right_cell_start,
                    output,
                );
            }
        }
    }

    fn byte_to_utf16(&self, byte_offset: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                byte_to_utf16_in_str(text.as_str(), byte_offset.min(*bytes))
            }
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.byte_to_utf16(byte_offset)
                } else {
                    left.utf16() + right.byte_to_utf16(byte_offset - left.bytes())
                }
            }
        }
    }

    fn utf16_to_byte(&self, utf16_offset: usize) -> usize {
        match self {
            Self::Leaf { text, utf16, .. } => {
                utf16_to_byte_in_str(text.as_str(), utf16_offset.min(*utf16))
            }
            Self::Branch { left, right, .. } => {
                if utf16_offset <= left.utf16() {
                    left.utf16_to_byte(utf16_offset)
                } else {
                    left.bytes() + right.utf16_to_byte(utf16_offset - left.utf16())
                }
            }
        }
    }

    fn grapheme_to_byte(&self, grapheme_index: usize) -> usize {
        match self {
            Self::Leaf {
                text,
                bytes,
                graphemes,
                ..
            } => {
                if grapheme_index >= *graphemes {
                    *bytes
                } else {
                    text.as_str()
                        .grapheme_indices(true)
                        .nth(grapheme_index)
                        .map(|(offset, _)| offset)
                        .unwrap_or(*bytes)
                }
            }
            Self::Branch { left, right, .. } => {
                if grapheme_index <= left.graphemes() {
                    left.grapheme_to_byte(grapheme_index)
                } else {
                    left.bytes() + right.grapheme_to_byte(grapheme_index - left.graphemes())
                }
            }
        }
    }

    fn byte_to_grapheme(&self, byte_offset: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => text
                .as_str()
                .grapheme_indices(true)
                .take_while(|(offset, _)| *offset < byte_offset.min(*bytes))
                .count(),
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.byte_to_grapheme(byte_offset)
                } else {
                    left.graphemes() + right.byte_to_grapheme(byte_offset - left.bytes())
                }
            }
        }
    }

    fn byte_to_display_cell(&self, byte_offset: usize, node_cell_start: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.as_str().grapheme_indices(true) {
                    if local_byte_start >= byte_offset.min(*bytes) {
                        break;
                    }

                    if grapheme == "\n" {
                        cell_index = next_line_cell_index(cell_index);
                    } else {
                        cell_index += 1;
                    }
                }
                cell_index
            }
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.byte_to_display_cell(byte_offset, node_cell_start)
                } else {
                    let right_cell_start =
                        node_cell_start + left.cell_advance(node_cell_start % ROWS);
                    right.byte_to_display_cell(byte_offset - left.bytes(), right_cell_start)
                }
            }
        }
    }

    fn display_cell_to_byte(
        &self,
        target_cell_index: usize,
        node_byte_start: usize,
        node_cell_start: usize,
    ) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.as_str().grapheme_indices(true) {
                    if target_cell_index <= cell_index {
                        return node_byte_start + local_byte_start;
                    }

                    if grapheme == "\n" {
                        let next_cell_index = next_line_cell_index(cell_index);
                        if target_cell_index < next_cell_index {
                            return node_byte_start + local_byte_start;
                        }
                        cell_index = next_cell_index;
                    } else {
                        cell_index += 1;
                    }
                }

                node_byte_start + bytes
            }
            Self::Branch { left, right, .. } => {
                let right_cell_start = node_cell_start + left.cell_advance(node_cell_start % ROWS);
                if target_cell_index < right_cell_start {
                    left.display_cell_to_byte(target_cell_index, node_byte_start, node_cell_start)
                } else {
                    right.display_cell_to_byte(
                        target_cell_index,
                        node_byte_start + left.bytes(),
                        right_cell_start,
                    )
                }
            }
        }
    }

    #[cfg(test)]
    fn assert_balanced(&self) -> usize {
        match self {
            Self::Leaf {
                text,
                bytes,
                utf16,
                graphemes,
                cell_advances,
                height,
            } => {
                let text = text.as_str();
                assert_eq!(*bytes, text.len());
                assert_eq!(*utf16, text.encode_utf16().count());
                assert_eq!(*graphemes, text.graphemes(true).count());
                assert_eq!(*cell_advances, cell_advances_for_text(text, *graphemes));
                assert_eq!(*height, 1);
                1
            }
            Self::Branch {
                left,
                right,
                bytes,
                utf16,
                graphemes,
                cell_advances,
                height,
            } => {
                let left_height = left.assert_balanced();
                let right_height = right.assert_balanced();
                assert!(
                    left_height.abs_diff(right_height) <= 1,
                    "rope branch is unbalanced: left height {left_height}, right height {right_height}"
                );
                assert_eq!(*bytes, left.bytes() + right.bytes());
                assert_eq!(*utf16, left.utf16() + right.utf16());
                assert_eq!(*graphemes, left.graphemes() + right.graphemes());
                assert_eq!(*cell_advances, compose_cell_advances(left, right));
                assert_eq!(*height, left_height.max(right_height) + 1);
                *height
            }
        }
    }

    #[cfg(test)]
    fn shared_leaf_count(&self) -> usize {
        match self {
            Self::Leaf {
                text: RopeLeafText::Shared { .. },
                ..
            } => 1,
            Self::Leaf { .. } => 0,
            Self::Branch { left, right, .. } => {
                left.shared_leaf_count() + right.shared_leaf_count()
            }
        }
    }
}

fn split_node(
    node: Option<Box<RopeNode>>,
    byte_offset: usize,
) -> (Option<Box<RopeNode>>, Option<Box<RopeNode>>) {
    let Some(node) = node else {
        return (None, None);
    };

    if byte_offset == 0 {
        return (None, Some(node));
    }

    if byte_offset >= node.bytes() {
        return (Some(node), None);
    }

    match *node {
        RopeNode::Leaf { text, .. } => {
            debug_assert!(text.as_str().is_char_boundary(byte_offset));
            text.split_at(byte_offset)
        }
        RopeNode::Branch { left, right, .. } => {
            let left_len = left.bytes();
            if byte_offset < left_len {
                let (left_left, left_right) = split_node(Some(left), byte_offset);
                (left_left, concat_nodes(left_right, Some(right)))
            } else if byte_offset == left_len {
                (Some(left), Some(right))
            } else {
                let (right_left, right_right) = split_node(Some(right), byte_offset - left_len);
                (concat_nodes(Some(left), right_left), right_right)
            }
        }
    }
}

fn concat_nodes(
    left: Option<Box<RopeNode>>,
    right: Option<Box<RopeNode>>,
) -> Option<Box<RopeNode>> {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(left), Some(right)) => Some(concat_non_empty(left, right)),
    }
}

fn concat_non_empty(left: Box<RopeNode>, right: Box<RopeNode>) -> Box<RopeNode> {
    if left.bytes() + right.bytes() <= ROPE_LEAF_BYTES {
        let mut text = String::with_capacity(left.bytes() + right.bytes());
        left.push_to_string(&mut text);
        right.push_to_string(&mut text);
        return RopeNode::leaf(text);
    }

    if left.height() > right.height() + 1 {
        match *left {
            RopeNode::Branch {
                left: left_left,
                right: left_right,
                ..
            } => {
                return balance_branch(left_left, concat_non_empty(left_right, right));
            }
            leaf => return branch_node(Box::new(leaf), right),
        }
    }

    if right.height() > left.height() + 1 {
        match *right {
            RopeNode::Branch {
                left: right_left,
                right: right_right,
                ..
            } => {
                return balance_branch(concat_non_empty(left, right_left), right_right);
            }
            leaf => return branch_node(left, Box::new(leaf)),
        }
    }

    branch_node(left, right)
}

fn balance_branch(left: Box<RopeNode>, right: Box<RopeNode>) -> Box<RopeNode> {
    if left.height() > right.height() + 1 {
        return match *left {
            RopeNode::Branch {
                left: left_left,
                right: left_right,
                ..
            } => {
                if left_left.height() >= left_right.height() {
                    branch_node(left_left, branch_node(left_right, right))
                } else {
                    match *left_right {
                        RopeNode::Branch {
                            left: left_right_left,
                            right: left_right_right,
                            ..
                        } => branch_node(
                            branch_node(left_left, left_right_left),
                            branch_node(left_right_right, right),
                        ),
                        leaf => branch_node(left_left, branch_node(Box::new(leaf), right)),
                    }
                }
            }
            leaf => branch_node(Box::new(leaf), right),
        };
    }

    if right.height() > left.height() + 1 {
        return match *right {
            RopeNode::Branch {
                left: right_left,
                right: right_right,
                ..
            } => {
                if right_right.height() >= right_left.height() {
                    branch_node(branch_node(left, right_left), right_right)
                } else {
                    match *right_left {
                        RopeNode::Branch {
                            left: right_left_left,
                            right: right_left_right,
                            ..
                        } => branch_node(
                            branch_node(left, right_left_left),
                            branch_node(right_left_right, right_right),
                        ),
                        leaf => branch_node(branch_node(left, Box::new(leaf)), right_right),
                    }
                }
            }
            leaf => branch_node(left, Box::new(leaf)),
        };
    }

    branch_node(left, right)
}

fn branch_node(left: Box<RopeNode>, right: Box<RopeNode>) -> Box<RopeNode> {
    let bytes = left.bytes() + right.bytes();
    let utf16 = left.utf16() + right.utf16();
    let graphemes = left.graphemes() + right.graphemes();
    let cell_advances = compose_cell_advances(&left, &right);
    let height = left.height().max(right.height()) + 1;

    Box::new(RopeNode::Branch {
        left,
        right,
        bytes,
        utf16,
        graphemes,
        cell_advances,
        height,
    })
}

fn next_line_cell_index(cell_index: usize) -> usize {
    ((cell_index / ROWS) + 1) * ROWS
}

fn cell_advances_for_text(text: &str, graphemes: usize) -> [usize; ROWS] {
    if !text.as_bytes().contains(&b'\n') {
        return [graphemes; ROWS];
    }

    let mut advances = [0; ROWS];

    for start_row in 0..ROWS {
        let mut cell_index = start_row;
        for grapheme in text.graphemes(true) {
            if grapheme == "\n" {
                cell_index = next_line_cell_index(cell_index);
            } else {
                cell_index += 1;
            }
        }
        advances[start_row] = cell_index - start_row;
    }

    advances
}

fn compose_cell_advances(left: &RopeNode, right: &RopeNode) -> [usize; ROWS] {
    let mut advances = [0; ROWS];

    for start_row in 0..ROWS {
        let left_advance = left.cell_advance(start_row);
        let right_advance = right.cell_advance((start_row + left_advance) % ROWS);
        advances[start_row] = left_advance + right_advance;
    }

    advances
}

fn build_balanced(leaves: Vec<String>) -> Option<Box<RopeNode>> {
    build_balanced_nodes(leaves.into_iter().map(RopeNode::leaf).collect())
}

fn build_balanced_nodes(mut nodes: Vec<Box<RopeNode>>) -> Option<Box<RopeNode>> {
    while nodes.len() > 1 {
        let mut next_nodes = Vec::with_capacity(nodes.len().div_ceil(2));
        let mut iter = nodes.into_iter();

        while let Some(left) = iter.next() {
            if let Some(right) = iter.next() {
                next_nodes.push(concat_non_empty(left, right));
            } else {
                next_nodes.push(left);
            }
        }

        nodes = next_nodes;
    }

    nodes.pop()
}

fn chunk_string(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for grapheme in text.graphemes(true) {
        if !current.is_empty() && current.len() + grapheme.len() > ROPE_LEAF_BYTES {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(grapheme);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn chunk_shared_string(source: Arc<String>) -> Vec<Box<RopeNode>> {
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    let mut chunk_bytes = 0;

    for (byte_offset, grapheme) in source.grapheme_indices(true) {
        if chunk_bytes > 0 && chunk_bytes + grapheme.len() > ROPE_LEAF_BYTES {
            if let Some(chunk) = RopeNode::shared_leaf(source.clone(), chunk_start..byte_offset) {
                chunks.push(chunk);
            }
            chunk_start = byte_offset;
            chunk_bytes = 0;
        }

        chunk_bytes += grapheme.len();
    }

    if chunk_start < source.len() {
        if let Some(chunk) = RopeNode::shared_leaf(source.clone(), chunk_start..source.len()) {
            chunks.push(chunk);
        }
    }

    chunks
}

fn byte_to_utf16_in_str(text: &str, byte_offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;

    for character in text.chars() {
        if utf8_count >= byte_offset {
            break;
        }
        utf8_count += character.len_utf8();
        utf16_offset += character.len_utf16();
    }

    utf16_offset
}

fn utf16_to_byte_in_str(text: &str, utf16_offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for character in text.chars() {
        if utf16_count >= utf16_offset {
            break;
        }
        utf16_count += character.len_utf16();
        utf8_offset += character.len_utf8();
    }

    utf8_offset
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn row_column_for_logical_index(
    logical_index: usize,
    first_visible_column: usize,
) -> Option<(usize, usize)> {
    let logical_column = logical_index / ROWS;
    if logical_column < first_visible_column {
        return None;
    }

    let column_from_right = logical_column - first_visible_column;
    if column_from_right >= COLUMNS {
        return None;
    }

    let row = logical_index % ROWS;
    let column = COLUMNS - 1 - column_from_right;
    Some((row, column))
}

fn cell_bounds_for_logical_index(
    board_bounds: Bounds<Pixels>,
    logical_index: usize,
    first_visible_column: usize,
) -> Option<Bounds<Pixels>> {
    let (row, column) = row_column_for_logical_index(logical_index, first_visible_column)?;
    Some(Bounds::new(
        point(
            board_bounds.left() + px(column as f32 * CELL_SIZE),
            board_bounds.top() + px(row as f32 * CELL_SIZE),
        ),
        size(px(CELL_SIZE), px(CELL_SIZE)),
    ))
}

fn logical_index_for_point(
    board_bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
    first_visible_column: usize,
) -> Option<usize> {
    if !board_bounds.contains(&position) {
        return None;
    }

    let column = ((position.x - board_bounds.left()) / px(CELL_SIZE))
        .floor()
        .clamp(0.0, (COLUMNS - 1) as f32) as usize;
    let row = ((position.y - board_bounds.top()) / px(CELL_SIZE))
        .floor()
        .clamp(0.0, (ROWS - 1) as f32) as usize;
    let column_from_right = COLUMNS - 1 - column;
    Some((first_visible_column + column_from_right) * ROWS + row)
}

fn main() {
    Application::new().run(|cx: &mut App| {
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
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(760.0), px(760.0)), cx);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    app_id: Some("dev.genko".into()),
                    ..Default::default()
                },
                |_, cx| cx.new(GenkoApp::new),
            )
            .unwrap();

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
            })
            .unwrap();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_flow_starts_at_top_right() {
        assert_eq!(row_column_for_logical_index(0, 0), Some((0, COLUMNS - 1)));
        assert_eq!(row_column_for_logical_index(1, 0), Some((1, COLUMNS - 1)));
        assert_eq!(
            row_column_for_logical_index(ROWS, 0),
            Some((0, COLUMNS - 2))
        );
    }

    #[test]
    fn virtual_scroll_offsets_visible_columns() {
        assert_eq!(row_column_for_logical_index(0, 1), None);
        assert_eq!(
            row_column_for_logical_index(ROWS, 1),
            Some((0, COLUMNS - 1))
        );
        assert_eq!(
            row_column_for_logical_index(ROWS * COLUMNS, 1),
            Some((0, 0))
        );
        assert_eq!(row_column_for_logical_index(ROWS * (COLUMNS + 1), 1), None);
    }

    #[test]
    fn rope_replaces_japanese_text_on_char_boundaries() {
        let mut rope = TextRope::from_str("abc");
        rope.replace_range(1..2, "日本語");
        assert_eq!(rope.to_string(), "a日本語c");
        rope.replace_range(1.."日本語".len() + 1, "文");
        assert_eq!(rope.to_string(), "a文c");
    }

    #[test]
    fn rope_converts_utf16_offsets() {
        let rope = TextRope::from_str("a😀文");
        assert_eq!(rope.utf16_to_byte(0), 0);
        assert_eq!(rope.utf16_to_byte(1), 1);
        assert_eq!(rope.utf16_to_byte(3), "a😀".len());
        assert_eq!(rope.byte_to_utf16("a😀".len()), 3);
    }

    #[test]
    fn rope_returns_only_visible_graphemes() {
        let rope = TextRope::from_str("一二三四五");
        let visible = rope.visible_graphemes(2, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].logical_index, 2);
        assert_eq!(visible[0].text, "三");
        assert_eq!(visible[1].logical_index, 3);
        assert_eq!(visible[1].text, "四");
    }

    #[test]
    fn rope_newline_advances_to_next_vertical_column() {
        let rope = TextRope::from_str("あ\nい");
        let visible = rope.visible_cells(0, ROWS + 1);

        assert_eq!(rope.len_graphemes(), 3);
        assert_eq!(rope.len_display_cells(), ROWS + 1);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].logical_index, 0);
        assert_eq!(visible[0].text, "あ");
        assert_eq!(visible[1].logical_index, ROWS);
        assert_eq!(visible[1].text, "い");
    }

    #[test]
    fn rope_maps_newline_gap_to_line_break_byte_offset() {
        let rope = TextRope::from_str("あ\nい");
        let newline_start = "あ".len();
        let after_newline = "あ\n".len();

        assert_eq!(rope.display_cell_for_byte(newline_start), 1);
        assert_eq!(rope.display_cell_for_byte(after_newline), ROWS);
        assert_eq!(rope.byte_offset_for_display_cell(1), newline_start);
        assert_eq!(rope.byte_offset_for_display_cell(ROWS - 1), newline_start);
        assert_eq!(rope.byte_offset_for_display_cell(ROWS), after_newline);
    }

    #[test]
    fn rope_keeps_right_edge_appends_balanced() {
        let mut rope = TextRope::new();

        for _ in 0..20_000 {
            let end = rope.len_bytes();
            rope.replace_range(end..end, "文");
        }

        assert_eq!(rope.len_graphemes(), 20_000);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(19_998, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].logical_index, 19_998);
        assert_eq!(visible[1].logical_index, 19_999);
    }

    #[test]
    fn rope_keeps_middle_edits_balanced() {
        let mut rope = TextRope::from_str(&"a".repeat(20_000));

        for _ in 0..500 {
            let midpoint = rope.byte_offset_for_grapheme_index(rope.len_graphemes() / 2);
            rope.replace_range(midpoint..midpoint, "文");
        }

        assert_eq!(rope.len_graphemes(), 20_500);
        assert!(rope.height() <= 32);
        rope.assert_balanced();
    }

    #[test]
    fn rope_uses_shared_chunks_for_owned_large_text() {
        let rope = TextRope::from_string("文".repeat(20_000));

        assert_eq!(rope.len_graphemes(), 20_000);
        assert!(rope.shared_leaf_count() > 0);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(19_998, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].text, "文");
        assert_eq!(visible[1].text, "文");
    }

    #[test]
    fn rope_pastes_owned_large_text_into_existing_document() {
        let mut rope = TextRope::from_str("開始終了");
        let insert_at = "開始".len();

        rope.replace_range_owned(insert_at..insert_at, "文".repeat(20_000));

        assert_eq!(rope.len_graphemes(), 20_004);
        assert!(rope.shared_leaf_count() > 0);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(0, 3);
        assert_eq!(visible[0].text, "開");
        assert_eq!(visible[1].text, "始");
        assert_eq!(visible[2].text, "文");
    }
}
