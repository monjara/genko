use std::ops::Range;

use gpui::{
    App, Application, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent, ParentElement, Pixels, Render,
    SharedString, Style, Styled, TextRun, UTF16Selection, Window, WindowBounds, WindowOptions,
    actions, div, fill, point, prelude::*, px, rgb, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

const ROWS: usize = 20;
const COLUMNS: usize = 20;
const CELL_SIZE: f32 = 28.0;
const CELL_CAPACITY: usize = ROWS * COLUMNS;
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
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.draft
            .to_string()
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.draft
            .to_string()
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.draft.len_bytes())
    }

    fn byte_offset_for_grapheme_index(&self, target_index: usize) -> usize {
        self.draft
            .to_string()
            .grapheme_indices(true)
            .nth(target_index)
            .map(|(offset, _)| offset)
            .unwrap_or(self.draft.len_bytes())
    }

    fn grapheme_index_for_byte(&self, byte_offset: usize) -> usize {
        self.draft
            .to_string()
            .grapheme_indices(true)
            .take_while(|(offset, _)| *offset < byte_offset)
            .count()
    }

    fn visible_text(&self) -> Vec<CellText> {
        self.draft
            .to_string()
            .grapheme_indices(true)
            .take(CELL_CAPACITY)
            .map(|(start, grapheme)| CellText {
                text: grapheme.to_string(),
                range: start..start + grapheme.len(),
            })
            .collect()
    }

    fn used_cells(&self) -> usize {
        self.draft.to_string().graphemes(true).count()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.draft.byte_to_utf16(range.start)..self.draft.byte_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.draft.utf16_to_byte(range_utf16.start)..self.draft.utf16_to_byte(range_utf16.end)
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
            self.replace_text_in_byte_range(self.selected_range.clone(), &text, cx);
        }
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

    fn byte_offset_for_point(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        let bounds = self.last_board_bounds?;
        let index = logical_index_for_point(bounds, position)?;
        Some(self.byte_offset_for_grapheme_index(index.min(self.used_cells())))
    }

    fn bounds_for_byte_range(
        &self,
        range: Range<usize>,
        board_bounds: Bounds<Pixels>,
    ) -> Bounds<Pixels> {
        let logical_index = self.grapheme_index_for_byte(range.start);
        cell_bounds_for_logical_index(board_bounds, logical_index.min(CELL_CAPACITY - 1))
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
                "vertical / {} of {} cells",
                self.used_cells(),
                CELL_CAPACITY
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
        Some(self.bounds_for_byte_range(range, board_bounds))
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
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_board_mouse_down))
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

        let (visible_text, selected_range, marked_range, cursor_index) = {
            let app = self.app.read(cx);
            (
                app.visible_text(),
                app.selected_range.clone(),
                app.marked_range.clone(),
                app.grapheme_index_for_byte(app.cursor_offset()),
            )
        };

        self.paint_selection(
            &visible_text,
            &selected_range,
            marked_range.as_ref(),
            bounds,
            window,
        );
        self.paint_grid(bounds, window);
        self.paint_text(&visible_text, bounds, window, cx);
        if focus_handle.is_focused(window) {
            self.paint_cursor(cursor_index, bounds, window);
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
        window: &mut Window,
    ) {
        for (logical_index, cell_text) in visible_text.iter().enumerate() {
            if ranges_overlap(&cell_text.range, selected_range) {
                let cell_bounds = cell_bounds_for_logical_index(bounds, logical_index);
                window.paint_quad(fill(cell_bounds, rgba(0x2f6fff30)));
            } else if marked_range.is_some_and(|range| ranges_overlap(&cell_text.range, range)) {
                let cell_bounds = cell_bounds_for_logical_index(bounds, logical_index);
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
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = window.text_style();
        let font_size = px(21.0);
        let line_height = px(24.0);

        for (logical_index, cell_text) in visible_text.iter().enumerate() {
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
            let cell_bounds = cell_bounds_for_logical_index(bounds, logical_index);
            let text_origin = point(
                cell_bounds.left() + (px(CELL_SIZE) - line.width) / 2.0,
                cell_bounds.top() + (px(CELL_SIZE) - line_height) / 2.0,
            );
            line.paint(text_origin, line_height, window, cx).ok();
        }
    }

    fn paint_cursor(&self, cursor_index: usize, bounds: Bounds<Pixels>, window: &mut Window) {
        let logical_index = cursor_index.min(CELL_CAPACITY - 1);
        let cell_bounds = cell_bounds_for_logical_index(bounds, logical_index);
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

    fn from_str(text: &str) -> Self {
        Self {
            root: RopeNode::from_str(text),
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

    fn replace_range(&mut self, range: Range<usize>, text: &str) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len_bytes());

        let root = self.root.take();
        let (left, rest) = split_node(root, range.start);
        let (_, right) = split_node(rest, range.end - range.start);
        self.root = concat_nodes(concat_nodes(left, RopeNode::from_str(text)), right);
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
}

#[derive(Clone, Debug)]
enum RopeNode {
    Leaf {
        text: String,
        bytes: usize,
        utf16: usize,
        height: usize,
    },
    Branch {
        left: Box<RopeNode>,
        right: Box<RopeNode>,
        bytes: usize,
        utf16: usize,
        height: usize,
    },
}

impl RopeNode {
    fn from_str(text: &str) -> Option<Box<Self>> {
        if text.is_empty() {
            return None;
        }

        let chunks = chunk_string(text);
        build_balanced(chunks)
    }

    fn leaf(text: String) -> Box<Self> {
        let bytes = text.len();
        let utf16 = text.encode_utf16().count();
        Box::new(Self::Leaf {
            text,
            bytes,
            utf16,
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

    fn height(&self) -> usize {
        match self {
            Self::Leaf { height, .. } | Self::Branch { height, .. } => *height,
        }
    }

    fn push_to_string(&self, output: &mut String) {
        match self {
            Self::Leaf { text, .. } => output.push_str(text),
            Self::Branch { left, right, .. } => {
                left.push_to_string(output);
                right.push_to_string(output);
            }
        }
    }

    fn push_leaves(&self, leaves: &mut Vec<String>) {
        match self {
            Self::Leaf { text, .. } => leaves.push(text.clone()),
            Self::Branch { left, right, .. } => {
                left.push_leaves(leaves);
                right.push_leaves(leaves);
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
                output.push_str(&text[local_start..local_end]);
            }
            Self::Branch { left, right, .. } => {
                left.push_range(range.clone(), node_start, output);
                right.push_range(range, node_start + left.bytes(), output);
            }
        }
    }

    fn byte_to_utf16(&self, byte_offset: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => byte_to_utf16_in_str(text, byte_offset.min(*bytes)),
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
            Self::Leaf { text, utf16, .. } => utf16_to_byte_in_str(text, utf16_offset.min(*utf16)),
            Self::Branch { left, right, .. } => {
                if utf16_offset <= left.utf16() {
                    left.utf16_to_byte(utf16_offset)
                } else {
                    left.bytes() + right.utf16_to_byte(utf16_offset - left.utf16())
                }
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

    match *node {
        RopeNode::Leaf { text, .. } => {
            if byte_offset == 0 {
                (None, RopeNode::from_str(&text))
            } else if byte_offset >= text.len() {
                (RopeNode::from_str(&text), None)
            } else {
                debug_assert!(text.is_char_boundary(byte_offset));
                let left = RopeNode::from_str(&text[..byte_offset]);
                let right = RopeNode::from_str(&text[byte_offset..]);
                (left, right)
            }
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
        (Some(left), Some(right)) => {
            if left.bytes() + right.bytes() <= ROPE_LEAF_BYTES {
                let mut text = String::with_capacity(left.bytes() + right.bytes());
                left.push_to_string(&mut text);
                right.push_to_string(&mut text);
                return Some(RopeNode::leaf(text));
            }

            let height_gap = left.height().abs_diff(right.height());
            if height_gap > 1 {
                let mut leaves = Vec::new();
                left.push_leaves(&mut leaves);
                right.push_leaves(&mut leaves);
                return build_balanced(leaves);
            }

            let bytes = left.bytes() + right.bytes();
            let utf16 = left.utf16() + right.utf16();
            let height = left.height().max(right.height()) + 1;
            Some(Box::new(RopeNode::Branch {
                left,
                right,
                bytes,
                utf16,
                height,
            }))
        }
    }
}

fn build_balanced(mut leaves: Vec<String>) -> Option<Box<RopeNode>> {
    if leaves.is_empty() {
        return None;
    }
    build_balanced_slice(&mut leaves)
}

fn build_balanced_slice(leaves: &mut [String]) -> Option<Box<RopeNode>> {
    match leaves.len() {
        0 => None,
        1 => Some(RopeNode::leaf(std::mem::take(&mut leaves[0]))),
        len => {
            let midpoint = len / 2;
            let left = build_balanced_slice(&mut leaves[..midpoint]);
            let right = build_balanced_slice(&mut leaves[midpoint..]);
            concat_nodes(left, right)
        }
    }
}

fn chunk_string(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + ROPE_LEAF_BYTES).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = text[start..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| start + offset)
                .unwrap_or(text.len());
        }
        chunks.push(text[start..end].to_string());
        start = end;
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

fn row_column_for_logical_index(logical_index: usize) -> Option<(usize, usize)> {
    if logical_index >= CELL_CAPACITY {
        return None;
    }

    let column_from_right = logical_index / ROWS;
    let row = logical_index % ROWS;
    let column = COLUMNS - 1 - column_from_right;
    Some((row, column))
}

fn cell_bounds_for_logical_index(
    board_bounds: Bounds<Pixels>,
    logical_index: usize,
) -> Bounds<Pixels> {
    let (row, column) = row_column_for_logical_index(logical_index).unwrap_or((ROWS - 1, 0));
    Bounds::new(
        point(
            board_bounds.left() + px(column as f32 * CELL_SIZE),
            board_bounds.top() + px(row as f32 * CELL_SIZE),
        ),
        size(px(CELL_SIZE), px(CELL_SIZE)),
    )
}

fn logical_index_for_point(
    board_bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
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
    Some(column_from_right * ROWS + row)
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
        assert_eq!(row_column_for_logical_index(0), Some((0, COLUMNS - 1)));
        assert_eq!(row_column_for_logical_index(1), Some((1, COLUMNS - 1)));
        assert_eq!(row_column_for_logical_index(ROWS), Some((0, COLUMNS - 2)));
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
}
