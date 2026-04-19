use std::ops::Range;

mod board;
mod settings;
mod settings_window;

use board::{BoardElement, cell_bounds_for_logical_index, logical_index_for_point};
use genko_rope::{CellText, TextRope, utf16_to_byte_in_text};
use settings::AppSettings;
use settings_window::SettingsWindow;

use gpui::{
    App, Application, Bounds, ClipboardItem, Context, CursorStyle, Entity, EntityInputHandler,
    FocusHandle, Focusable, KeyBinding, Menu, MenuItem, MouseButton, MouseDownEvent, ParentElement,
    Pixels, Render, ScrollWheelEvent, SharedString, Styled, UTF16Selection, Window, WindowBounds,
    WindowOptions, actions, div, prelude::*, px, rgb, size,
};

const DEFAULT_VISIBLE_COLUMNS: usize = 20;
const CELL_SIZE: f32 = 28.0;

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
        OpenSettings,
        Quit,
    ]
);

pub(crate) struct GenkoApp {
    title: SharedString,
    draft: TextRope,
    pub(crate) settings: AppSettings,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) selected_range: Range<usize>,
    selection_reversed: bool,
    pub(crate) cursor_cell: usize,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) last_board_bounds: Option<Bounds<Pixels>>,
    pub(crate) scroll_column: usize,
    scroll_remainder_columns: f32,
    visible_columns: usize,
}

impl GenkoApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let settings = AppSettings::load();
        let draft = TextRope::new_with_rows(settings.rows_per_column);
        Self {
            title: "Genko".into(),
            draft,
            settings,
            focus_handle: cx.focus_handle(),
            selected_range: 0..0,
            selection_reversed: false,
            cursor_cell: 0,
            marked_range: None,
            last_board_bounds: None,
            scroll_column: 0,
            scroll_remainder_columns: 0.0,
            visible_columns: DEFAULT_VISIBLE_COLUMNS,
        }
    }

    pub(crate) fn apply_settings(&mut self, settings: AppSettings, cx: &mut Context<Self>) {
        self.settings = settings.normalized();
        self.draft
            .set_rows_per_column(self.settings.rows_per_column);
        self.cursor_cell = self.display_cell_for_byte(self.cursor_offset());
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to_display_cell(&mut self, cell_index: usize, cx: &mut Context<Self>) {
        let offset = self.draft.byte_offset_for_display_cell(cell_index);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.cursor_cell = cell_index;
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

    pub(crate) fn visible_text(&self) -> Vec<CellText> {
        let first_visible_index = self.first_visible_cell_index();
        self.draft
            .visible_cells(first_visible_index, self.visible_cell_capacity())
    }

    fn used_cells(&self) -> usize {
        self.draft.len_display_cells()
    }

    pub(crate) fn rows_per_column(&self) -> usize {
        self.settings.rows_per_column
    }

    pub(crate) fn visible_columns(&self) -> usize {
        self.visible_columns
    }

    fn update_visible_columns(&mut self, visible_columns: usize) {
        self.visible_columns = visible_columns.max(1);
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

    fn total_columns(&self) -> usize {
        let document_columns = self.used_cells().div_ceil(self.rows_per_column()).max(1);
        document_columns.max(self.cursor_column() + 1)
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

    fn delete(&mut self, _: &Delete, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            self.selected_range = self.cursor_offset()..next;
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
    }

    fn up(&mut self, _: &Up, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_cell_delta(-1, cx);
    }

    fn down(&mut self, _: &Down, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_cell_delta(1, cx);
    }

    fn left(&mut self, _: &Left, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_cell_delta(self.rows_per_column() as isize, cx);
    }

    fn right(&mut self, _: &Right, _window: &mut Window, cx: &mut Context<Self>) {
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

    fn on_board_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
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
                (self.scroll_column + self.visible_columns()).min(self.total_columns()),
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

impl Render for GenkoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_visible_columns(visible_columns_for_window_width(
            window.viewport_size().width,
        ));

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

fn visible_columns_for_window_width(width: Pixels) -> usize {
    ((width / px(CELL_SIZE)).floor() as usize)
        .saturating_sub(2)
        .max(1)
}

impl Focusable for GenkoApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn open_settings_window(app: Entity<GenkoApp>, cx: &mut App) {
    let settings = app.read(cx).settings.clone();
    let bounds = Bounds::centered(None, size(px(520.0), px(390.0)), cx);

    let settings_window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("dev.genko.settings".into()),
                ..Default::default()
            },
            move |_, cx| cx.new(move |_| SettingsWindow::new(app, settings)),
        )
        .unwrap();

    settings_window
        .update(cx, |_, window, cx| {
            window.activate_window();
            cx.activate(true);
        })
        .unwrap();
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

        let app_entity = window.entity(cx).unwrap().downgrade();
        cx.on_action(move |_: &OpenSettings, cx| {
            if let Some(app_entity) = app_entity.upgrade() {
                open_settings_window(app_entity, cx);
            }
        });
        cx.set_menus(vec![Menu {
            name: "Genko".into(),
            items: vec![
                MenuItem::action("設定", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("終了", Quit),
            ],
        }]);

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
            })
            .unwrap();
    });
}
