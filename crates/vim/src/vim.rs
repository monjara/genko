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
        VimYankOperator,
        VimTextObjectInner,
        VimTextObjectAround,
        VimTextObjectWord,
        VimTextObjectBigWord,
        VimTextObjectDoubleQuote,
        VimTextObjectSingleQuote,
        VimTextObjectParen,
        VimTextObjectBracket,
        VimMoveWordForward,
        VimMoveBigWordForward,
        VimMoveWordEndForward,
        VimPasteAfter,
        VimPasteBefore,
        VimUndo,
        VimRedo,
        VimRepeatLastChange,
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
    Yank,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MotionKind {
    WordForward,
    BigWordForward,
    WordEndForward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertKind {
    Insert,
    Append,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RepeatTarget {
    Motion(MotionKind),
    TextObject(TextObjectModifier, TextObjectTarget),
    Line,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RepeatableCommand {
    DeleteChar,
    Delete(RepeatTarget),
    Change {
        target: RepeatTarget,
        inserted_text: String,
    },
    PasteAfter,
    PasteBefore,
    Insert {
        kind: InsertKind,
        inserted_text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingInsert {
    kind: InsertKind,
    change_target: Option<RepeatTarget>,
    before_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum YankRegister {
    Empty,
    CharWise(String),
    LineWise(String),
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
            (VimMode::Normal, Some(VimOperator::Yank), None) => "Genko vim_mode=operator_yank",
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
            (VimMode::Normal, Some(VimOperator::Yank), Some(TextObjectModifier::Inner)) => {
                "Genko vim_mode=operator_yank_inner"
            }
            (VimMode::Normal, Some(VimOperator::Yank), Some(TextObjectModifier::Around)) => {
                "Genko vim_mode=operator_yank_around"
            }
        }
    }
}

pub struct Vim {
    editor: Entity<Editor>,
    state: VimState,
    yank_register: YankRegister,
    last_change: Option<RepeatableCommand>,
    pending_insert: Option<PendingInsert>,
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
            KeyBinding::new("escape", VimNormalMode, Some("vim_mode == operator_yank")),
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
            KeyBinding::new(
                "escape",
                VimNormalMode,
                Some("vim_mode == operator_yank_inner"),
            ),
            KeyBinding::new(
                "escape",
                VimNormalMode,
                Some("vim_mode == operator_yank_around"),
            ),
            KeyBinding::new("v", VimVisualMode, Some("vim_mode == normal")),
            KeyBinding::new("v", VimNormalMode, Some("vim_mode == visual")),
            KeyBinding::new("d", VimDeleteOperator, Some("vim_mode == normal")),
            KeyBinding::new("c", VimChangeOperator, Some("vim_mode == normal")),
            KeyBinding::new("y", VimYankOperator, Some("vim_mode == normal")),
            KeyBinding::new("p", VimPasteAfter, Some("vim_mode == normal")),
            KeyBinding::new("P", VimPasteBefore, Some("vim_mode == normal")),
            KeyBinding::new("u", VimUndo, Some("vim_mode == normal")),
            KeyBinding::new("ctrl-r", VimRedo, Some("vim_mode == normal")),
            KeyBinding::new(".", VimRepeatLastChange, Some("vim_mode == normal")),
            KeyBinding::new("w", VimMoveWordForward, Some("vim_mode == normal")),
            KeyBinding::new("W", VimMoveBigWordForward, Some("vim_mode == normal")),
            KeyBinding::new("e", VimMoveWordEndForward, Some("vim_mode == normal")),
            KeyBinding::new("d", VimDeleteOperator, Some("vim_mode == operator_delete")),
            KeyBinding::new("c", VimChangeOperator, Some("vim_mode == operator_change")),
            KeyBinding::new("y", VimYankOperator, Some("vim_mode == operator_yank")),
            KeyBinding::new("w", VimMoveWordForward, Some("vim_mode == operator_delete")),
            KeyBinding::new("w", VimMoveWordForward, Some("vim_mode == operator_change")),
            KeyBinding::new("w", VimMoveWordForward, Some("vim_mode == operator_yank")),
            KeyBinding::new(
                "W",
                VimMoveBigWordForward,
                Some("vim_mode == operator_delete"),
            ),
            KeyBinding::new(
                "W",
                VimMoveBigWordForward,
                Some("vim_mode == operator_change"),
            ),
            KeyBinding::new(
                "W",
                VimMoveBigWordForward,
                Some("vim_mode == operator_yank"),
            ),
            KeyBinding::new(
                "e",
                VimMoveWordEndForward,
                Some("vim_mode == operator_delete"),
            ),
            KeyBinding::new(
                "e",
                VimMoveWordEndForward,
                Some("vim_mode == operator_change"),
            ),
            KeyBinding::new(
                "e",
                VimMoveWordEndForward,
                Some("vim_mode == operator_yank"),
            ),
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
            KeyBinding::new("i", VimTextObjectInner, Some("vim_mode == operator_yank")),
            KeyBinding::new("a", VimTextObjectAround, Some("vim_mode == operator_yank")),
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
                "w",
                VimTextObjectWord,
                Some("vim_mode == operator_yank_inner"),
            ),
            KeyBinding::new(
                "w",
                VimTextObjectWord,
                Some("vim_mode == operator_yank_around"),
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
                "W",
                VimTextObjectBigWord,
                Some("vim_mode == operator_yank_inner"),
            ),
            KeyBinding::new(
                "W",
                VimTextObjectBigWord,
                Some("vim_mode == operator_yank_around"),
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
                "\"",
                VimTextObjectDoubleQuote,
                Some("vim_mode == operator_yank_inner"),
            ),
            KeyBinding::new(
                "\"",
                VimTextObjectDoubleQuote,
                Some("vim_mode == operator_yank_around"),
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
                "'",
                VimTextObjectSingleQuote,
                Some("vim_mode == operator_yank_inner"),
            ),
            KeyBinding::new(
                "'",
                VimTextObjectSingleQuote,
                Some("vim_mode == operator_yank_around"),
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
                "(",
                VimTextObjectParen,
                Some("vim_mode == operator_yank_inner"),
            ),
            KeyBinding::new(
                "(",
                VimTextObjectParen,
                Some("vim_mode == operator_yank_around"),
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
            KeyBinding::new(
                "[",
                VimTextObjectBracket,
                Some("vim_mode == operator_yank_inner"),
            ),
            KeyBinding::new(
                "[",
                VimTextObjectBracket,
                Some("vim_mode == operator_yank_around"),
            ),
            KeyBinding::new(
                "h",
                editor::Left,
                Some("vim_mode == normal || vim_mode == visual"),
            ),
            KeyBinding::new(
                "j",
                editor::Down,
                Some("vim_mode == normal || vim_mode == visual"),
            ),
            KeyBinding::new(
                "k",
                editor::Up,
                Some("vim_mode == normal || vim_mode == visual"),
            ),
            KeyBinding::new(
                "l",
                editor::Right,
                Some("vim_mode == normal || vim_mode == visual"),
            ),
            KeyBinding::new(
                "x",
                VimDeleteChar,
                Some("vim_mode == normal || vim_mode == visual"),
            ),
        ]);
    }

    pub fn new(editor: Entity<Editor>) -> Self {
        Self {
            editor,
            state: VimState::new(),
            yank_register: YankRegister::Empty,
            last_change: None,
            pending_insert: None,
        }
    }

    pub fn update_viewport_size(&mut self, size: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) {
        let text_input_enabled = self.state.mode() == VimMode::Insert;
        self.editor.update(cx, |editor, cx| {
            editor.update_viewport_size(size, cx);
            editor.set_text_input_enabled(text_input_enabled, cx);
        });
    }

    fn start_insert_session(
        &mut self,
        kind: InsertKind,
        change_target: Option<RepeatTarget>,
        before_text: String,
        cx: &mut Context<Self>,
    ) {
        self.pending_insert = Some(PendingInsert {
            kind,
            change_target,
            before_text,
        });
        self.editor.update(cx, |editor, _| {
            editor.begin_transaction();
        });
        self.state.set_mode(VimMode::Insert);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(true, cx);
            editor.collapse_selection_to_cursor_offset(cx);
        });
        cx.notify();
    }

    fn enter_insert_mode(&mut self, cx: &mut Context<Self>) {
        let before_text = self.editor.read(cx).snapshot_text();
        self.start_insert_session(InsertKind::Insert, None, before_text, cx);
    }

    fn append(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.move_cursor_by(1, cx);
        });
        let before_text = self.editor.read(cx).snapshot_text();
        self.start_insert_session(InsertKind::Append, None, before_text, cx);
    }

    fn normal_mode(&mut self, cx: &mut Context<Self>) {
        let leaving_insert = self.state.mode() == VimMode::Insert;
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        if leaving_insert {
            self.finish_insert_session(cx);
        }
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
        self.last_change = Some(RepeatableCommand::DeleteChar);
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn paste_after(&mut self, cx: &mut Context<Self>) {
        let (text, cursor) = {
            let editor = self.editor.read(cx);
            (editor.snapshot_text(), editor.cursor_byte_offset())
        };
        let Some((insertion_offset, inserted_text)) = resolve_paste(
            text.as_str(),
            cursor,
            &self.yank_register,
            PastePosition::After,
        ) else {
            return;
        };

        self.editor.update(cx, |editor, cx| {
            editor.replace_byte_range(
                insertion_offset..insertion_offset,
                inserted_text.as_str(),
                cx,
            );
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.last_change = Some(RepeatableCommand::PasteAfter);
        cx.notify();
    }

    fn paste_before(&mut self, cx: &mut Context<Self>) {
        let (text, cursor) = {
            let editor = self.editor.read(cx);
            (editor.snapshot_text(), editor.cursor_byte_offset())
        };
        let Some((insertion_offset, inserted_text)) = resolve_paste(
            text.as_str(),
            cursor,
            &self.yank_register,
            PastePosition::Before,
        ) else {
            return;
        };

        self.editor.update(cx, |editor, cx| {
            editor.replace_byte_range(
                insertion_offset..insertion_offset,
                inserted_text.as_str(),
                cx,
            );
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.last_change = Some(RepeatableCommand::PasteBefore);
        cx.notify();
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        self.pending_insert = None;
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            let _ = editor.undo(cx);
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        self.pending_insert = None;
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.editor.update(cx, |editor, cx| {
            let _ = editor.redo(cx);
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn repeat_last_change(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self.last_change.clone() else {
            return;
        };
        self.execute_repeatable_command(command, cx);
    }

    fn move_by_motion(&mut self, motion: MotionKind, cx: &mut Context<Self>) {
        let (text, cursor) = {
            let editor = self.editor.read(cx);
            (editor.snapshot_text(), editor.cursor_byte_offset())
        };

        if self.state.pending_operator().is_some() {
            self.apply_motion(motion, &text, cursor, cx);
            return;
        }

        let Some(target) = resolve_motion_target(&text, cursor, motion) else {
            return;
        };

        self.editor.update(cx, |editor, cx| {
            editor.move_cursor_to_byte_offset(target, cx);
        });
        cx.notify();
    }

    fn apply_motion(
        &mut self,
        motion: MotionKind,
        text: &str,
        cursor: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(operator) = self.state.pending_operator() else {
            return;
        };
        let Some(range) = resolve_motion_range(text, cursor, motion, operator) else {
            self.state.clear_pending();
            cx.notify();
            return;
        };

        self.apply_operator_to_range(
            operator,
            range,
            Some(RepeatTarget::Motion(motion)),
            text,
            cx,
        );
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

        self.apply_operator_to_range(
            operator,
            range,
            Some(RepeatTarget::TextObject(modifier, target)),
            &text,
            cx,
        );
    }

    fn apply_operator_to_range(
        &mut self,
        operator: VimOperator,
        range: Range<usize>,
        repeat_target: Option<RepeatTarget>,
        source_text: &str,
        cx: &mut Context<Self>,
    ) {
        match operator {
            VimOperator::Yank => {
                let yanked = self.editor.read(cx).text_in_range(range);
                self.yank_register = if repeat_target == Some(RepeatTarget::Line) {
                    YankRegister::LineWise(trim_trailing_newline(&yanked))
                } else {
                    YankRegister::CharWise(yanked)
                };
            }
            VimOperator::Delete | VimOperator::Change => {
                if repeat_target == Some(RepeatTarget::Line) {
                    let yanked = self.editor.read(cx).text_in_range(range.clone());
                    self.yank_register = YankRegister::LineWise(trim_trailing_newline(&yanked));
                }
                if operator == VimOperator::Change {
                    self.editor.update(cx, |editor, _| {
                        editor.begin_transaction();
                    });
                }
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
                        VimOperator::Yank => {}
                    }
                });
                match operator {
                    VimOperator::Delete => {
                        if let Some(target) = repeat_target {
                            self.last_change = Some(RepeatableCommand::Delete(target));
                        }
                    }
                    VimOperator::Change => {
                        self.pending_insert = Some(PendingInsert {
                            kind: InsertKind::Insert,
                            change_target: repeat_target,
                            before_text: source_text.to_string(),
                        });
                    }
                    VimOperator::Yank => {}
                }
            }
        }

        self.state.clear_pending();
        self.state.set_visual_anchor_cell(None);
        self.state.set_mode(match operator {
            VimOperator::Delete => VimMode::Normal,
            VimOperator::Change => VimMode::Insert,
            VimOperator::Yank => VimMode::Normal,
        });
        cx.notify();
    }

    fn apply_current_line_operator(&mut self, operator: VimOperator, cx: &mut Context<Self>) {
        let (text, cursor) = {
            let editor = self.editor.read(cx);
            (editor.snapshot_text(), editor.cursor_byte_offset())
        };
        let Some(range) = current_line_delete_range(text.as_str(), cursor) else {
            self.state.clear_pending();
            cx.notify();
            return;
        };

        self.apply_operator_to_range(operator, range, Some(RepeatTarget::Line), &text, cx);
    }

    fn finish_insert_session(&mut self, cx: &mut Context<Self>) {
        let Some(pending_insert) = self.pending_insert.take() else {
            return;
        };
        let (committed, after_text) = self.editor.update(cx, |editor, cx| {
            let committed = editor.commit_transaction(cx);
            (committed, editor.snapshot_text())
        });
        if !committed {
            return;
        }

        let inserted_text = inserted_text_between(&pending_insert.before_text, &after_text);
        self.last_change = match pending_insert.change_target {
            Some(target) => Some(RepeatableCommand::Change {
                target,
                inserted_text,
            }),
            None if inserted_text.is_empty() => None,
            None => Some(RepeatableCommand::Insert {
                kind: pending_insert.kind,
                inserted_text,
            }),
        };
    }

    fn execute_repeatable_command(&mut self, command: RepeatableCommand, cx: &mut Context<Self>) {
        match command {
            RepeatableCommand::DeleteChar => self.delete_char(cx),
            RepeatableCommand::Delete(target) => self.execute_repeat_target(target, None, cx),
            RepeatableCommand::Change {
                target,
                inserted_text,
            } => self.execute_repeat_target(target, Some(inserted_text), cx),
            RepeatableCommand::PasteAfter => self.paste_after(cx),
            RepeatableCommand::PasteBefore => self.paste_before(cx),
            RepeatableCommand::Insert {
                kind,
                inserted_text,
            } => self.execute_repeat_insert(kind, inserted_text, cx),
        }
    }

    fn execute_repeat_target(
        &mut self,
        target: RepeatTarget,
        inserted_text: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let (text, cursor) = {
            let editor = self.editor.read(cx);
            (editor.snapshot_text(), editor.cursor_byte_offset())
        };
        let Some(range) =
            resolve_repeat_target_range(&text, cursor, target, inserted_text.is_some())
        else {
            return;
        };

        if let Some(inserted_text) = inserted_text {
            self.editor.update(cx, |editor, cx| {
                editor.begin_transaction();
                editor.replace_byte_range(range, "", cx);
                let insertion_offset = editor.cursor_byte_offset();
                editor.replace_byte_range(
                    insertion_offset..insertion_offset,
                    inserted_text.as_str(),
                    cx,
                );
                editor.set_text_input_enabled(false, cx);
                editor.collapse_selection_to_cursor_cell(cx);
                let _ = editor.commit_transaction(cx);
            });
            self.state.set_mode(VimMode::Normal);
            self.state.set_visual_anchor_cell(None);
            self.state.clear_pending();
            self.last_change = Some(RepeatableCommand::Change {
                target,
                inserted_text,
            });
        } else {
            self.apply_operator_to_range(VimOperator::Delete, range, Some(target), &text, cx);
        }
    }

    fn execute_repeat_insert(
        &mut self,
        kind: InsertKind,
        inserted_text: String,
        cx: &mut Context<Self>,
    ) {
        if inserted_text.is_empty() {
            return;
        }
        self.editor.update(cx, |editor, cx| {
            editor.begin_transaction();
            if kind == InsertKind::Append {
                editor.move_cursor_by(1, cx);
            }
            let insertion_offset = editor.cursor_byte_offset();
            editor.replace_byte_range(
                insertion_offset..insertion_offset,
                inserted_text.as_str(),
                cx,
            );
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
            let _ = editor.commit_transaction(cx);
        });
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.state.clear_pending();
        self.last_change = Some(RepeatableCommand::Insert {
            kind,
            inserted_text,
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
        if self.state.pending_operator() == Some(VimOperator::Delete) {
            self.apply_current_line_operator(VimOperator::Delete, cx);
            return;
        }
        self.begin_operator(VimOperator::Delete, cx);
    }

    fn vim_change_operator(
        &mut self,
        _: &VimChangeOperator,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.pending_operator() == Some(VimOperator::Change) {
            self.apply_current_line_operator(VimOperator::Change, cx);
            return;
        }
        self.begin_operator(VimOperator::Change, cx);
    }

    fn vim_yank_operator(
        &mut self,
        _: &VimYankOperator,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.pending_operator() == Some(VimOperator::Yank) {
            self.apply_current_line_operator(VimOperator::Yank, cx);
            return;
        }
        self.begin_operator(VimOperator::Yank, cx);
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

    fn vim_move_word_forward(
        &mut self,
        _: &VimMoveWordForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::WordForward, cx);
    }

    fn vim_move_big_word_forward(
        &mut self,
        _: &VimMoveBigWordForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::BigWordForward, cx);
    }

    fn vim_move_word_end_forward(
        &mut self,
        _: &VimMoveWordEndForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::WordEndForward, cx);
    }

    fn vim_paste_after(&mut self, _: &VimPasteAfter, _window: &mut Window, cx: &mut Context<Self>) {
        self.paste_after(cx);
    }

    fn vim_paste_before(
        &mut self,
        _: &VimPasteBefore,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.paste_before(cx);
    }

    fn vim_undo(&mut self, _: &VimUndo, _window: &mut Window, cx: &mut Context<Self>) {
        self.undo(cx);
    }

    fn vim_redo(&mut self, _: &VimRedo, _window: &mut Window, cx: &mut Context<Self>) {
        self.redo(cx);
    }

    fn vim_repeat_last_change(
        &mut self,
        _: &VimRepeatLastChange,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.repeat_last_change(cx);
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
            .on_action(cx.listener(Self::vim_yank_operator))
            .on_action(cx.listener(Self::vim_text_object_inner))
            .on_action(cx.listener(Self::vim_text_object_around))
            .on_action(cx.listener(Self::vim_text_object_word))
            .on_action(cx.listener(Self::vim_text_object_big_word))
            .on_action(cx.listener(Self::vim_text_object_double_quote))
            .on_action(cx.listener(Self::vim_text_object_single_quote))
            .on_action(cx.listener(Self::vim_text_object_paren))
            .on_action(cx.listener(Self::vim_text_object_bracket))
            .on_action(cx.listener(Self::vim_move_word_forward))
            .on_action(cx.listener(Self::vim_move_big_word_forward))
            .on_action(cx.listener(Self::vim_move_word_end_forward))
            .on_action(cx.listener(Self::vim_paste_after))
            .on_action(cx.listener(Self::vim_paste_before))
            .on_action(cx.listener(Self::vim_undo))
            .on_action(cx.listener(Self::vim_redo))
            .on_action(cx.listener(Self::vim_repeat_last_change))
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

fn resolve_motion_target(
    text: &str,
    cursor_byte_offset: usize,
    motion: MotionKind,
) -> Option<usize> {
    match motion {
        MotionKind::WordForward => resolve_forward_word_start(text, cursor_byte_offset, false),
        MotionKind::BigWordForward => resolve_forward_word_start(text, cursor_byte_offset, true),
        MotionKind::WordEndForward => resolve_forward_word_end(text, cursor_byte_offset),
    }
}

fn resolve_motion_range(
    text: &str,
    cursor_byte_offset: usize,
    motion: MotionKind,
    operator: VimOperator,
) -> Option<Range<usize>> {
    if text.is_empty() {
        return None;
    }

    let start = cursor_byte_offset.min(text.len());
    match motion {
        MotionKind::WordForward | MotionKind::BigWordForward => {
            if operator == VimOperator::Change {
                let target = match motion {
                    MotionKind::WordForward => find_japanese_word_range(text, start)
                        .map(|(_, end)| end)
                        .or_else(|| find_word_range(text, start, is_word_char).map(|(_, end)| end)),
                    MotionKind::BigWordForward => {
                        find_word_range(text, start, |ch| !ch.is_whitespace()).map(|(_, end)| end)
                    }
                    MotionKind::WordEndForward => None,
                }
                .unwrap_or(text.len());
                Some(start.min(target)..target.max(start))
            } else {
                let target = resolve_motion_target(text, start, motion).unwrap_or(text.len());
                Some(start.min(target)..target.max(start))
            }
        }
        MotionKind::WordEndForward => {
            let target = resolve_motion_target(text, start, motion)?;
            let end = next_char_end(text, target);
            Some(start.min(end)..end.max(start))
        }
    }
}

fn resolve_repeat_target_range(
    text: &str,
    cursor_byte_offset: usize,
    target: RepeatTarget,
    is_change: bool,
) -> Option<Range<usize>> {
    match target {
        RepeatTarget::Motion(motion) => resolve_motion_range(
            text,
            cursor_byte_offset,
            motion,
            if is_change {
                VimOperator::Change
            } else {
                VimOperator::Delete
            },
        ),
        RepeatTarget::TextObject(modifier, target) => {
            resolve_text_object_range(text, cursor_byte_offset, modifier, target)
        }
        RepeatTarget::Line => current_line_delete_range(text, cursor_byte_offset),
    }
}

fn resolve_forward_word_start(
    text: &str,
    cursor_byte_offset: usize,
    big_word: bool,
) -> Option<usize> {
    if text.is_empty() {
        return None;
    }

    let ranges = token_ranges_for_target(
        text,
        if cursor_byte_offset < text.len() {
            cursor_byte_offset
        } else {
            text.len().saturating_sub(1)
        },
        if big_word {
            TextObjectTarget::BigWord
        } else {
            TextObjectTarget::Word
        },
    )?;

    let current = current_token_index(&ranges, cursor_byte_offset);
    if let Some(index) = current {
        if cursor_byte_offset < ranges[index].end.saturating_sub(1) {
            return ranges.get(index + 1).map(|range| range.start);
        }
        return ranges.get(index + 1).map(|range| range.start);
    }

    ranges
        .iter()
        .find(|range| range.start >= cursor_byte_offset)
        .map(|range| range.start)
}

fn resolve_forward_word_end(text: &str, cursor_byte_offset: usize) -> Option<usize> {
    if text.is_empty() {
        return None;
    }

    let ranges = token_ranges_for_target(
        text,
        if cursor_byte_offset < text.len() {
            cursor_byte_offset
        } else {
            text.len().saturating_sub(1)
        },
        TextObjectTarget::Word,
    )?;

    let current = current_token_index(&ranges, cursor_byte_offset);
    if let Some(index) = current {
        let range = &ranges[index];
        if cursor_byte_offset < range.end.saturating_sub(1) {
            return Some(previous_char_start(text, range.end));
        }
        return ranges
            .get(index + 1)
            .map(|range| previous_char_start(text, range.end));
    }

    ranges
        .iter()
        .find(|range| range.start >= cursor_byte_offset)
        .map(|range| previous_char_start(text, range.end))
}

fn token_ranges_for_target(
    text: &str,
    cursor_byte_offset: usize,
    target: TextObjectTarget,
) -> Option<Vec<Range<usize>>> {
    match target {
        TextObjectTarget::Word => japanese_token_ranges(text)
            .or_else(|| word_ranges(text, is_word_char))
            .filter(|ranges| !ranges.is_empty())
            .or_else(|| {
                let _ = cursor_byte_offset;
                None
            }),
        TextObjectTarget::BigWord => word_ranges(text, |ch| !ch.is_whitespace()),
        _ => None,
    }
}

fn japanese_token_ranges(text: &str) -> Option<Vec<Range<usize>>> {
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
        ranges.push(start..end);
        offset = end;
    }
    Some(ranges)
}

fn word_ranges<F>(text: &str, predicate: F) -> Option<Vec<Range<usize>>>
where
    F: Fn(char) -> bool,
{
    let mut ranges = Vec::new();
    let mut current_start = None;

    for (index, ch) in text.char_indices() {
        if predicate(ch) {
            current_start.get_or_insert(index);
        } else if let Some(start) = current_start.take() {
            ranges.push(start..index);
        }
    }

    if let Some(start) = current_start {
        ranges.push(start..text.len());
    }

    (!ranges.is_empty()).then_some(ranges)
}

fn current_token_index(ranges: &[Range<usize>], cursor_byte_offset: usize) -> Option<usize> {
    ranges
        .iter()
        .position(|range| range.start <= cursor_byte_offset && cursor_byte_offset < range.end)
}

fn find_japanese_word_range(text: &str, cursor_byte_offset: usize) -> Option<(usize, usize)> {
    let ranges = japanese_token_ranges(text)?;
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

fn next_char_end(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let ch = text[offset..].chars().next().unwrap();
    offset + ch.len_utf8()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PastePosition {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineBounds {
    start: usize,
    end_content: usize,
    end_with_newline: usize,
}

fn resolve_paste(
    text: &str,
    cursor_byte_offset: usize,
    register: &YankRegister,
    position: PastePosition,
) -> Option<(usize, String)> {
    match register {
        YankRegister::Empty => None,
        YankRegister::CharWise(content) => {
            let insertion_offset = match position {
                PastePosition::Before => cursor_byte_offset,
                PastePosition::After => next_char_end(text, cursor_byte_offset),
            };
            Some((insertion_offset, content.clone()))
        }
        YankRegister::LineWise(content) => {
            let line = current_line_bounds(text, cursor_byte_offset)?;
            let insertion_offset = match position {
                PastePosition::Before => line.start,
                PastePosition::After => {
                    if line.end_with_newline > line.end_content {
                        line.end_with_newline
                    } else {
                        line.end_content
                    }
                }
            };
            let inserted = match position {
                PastePosition::Before => linewise_text_before(content, text.is_empty()),
                PastePosition::After => linewise_text_after(
                    content,
                    text.is_empty(),
                    line.end_with_newline == text.len(),
                ),
            };
            Some((insertion_offset, inserted))
        }
    }
}

fn current_line_bounds(text: &str, cursor_byte_offset: usize) -> Option<LineBounds> {
    if text.is_empty() {
        return Some(LineBounds {
            start: 0,
            end_content: 0,
            end_with_newline: 0,
        });
    }

    let cursor = cursor_byte_offset.min(text.len());
    let start = text[..cursor]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let end_content = text[start..]
        .find('\n')
        .map(|index| start + index)
        .unwrap_or(text.len());
    let end_with_newline = if end_content < text.len() {
        end_content + 1
    } else {
        end_content
    };

    Some(LineBounds {
        start,
        end_content,
        end_with_newline,
    })
}

fn current_line_delete_range(text: &str, cursor_byte_offset: usize) -> Option<Range<usize>> {
    let line = current_line_bounds(text, cursor_byte_offset)?;
    if line.end_with_newline > line.end_content {
        return Some(line.start..line.end_with_newline);
    }
    if line.start > 0 {
        return Some(previous_char_start(text, line.start)..line.end_content);
    }
    Some(line.start..line.end_content)
}

fn trim_trailing_newline(text: &str) -> String {
    text.strip_suffix('\n').unwrap_or(text).to_string()
}

fn inserted_text_between(before: &str, after: &str) -> String {
    let mut prefix_len = 0usize;
    let prefix_max = before.len().min(after.len());
    while prefix_len < prefix_max && before.as_bytes()[prefix_len] == after.as_bytes()[prefix_len] {
        prefix_len += 1;
    }

    let mut before_suffix = before.len();
    let mut after_suffix = after.len();
    while before_suffix > prefix_len
        && after_suffix > prefix_len
        && before.as_bytes()[before_suffix - 1] == after.as_bytes()[after_suffix - 1]
    {
        before_suffix -= 1;
        after_suffix -= 1;
    }

    after[prefix_len..after_suffix].to_string()
}

fn linewise_text_before(content: &str, is_empty_document: bool) -> String {
    if is_empty_document {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

fn linewise_text_after(content: &str, is_empty_document: bool, after_last_line: bool) -> String {
    if is_empty_document {
        return content.to_string();
    }
    if after_last_line {
        format!("\n{content}")
    } else {
        format!("{content}\n")
    }
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

    #[test]
    fn word_forward_moves_to_next_word_start() {
        assert_eq!(
            resolve_motion_target("alpha beta gamma", 0, MotionKind::WordForward),
            Some(6)
        );
        assert_eq!(
            resolve_motion_target("alpha beta gamma", 5, MotionKind::WordForward),
            Some(6)
        );
    }

    #[test]
    fn big_word_forward_skips_until_next_whitespace_boundary() {
        assert_eq!(
            resolve_motion_target("foo.bar baz", 0, MotionKind::BigWordForward),
            Some(8)
        );
    }

    #[test]
    fn word_end_moves_to_current_or_next_word_end() {
        assert_eq!(
            resolve_motion_target("alpha beta", 1, MotionKind::WordEndForward),
            Some(4)
        );
        assert_eq!(
            resolve_motion_target("alpha beta", 5, MotionKind::WordEndForward),
            Some(9)
        );
    }

    #[test]
    fn japanese_word_forward_uses_lindera_boundaries() {
        let text = "関西国際空港限定トートバッグ";
        assert_eq!(
            resolve_motion_target(text, 0, MotionKind::WordForward),
            Some("関西国際空港".len())
        );
    }

    #[test]
    fn delete_word_motion_targets_next_word_start() {
        assert_eq!(
            resolve_motion_range(
                "alpha beta",
                0,
                MotionKind::WordForward,
                VimOperator::Delete
            ),
            Some(0..6)
        );
    }

    #[test]
    fn change_word_motion_stops_at_current_word_end() {
        assert_eq!(
            resolve_motion_range(
                "alpha beta",
                0,
                MotionKind::WordForward,
                VimOperator::Change
            ),
            Some(0..5)
        );
    }

    #[test]
    fn end_motion_range_includes_target_character() {
        assert_eq!(
            resolve_motion_range(
                "alpha beta",
                0,
                MotionKind::WordEndForward,
                VimOperator::Delete
            ),
            Some(0..5)
        );
    }

    #[test]
    fn line_delete_range_prefers_trailing_newline() {
        assert_eq!(current_line_delete_range("one\ntwo", 0), Some(0..4));
    }

    #[test]
    fn line_delete_range_uses_leading_newline_for_last_line() {
        assert_eq!(current_line_delete_range("one\ntwo", 4), Some(3..7));
    }

    #[test]
    fn linewise_paste_after_last_line_inserts_newline_prefix() {
        assert_eq!(
            resolve_paste(
                "one",
                0,
                &YankRegister::LineWise("two".to_string()),
                PastePosition::After
            ),
            Some((3, "\ntwo".to_string()))
        );
    }

    #[test]
    fn linewise_paste_before_prefixes_line_and_newline() {
        assert_eq!(
            resolve_paste(
                "one\ntwo",
                4,
                &YankRegister::LineWise("zero".to_string()),
                PastePosition::Before
            ),
            Some((4, "zero\n".to_string()))
        );
    }
}
