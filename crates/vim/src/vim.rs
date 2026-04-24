use std::ops::Range;
use std::sync::OnceLock;

use editor::Editor;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, InteractiveElement, KeyBinding, ParentElement,
    Render, Window, actions, div,
};
use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer;

actions!(
    vim,
    [
        VimEnterInsertMode,
        VimAppend,
        VimNormalMode,
        VimVisualMode,
        VimDeleteChar,
        VimDeleteOperator,
        VimChangeOperator,
        VimTextObjectInner,
        VimTextObjectAround,
        VimTextObjectWord,
        VimTextObjectBigWord,
        VimTextObjectDoubleQuote,
        VimTextObjectSingleQuote,
        VimTextObjectParen,
        VimTextObjectBracket,
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VimOperator {
    Delete,
    Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextObjectModifier {
    Inner,
    Around,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextObjectTarget {
    Word,
    BigWord,
    DoubleQuote,
    SingleQuote,
    Paren,
    Bracket,
}

static JAPANESE_TOKENIZER: OnceLock<Result<Tokenizer, String>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VimState {
    mode: VimMode,
    visual_anchor_cell: Option<usize>,
    pending_operator: Option<VimOperator>,
    pending_text_object_modifier: Option<TextObjectModifier>,
}

impl VimState {
    pub fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            visual_anchor_cell: None,
            pending_operator: None,
            pending_text_object_modifier: None,
        }
    }

    pub fn mode(&self) -> VimMode {
        self.mode
    }

    fn set_mode(&mut self, mode: VimMode) {
        self.mode = mode;
    }

    fn visual_anchor_cell(&self) -> Option<usize> {
        self.visual_anchor_cell
    }

    fn set_visual_anchor_cell(&mut self, anchor: Option<usize>) {
        self.visual_anchor_cell = anchor;
    }

    fn set_pending_operator(&mut self, operator: Option<VimOperator>) {
        self.pending_operator = operator;
    }

    fn pending_operator(&self) -> Option<VimOperator> {
        self.pending_operator
    }

    fn set_pending_text_object_modifier(&mut self, modifier: Option<TextObjectModifier>) {
        self.pending_text_object_modifier = modifier;
    }

    fn pending_text_object_modifier(&self) -> Option<TextObjectModifier> {
        self.pending_text_object_modifier
    }

    fn clear_pending(&mut self) {
        self.pending_operator = None;
        self.pending_text_object_modifier = None;
    }

    fn key_context(&self) -> &'static str {
        match (
            self.mode,
            self.pending_operator,
            self.pending_text_object_modifier,
        ) {
            (VimMode::Insert, _, _) => "Genko vim_mode=insert",
            (VimMode::Visual, _, _) => "Genko vim_mode=visual",
            (VimMode::Normal, None, _) => "Genko vim_mode=normal",
            (VimMode::Normal, Some(VimOperator::Delete), None) => "Genko vim_mode=operator_delete",
            (VimMode::Normal, Some(VimOperator::Change), None) => "Genko vim_mode=operator_change",
            (VimMode::Normal, Some(VimOperator::Delete), Some(TextObjectModifier::Inner)) => {
                "Genko vim_mode=operator_delete_inner"
            }
            (VimMode::Normal, Some(VimOperator::Delete), Some(TextObjectModifier::Around)) => {
                "Genko vim_mode=operator_delete_around"
            }
            (VimMode::Normal, Some(VimOperator::Change), Some(TextObjectModifier::Inner)) => {
                "Genko vim_mode=operator_change_inner"
            }
            (VimMode::Normal, Some(VimOperator::Change), Some(TextObjectModifier::Around)) => {
                "Genko vim_mode=operator_change_around"
            }
        }
    }
}

pub struct Vim {
    editor: Entity<Editor>,
    state: VimState,
}

impl Vim {
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("i", VimEnterInsertMode, Some("vim_mode == normal")),
            KeyBinding::new("a", VimAppend, Some("vim_mode == normal")),
            KeyBinding::new("escape", VimNormalMode, Some("vim_mode == insert")),
            KeyBinding::new("escape", VimNormalMode, Some("vim_mode == visual")),
            KeyBinding::new("escape", VimNormalMode, Some("vim_mode == operator_delete")),
            KeyBinding::new("escape", VimNormalMode, Some("vim_mode == operator_change")),
            KeyBinding::new(
                "escape",
                VimNormalMode,
                Some("vim_mode == operator_delete_inner"),
            ),
            KeyBinding::new(
                "escape",
                VimNormalMode,
                Some("vim_mode == operator_delete_around"),
            ),
            KeyBinding::new(
                "escape",
                VimNormalMode,
                Some("vim_mode == operator_change_inner"),
            ),
            KeyBinding::new(
                "escape",
                VimNormalMode,
                Some("vim_mode == operator_change_around"),
            ),
            KeyBinding::new("v", VimVisualMode, Some("vim_mode == normal")),
            KeyBinding::new("v", VimNormalMode, Some("vim_mode == visual")),
            KeyBinding::new("d", VimDeleteOperator, Some("vim_mode == normal")),
            KeyBinding::new("c", VimChangeOperator, Some("vim_mode == normal")),
            KeyBinding::new("i", VimTextObjectInner, Some("vim_mode == operator_delete")),
            KeyBinding::new(
                "a",
                VimTextObjectAround,
                Some("vim_mode == operator_delete"),
            ),
            KeyBinding::new("i", VimTextObjectInner, Some("vim_mode == operator_change")),
            KeyBinding::new(
                "a",
                VimTextObjectAround,
                Some("vim_mode == operator_change"),
            ),
            KeyBinding::new(
                "w",
                VimTextObjectWord,
                Some("vim_mode == operator_delete_inner"),
            ),
            KeyBinding::new(
                "w",
                VimTextObjectWord,
                Some("vim_mode == operator_delete_around"),
            ),
            KeyBinding::new(
                "w",
                VimTextObjectWord,
                Some("vim_mode == operator_change_inner"),
            ),
            KeyBinding::new(
                "w",
                VimTextObjectWord,
                Some("vim_mode == operator_change_around"),
            ),
            KeyBinding::new(
                "W",
                VimTextObjectBigWord,
                Some("vim_mode == operator_delete_inner"),
            ),
            KeyBinding::new(
                "W",
                VimTextObjectBigWord,
                Some("vim_mode == operator_delete_around"),
            ),
            KeyBinding::new(
                "W",
                VimTextObjectBigWord,
                Some("vim_mode == operator_change_inner"),
            ),
            KeyBinding::new(
                "W",
                VimTextObjectBigWord,
                Some("vim_mode == operator_change_around"),
            ),
            KeyBinding::new(
                "\"",
                VimTextObjectDoubleQuote,
                Some("vim_mode == operator_delete_inner"),
            ),
            KeyBinding::new(
                "\"",
                VimTextObjectDoubleQuote,
                Some("vim_mode == operator_delete_around"),
            ),
            KeyBinding::new(
                "\"",
                VimTextObjectDoubleQuote,
                Some("vim_mode == operator_change_inner"),
            ),
            KeyBinding::new(
                "\"",
                VimTextObjectDoubleQuote,
                Some("vim_mode == operator_change_around"),
            ),
            KeyBinding::new(
                "'",
                VimTextObjectSingleQuote,
                Some("vim_mode == operator_delete_inner"),
            ),
            KeyBinding::new(
                "'",
                VimTextObjectSingleQuote,
                Some("vim_mode == operator_delete_around"),
            ),
            KeyBinding::new(
                "'",
                VimTextObjectSingleQuote,
                Some("vim_mode == operator_change_inner"),
            ),
            KeyBinding::new(
                "'",
                VimTextObjectSingleQuote,
                Some("vim_mode == operator_change_around"),
            ),
            KeyBinding::new(
                "(",
                VimTextObjectParen,
                Some("vim_mode == operator_delete_inner"),
            ),
            KeyBinding::new(
                "(",
                VimTextObjectParen,
                Some("vim_mode == operator_delete_around"),
            ),
            KeyBinding::new(
                "(",
                VimTextObjectParen,
                Some("vim_mode == operator_change_inner"),
            ),
            KeyBinding::new(
                "(",
                VimTextObjectParen,
                Some("vim_mode == operator_change_around"),
            ),
            KeyBinding::new(
                "[",
                VimTextObjectBracket,
                Some("vim_mode == operator_delete_inner"),
            ),
            KeyBinding::new(
                "[",
                VimTextObjectBracket,
                Some("vim_mode == operator_delete_around"),
            ),
            KeyBinding::new(
                "[",
                VimTextObjectBracket,
                Some("vim_mode == operator_change_inner"),
            ),
            KeyBinding::new(
                "[",
                VimTextObjectBracket,
                Some("vim_mode == operator_change_around"),
            ),
            KeyBinding::new("h", editor::Left, Some("vim_mode == normal || vim_mode == visual")),
            KeyBinding::new("j", editor::Down, Some("vim_mode == normal || vim_mode == visual")),
            KeyBinding::new("k", editor::Up, Some("vim_mode == normal || vim_mode == visual")),
            KeyBinding::new("l", editor::Right, Some("vim_mode == normal || vim_mode == visual")),
            KeyBinding::new("x", VimDeleteChar, Some("vim_mode == normal || vim_mode == visual")),
        ]);
    }

    pub fn new(editor: Entity<Editor>) -> Self {
        Self {
            editor,
            state: VimState::new(),
        }
    }

    pub fn update_viewport_size(&mut self, size: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) {
        let text_input_enabled = self.state.mode() == VimMode::Insert;
        self.editor.update(cx, |editor, cx| {
            editor.update_viewport_size(size, cx);
            editor.set_text_input_enabled(text_input_enabled, cx);
        });
    }

    fn enter_insert_mode(&mut self, cx: &mut Context<Self>) {
        self.state.set_mode(VimMode::Insert);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(true, cx);
            editor.collapse_selection_to_cursor_offset(cx);
        });
        cx.notify();
    }

    fn append(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.move_cursor_by(1, cx);
        });
        self.enter_insert_mode(cx);
    }

    fn normal_mode(&mut self, cx: &mut Context<Self>) {
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn visual_mode(&mut self, cx: &mut Context<Self>) {
        let anchor = self.editor.read(cx).cursor_cell();
        self.state.set_mode(VimMode::Visual);
        self.state.set_visual_anchor_cell(Some(anchor));
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.select_visual_range(anchor, anchor, cx);
        });
        cx.notify();
    }

    fn begin_operator(&mut self, operator: VimOperator, cx: &mut Context<Self>) {
        self.state.set_mode(VimMode::Normal);
        self.state.set_pending_operator(Some(operator));
        self.state.set_pending_text_object_modifier(None);
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
        });
        cx.notify();
    }

    fn set_text_object_modifier(&mut self, modifier: TextObjectModifier, cx: &mut Context<Self>) {
        if self.state.pending_operator().is_none() {
            return;
        }
        self.state.set_pending_text_object_modifier(Some(modifier));
        cx.notify();
    }

    fn delete_char(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.delete_forward_command(cx);
        });
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn apply_text_object(&mut self, target: TextObjectTarget, cx: &mut Context<Self>) {
        let Some(operator) = self.state.pending_operator() else {
            return;
        };
        let Some(modifier) = self.state.pending_text_object_modifier() else {
            return;
        };

        let (text, cursor) = {
            let editor = self.editor.read(cx);
            (editor.snapshot_text(), editor.cursor_byte_offset())
        };

        let Some(range) = resolve_text_object_range(&text, cursor, modifier, target) else {
            self.state.clear_pending();
            cx.notify();
            return;
        };

        self.editor.update(cx, |editor, cx| {
            editor.replace_byte_range(range, "", cx);
            match operator {
                VimOperator::Delete => {
                    editor.set_text_input_enabled(false, cx);
                    editor.collapse_selection_to_cursor_cell(cx);
                }
                VimOperator::Change => {
                    editor.set_text_input_enabled(true, cx);
                    editor.collapse_selection_to_cursor_offset(cx);
                }
            }
        });

        self.state.clear_pending();
        self.state.set_visual_anchor_cell(None);
        self.state.set_mode(match operator {
            VimOperator::Delete => VimMode::Normal,
            VimOperator::Change => VimMode::Insert,
        });
        cx.notify();
    }

    fn sync_visual_selection_for_current_cursor(&mut self, cx: &mut Context<Self>) {
        let Some(anchor) = self.state.visual_anchor_cell() else {
            return;
        };
        let cursor = self.editor.read(cx).cursor_cell();
        self.editor.update(cx, |editor, cx| {
            editor.select_visual_range(anchor, cursor, cx);
        });
    }

    fn vim_enter_insert_mode(
        &mut self,
        _: &VimEnterInsertMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.enter_insert_mode(cx);
    }

    fn vim_append(&mut self, _: &VimAppend, _window: &mut Window, cx: &mut Context<Self>) {
        self.append(cx);
    }

    fn vim_normal_mode(&mut self, _: &VimNormalMode, _window: &mut Window, cx: &mut Context<Self>) {
        self.normal_mode(cx);
    }

    fn vim_visual_mode(&mut self, _: &VimVisualMode, _window: &mut Window, cx: &mut Context<Self>) {
        self.visual_mode(cx);
    }

    fn vim_delete_char(&mut self, _: &VimDeleteChar, _window: &mut Window, cx: &mut Context<Self>) {
        self.delete_char(cx);
    }

    fn vim_delete_operator(
        &mut self,
        _: &VimDeleteOperator,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_operator(VimOperator::Delete, cx);
    }

    fn vim_change_operator(
        &mut self,
        _: &VimChangeOperator,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_operator(VimOperator::Change, cx);
    }

    fn vim_text_object_inner(
        &mut self,
        _: &VimTextObjectInner,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_text_object_modifier(TextObjectModifier::Inner, cx);
    }

    fn vim_text_object_around(
        &mut self,
        _: &VimTextObjectAround,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_text_object_modifier(TextObjectModifier::Around, cx);
    }

    fn vim_text_object_word(
        &mut self,
        _: &VimTextObjectWord,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::Word, cx);
    }

    fn vim_text_object_big_word(
        &mut self,
        _: &VimTextObjectBigWord,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::BigWord, cx);
    }

    fn vim_text_object_double_quote(
        &mut self,
        _: &VimTextObjectDoubleQuote,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::DoubleQuote, cx);
    }

    fn vim_text_object_single_quote(
        &mut self,
        _: &VimTextObjectSingleQuote,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::SingleQuote, cx);
    }

    fn vim_text_object_paren(
        &mut self,
        _: &VimTextObjectParen,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::Paren, cx);
    }

    fn vim_text_object_bracket(
        &mut self,
        _: &VimTextObjectBracket,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::Bracket, cx);
    }

    fn on_up(&mut self, _: &editor::Up, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.mode() == VimMode::Visual {
            self.sync_visual_selection_for_current_cursor(cx);
        }
    }

    fn on_down(&mut self, _: &editor::Down, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.mode() == VimMode::Visual {
            self.sync_visual_selection_for_current_cursor(cx);
        }
    }

    fn on_left(&mut self, _: &editor::Left, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.mode() == VimMode::Visual {
            self.sync_visual_selection_for_current_cursor(cx);
        }
    }

    fn on_right(&mut self, _: &editor::Right, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.mode() == VimMode::Visual {
            self.sync_visual_selection_for_current_cursor(cx);
        }
    }
}

impl Render for Vim {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .track_focus(&self.editor.focus_handle(cx))
            .key_context(self.state.key_context())
            .on_action(cx.listener(Self::vim_enter_insert_mode))
            .on_action(cx.listener(Self::vim_append))
            .on_action(cx.listener(Self::vim_normal_mode))
            .on_action(cx.listener(Self::vim_visual_mode))
            .on_action(cx.listener(Self::vim_delete_char))
            .on_action(cx.listener(Self::vim_delete_operator))
            .on_action(cx.listener(Self::vim_change_operator))
            .on_action(cx.listener(Self::vim_text_object_inner))
            .on_action(cx.listener(Self::vim_text_object_around))
            .on_action(cx.listener(Self::vim_text_object_word))
            .on_action(cx.listener(Self::vim_text_object_big_word))
            .on_action(cx.listener(Self::vim_text_object_double_quote))
            .on_action(cx.listener(Self::vim_text_object_single_quote))
            .on_action(cx.listener(Self::vim_text_object_paren))
            .on_action(cx.listener(Self::vim_text_object_bracket))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .child(self.editor.clone())
    }
}

impl Focusable for Vim {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

fn resolve_text_object_range(
    text: &str,
    cursor_byte_offset: usize,
    modifier: TextObjectModifier,
    target: TextObjectTarget,
) -> Option<Range<usize>> {
    match target {
        TextObjectTarget::Word => {
            let (start, end) = find_japanese_word_range(text, cursor_byte_offset)
                .or_else(|| find_word_range(text, cursor_byte_offset, is_word_char))?;
            Some(match modifier {
                TextObjectModifier::Inner => start..end,
                TextObjectModifier::Around => expand_around_word(text, start, end),
            })
        }
        TextObjectTarget::BigWord => {
            let (start, end) = find_word_range(text, cursor_byte_offset, |ch| !ch.is_whitespace())?;
            Some(match modifier {
                TextObjectModifier::Inner => start..end,
                TextObjectModifier::Around => expand_around_word(text, start, end),
            })
        }
        TextObjectTarget::DoubleQuote => {
            resolve_quoted_object_range(text, cursor_byte_offset, '"', modifier)
        }
        TextObjectTarget::SingleQuote => {
            resolve_quoted_object_range(text, cursor_byte_offset, '\'', modifier)
        }
        TextObjectTarget::Paren => {
            resolve_delimited_object_range(text, cursor_byte_offset, '(', ')', modifier)
        }
        TextObjectTarget::Bracket => {
            resolve_delimited_object_range(text, cursor_byte_offset, '[', ']', modifier)
        }
    }
}

fn find_japanese_word_range(text: &str, cursor_byte_offset: usize) -> Option<(usize, usize)> {
    let tokenizer = japanese_tokenizer()?;
    let tokens = tokenizer.tokenize(text).ok()?;
    if tokens.is_empty() {
        return None;
    }

    let mut ranges = Vec::with_capacity(tokens.len());
    let mut offset = 0usize;
    for token in tokens {
        let surface = token.surface.as_ref();
        let relative_start = text[offset..].find(surface)?;
        let start = offset + relative_start;
        let end = start + surface.len();
        let range = start..end;
        offset = end;
        ranges.push(range);
    }

    find_token_range_at_or_near_cursor(text, &ranges, cursor_byte_offset)
}

fn japanese_tokenizer() -> Option<&'static Tokenizer> {
    JAPANESE_TOKENIZER
        .get_or_init(|| {
            let dictionary = load_dictionary("embedded://ipadic")
                .map_err(|error| format!("failed to load lindera dictionary: {error}"))?;
            let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
            Ok(Tokenizer::new(segmenter))
        })
        .as_ref()
        .ok()
}

fn find_token_range_at_or_near_cursor(
    text: &str,
    ranges: &[Range<usize>],
    cursor_byte_offset: usize,
) -> Option<(usize, usize)> {
    let mut cursor = cursor_byte_offset.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }

    if let Some(range) = ranges
        .iter()
        .find(|range| range.start <= cursor && cursor < range.end)
    {
        return Some((range.start, range.end));
    }

    if let Some(range) = ranges.iter().find(|range| range.start >= cursor) {
        return Some((range.start, range.end));
    }

    ranges.last().map(|range| (range.start, range.end))
}

fn find_word_range<F>(text: &str, cursor_byte_offset: usize, predicate: F) -> Option<(usize, usize)>
where
    F: Fn(char) -> bool,
{
    if text.is_empty() {
        return None;
    }

    let char_positions: Vec<(usize, char)> = text.char_indices().collect();
    if char_positions.is_empty() {
        return None;
    }

    let mut current_index =
        char_positions.partition_point(|(byte_index, _)| *byte_index < cursor_byte_offset);
    if current_index == char_positions.len() {
        current_index = current_index.saturating_sub(1);
    }

    if !predicate(char_positions[current_index].1) {
        if let Some(next_index) = char_positions
            .iter()
            .enumerate()
            .skip(current_index + 1)
            .find_map(|(index, (_, ch))| predicate(*ch).then_some(index))
        {
            current_index = next_index;
        } else if let Some(previous_index) = char_positions[..=current_index]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, (_, ch))| predicate(*ch).then_some(index))
        {
            current_index = previous_index;
        } else {
            return None;
        }
    }

    let mut start_index = current_index;
    while start_index > 0 && predicate(char_positions[start_index - 1].1) {
        start_index -= 1;
    }

    let mut end_index = current_index + 1;
    while end_index < char_positions.len() && predicate(char_positions[end_index].1) {
        end_index += 1;
    }

    let start = char_positions[start_index].0;
    let end = char_positions
        .get(end_index)
        .map_or(text.len(), |(byte_index, _)| *byte_index);
    Some((start, end))
}

fn expand_around_word(text: &str, word_start: usize, word_end: usize) -> Range<usize> {
    let trailing_end = consume_whitespace_forward(text, word_end);
    if trailing_end > word_end {
        word_start..trailing_end
    } else {
        consume_whitespace_backward(text, word_start)..word_end
    }
}

fn resolve_quoted_object_range(
    text: &str,
    cursor_byte_offset: usize,
    quote: char,
    modifier: TextObjectModifier,
) -> Option<Range<usize>> {
    let (open, close) = find_enclosing_quotes(text, cursor_byte_offset, quote)?;
    Some(match modifier {
        TextObjectModifier::Inner => (open + quote.len_utf8())..close,
        TextObjectModifier::Around => open..(close + quote.len_utf8()),
    })
}

fn find_enclosing_quotes(
    text: &str,
    cursor_byte_offset: usize,
    quote: char,
) -> Option<(usize, usize)> {
    let positions: Vec<usize> = text
        .char_indices()
        .filter_map(|(index, ch)| (ch == quote && !is_escaped(text, index)).then_some(index))
        .collect();
    if positions.len() < 2 {
        return None;
    }

    for pair in positions.windows(2) {
        let open = pair[0];
        let close = pair[1];
        let inside_start = open + quote.len_utf8();
        if (inside_start..=close).contains(&cursor_byte_offset)
            || (open..close).contains(&cursor_byte_offset)
        {
            return Some((open, close));
        }
    }

    None
}

fn resolve_delimited_object_range(
    text: &str,
    cursor_byte_offset: usize,
    open: char,
    close: char,
    modifier: TextObjectModifier,
) -> Option<Range<usize>> {
    let (open_index, close_index) = find_enclosing_pair(text, cursor_byte_offset, open, close)?;
    Some(match modifier {
        TextObjectModifier::Inner => (open_index + open.len_utf8())..close_index,
        TextObjectModifier::Around => open_index..(close_index + close.len_utf8()),
    })
}

fn find_enclosing_pair(
    text: &str,
    cursor_byte_offset: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let cursor_index = chars.partition_point(|(byte_index, _)| *byte_index < cursor_byte_offset);

    for left_index in (0..chars.len()).rev() {
        let (open_offset, ch) = chars[left_index];
        if ch != open || open_offset > cursor_byte_offset {
            continue;
        }

        let mut depth = 0usize;
        for (close_offset, next_ch) in chars.iter().skip(left_index) {
            if *next_ch == open {
                depth += 1;
            } else if *next_ch == close {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let inside_start = open_offset + open.len_utf8();
                    if (inside_start..=*close_offset).contains(&cursor_byte_offset)
                        || (open_offset..*close_offset).contains(&cursor_byte_offset)
                        || cursor_index == chars.len()
                            && *close_offset == text.len() - close.len_utf8()
                    {
                        return Some((open_offset, *close_offset));
                    }
                    break;
                }
            }
        }
    }

    None
}

fn is_escaped(text: &str, byte_index: usize) -> bool {
    if byte_index == 0 {
        return false;
    }

    let mut index = previous_char_start(text, byte_index);
    let mut backslashes = 0usize;
    loop {
        let ch = text[index..].chars().next().unwrap();
        if ch != '\\' {
            break;
        }
        backslashes += 1;
        if index == 0 {
            break;
        }
        index = previous_char_start(text, index);
    }

    backslashes % 2 == 1
}

fn consume_whitespace_forward(text: &str, mut offset: usize) -> usize {
    while let Some(ch) = text[offset..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        offset += ch.len_utf8();
    }
    offset
}

fn consume_whitespace_backward(text: &str, offset: usize) -> usize {
    let mut start = offset;
    while start > 0 {
        let previous = previous_char_start(text, start);
        let ch = text[previous..start].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        start = previous;
    }
    start
}

fn previous_char_start(text: &str, offset: usize) -> usize {
    let mut index = offset.saturating_sub(1);
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_word_targets_current_word() {
        assert_eq!(
            resolve_text_object_range(
                "alpha beta",
                2,
                TextObjectModifier::Inner,
                TextObjectTarget::Word
            ),
            Some(0..5)
        );
    }

    #[test]
    fn inner_word_skips_forward_from_whitespace() {
        assert_eq!(
            resolve_text_object_range(
                "alpha beta",
                5,
                TextObjectModifier::Inner,
                TextObjectTarget::Word
            ),
            Some(6..10)
        );
    }

    #[test]
    fn around_word_prefers_trailing_spaces() {
        assert_eq!(
            resolve_text_object_range(
                "alpha   beta",
                1,
                TextObjectModifier::Around,
                TextObjectTarget::Word
            ),
            Some(0..8)
        );
    }

    #[test]
    fn around_word_uses_leading_spaces_when_no_trailing_spaces() {
        assert_eq!(
            resolve_text_object_range(
                "alpha",
                2,
                TextObjectModifier::Around,
                TextObjectTarget::Word
            ),
            Some(0..5)
        );
        assert_eq!(
            resolve_text_object_range(
                "   alpha",
                4,
                TextObjectModifier::Around,
                TextObjectTarget::Word
            ),
            Some(0..8)
        );
    }

    #[test]
    fn big_word_includes_punctuation_until_whitespace() {
        assert_eq!(
            resolve_text_object_range(
                "foo.bar baz",
                2,
                TextObjectModifier::Inner,
                TextObjectTarget::BigWord
            ),
            Some(0..7)
        );
    }

    #[test]
    fn japanese_word_uses_lindera_boundaries() {
        assert_eq!(
            resolve_text_object_range(
                "関西国際空港限定トートバッグ",
                "関西国際空港".len() + 1,
                TextObjectModifier::Inner,
                TextObjectTarget::Word
            ),
            Some("関西国際空港".len().."関西国際空港限定".len())
        );
    }

    #[test]
    fn around_japanese_word_expands_whitespace() {
        let text = "関西国際空港 限定 トートバッグ";
        let cursor = text.find("限定").unwrap();
        assert_eq!(
            resolve_text_object_range(
                text,
                cursor,
                TextObjectModifier::Around,
                TextObjectTarget::Word
            ),
            Some("関西国際空港 ".len().."関西国際空港 限定 ".len())
        );
    }

    #[test]
    fn double_quote_objects_work() {
        assert_eq!(
            resolve_text_object_range(
                r#"say "hello world" now"#,
                7,
                TextObjectModifier::Inner,
                TextObjectTarget::DoubleQuote
            ),
            Some(5..16)
        );
        assert_eq!(
            resolve_text_object_range(
                r#"say "hello world" now"#,
                7,
                TextObjectModifier::Around,
                TextObjectTarget::DoubleQuote
            ),
            Some(4..17)
        );
    }

    #[test]
    fn single_quote_objects_work() {
        assert_eq!(
            resolve_text_object_range(
                "say 'hello' now",
                7,
                TextObjectModifier::Inner,
                TextObjectTarget::SingleQuote
            ),
            Some(5..10)
        );
    }

    #[test]
    fn paren_objects_work() {
        assert_eq!(
            resolve_text_object_range(
                "call(foo(bar))",
                10,
                TextObjectModifier::Inner,
                TextObjectTarget::Paren
            ),
            Some(9..12)
        );
        assert_eq!(
            resolve_text_object_range(
                "call(foo(bar))",
                10,
                TextObjectModifier::Around,
                TextObjectTarget::Paren
            ),
            Some(8..13)
        );
    }

    #[test]
    fn bracket_objects_work() {
        assert_eq!(
            resolve_text_object_range(
                "arr[one[two]]",
                9,
                TextObjectModifier::Inner,
                TextObjectTarget::Bracket
            ),
            Some(8..11)
        );
        assert_eq!(
            resolve_text_object_range(
                "arr[one[two]]",
                9,
                TextObjectModifier::Around,
                TextObjectTarget::Bracket
            ),
            Some(7..12)
        );
    }
}
