use std::ops::Range;

use crate::editor::Editor;
use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window, actions, div,
};
use rope::BLANK_CELL;
use settings::AppSettings;

mod bindings;
mod block;
mod state;
#[cfg(test)]
mod tests;
mod text_objects;

use block::{
    PastePosition, block_byte_ranges_from_cursor, block_insert_target_cells,
    block_insert_target_cells_from_cursor, block_paste_operations, block_selection_byte_ranges,
    build_block_register, build_block_register_from_cursor, current_column_cell_range,
    top_right_block_cell,
};
use state::{
    BlockInsertKind, InsertKind, MotionKind, PendingBlockInsert, PendingInsert, RepeatTarget,
    RepeatableCommand, TextObjectModifier, TextObjectTarget, VimOperator, YankRegister,
};
use text_objects::{
    resolve_motion_range, resolve_motion_target, resolve_repeat_target_range,
    resolve_text_object_range,
};

pub use state::{VimMode, VimState};
use theme::Theme;

use self::state::operator_key_context;

actions!(
    vim,
    [
        VimEnterInsertMode,
        VimAppend,
        VimNormalMode,
        VimVisualMode,
        VimVisualBlockMode,
        VimBlockInsertBefore,
        VimBlockAppendAfter,
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
        VimMoveWordBackward,
        VimMoveBigWordBackward,
        VimMoveDocumentStart,
        VimMoveDocumentEnd,
        VimMoveLeft,
        VimMoveDown,
        VimMoveUp,
        VimMoveRight,
        VimPasteAfter,
        VimPasteBefore,
        VimOpenNextColumn,
        VimUndo,
        VimRedo,
        VimRepeatLastChange,
        VimCommandWrite,
        VimCommandQuit,
    ]
);

pub fn init(cx: &mut App) {
    bindings::init(cx);
    state::init(cx);
}

pub struct Vim {
    editor: Entity<Editor>,
    yank_register: YankRegister,
    last_change: Option<RepeatableCommand>,
    pending_operator: Option<VimOperator>,
    pending_insert: Option<PendingInsert>,
    pending_block_insert: Option<PendingBlockInsert>,
    pending_text_object_modifier: Option<TextObjectModifier>,
    visual_anchor_cell: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandAction {
    YankWholeBuffer,
    Write,
    Quit,
    DeleteWholeBuffer,
}

impl Vim {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let editor = cx.new(Editor::new);
        Self {
            editor,
            yank_register: YankRegister::Empty,
            last_change: None,
            pending_operator: None,
            pending_insert: None,
            pending_block_insert: None,
            pending_text_object_modifier: None,
            visual_anchor_cell: None,
        }
    }

    pub fn load_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.load_text(text, cx));
    }

    pub fn snapshot_text(&self, cx: &App) -> String {
        self.editor.read(cx).snapshot_text()
    }

    pub fn update_viewport_size(&mut self, size: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.update_viewport_size(size, cx);
        });
    }

    fn sync_text_input_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let text_input_enabled =
            !AppSettings::global(cx).vim_mode || VimState::global(cx).mode == VimMode::Insert;
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(text_input_enabled, cx);
        });
    }

    fn enter_command_mode(&mut self, cx: &mut Context<Self>) {
        self.clear_pending();
        let state = VimState::global_mut(cx);
        state.mode = VimMode::Command;
        state.command_line = Some(String::new());
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
        });
        cx.notify();
    }

    fn exit_command_mode(&mut self, cx: &mut Context<Self>) {
        let state = VimState::global_mut(cx);
        state.mode = VimMode::Normal;
        state.command_line = None;
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
        });
        cx.notify();
    }

    fn append_command_character(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(command_line) = VimState::global_mut(cx).command_line.as_mut() else {
            return;
        };
        command_line.push_str(text);
        cx.notify();
    }

    fn pop_command_character(&mut self, cx: &mut Context<Self>) {
        let Some(command_line) = VimState::global_mut(cx).command_line.as_mut() else {
            return;
        };
        command_line.pop();
        cx.notify();
    }

    fn execute_command_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command_line = VimState::global(cx)
            .command_line
            .as_deref()
            .unwrap_or_default()
            .trim();

        let action = parse_command_action(command_line);
        self.exit_command_mode(cx);

        match action {
            Some(CommandAction::YankWholeBuffer) => {
                let text = self.editor.read(cx).snapshot_text();
                self.yank_register = YankRegister::CharWise(text.clone());
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            Some(CommandAction::Write) => {
                window.dispatch_action(Box::new(VimCommandWrite), cx);
            }
            Some(CommandAction::Quit) => {
                window.dispatch_action(Box::new(VimCommandQuit), cx);
            }
            Some(CommandAction::DeleteWholeBuffer) => {
                let deleted_text = self.editor.read(cx).snapshot_text();
                self.yank_register = YankRegister::CharWise(deleted_text.clone());
                self.editor.update(cx, |editor, cx| {
                    editor.replace_byte_range(0..deleted_text.len(), "", cx);
                });
            }
            None => {}
        }
    }

    fn handle_command_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => {
                self.exit_command_mode(cx);
                cx.stop_propagation();
                return;
            }
            "enter" => {
                self.execute_command_mode(window, cx);
                cx.stop_propagation();
                return;
            }
            "backspace" => {
                self.pop_command_character(cx);
                cx.stop_propagation();
                return;
            }
            _ => {}
        }

        if let Some(text) = command_character(&event.keystroke) {
            self.append_command_character(text, cx);
            cx.stop_propagation();
        }
    }

    fn handle_vim_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !AppSettings::global(cx).vim_mode {
            return;
        }

        if VimState::global(cx).mode == VimMode::Command {
            self.handle_command_key_down(event, window, cx);
            return;
        }

        if VimState::global(cx).mode != VimMode::Normal {
            return;
        }

        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.function
        {
            return;
        }

        if event.keystroke.key_char.as_deref() == Some(":") {
            self.enter_command_mode(cx);
            cx.stop_propagation();
        }
    }

    fn start_insert_session(
        &mut self,
        kind: InsertKind,
        change_target: Option<RepeatTarget>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_block_insert = None;
        self.pending_insert = Some(PendingInsert {
            kind,
            change_target,
        });
        self.editor.update(cx, |editor, _| {
            editor.begin_transaction();
        });
        VimState::global_mut(cx).mode = VimMode::Insert;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(true, cx);
            editor.collapse_selection_to_cursor_offset(cx);
        });
        cx.notify();
    }

    fn start_block_insert_session(
        &mut self,
        kind: BlockInsertKind,
        row_count: usize,
        column_count: usize,
        delete_selection: bool,
        target_cells: Vec<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_insert = Some(PendingInsert {
            kind: InsertKind::Insert,
            change_target: None,
        });
        self.pending_block_insert = Some(PendingBlockInsert {
            kind,
            row_count,
            column_count,
            delete_selection,
            target_cells,
        });
        self.editor.update(cx, |editor, _| {
            editor.begin_transaction();
        });
        VimState::global_mut(cx).mode = VimMode::Insert;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.clear_block_selection(cx);
            editor.set_text_input_enabled(true, cx);
            editor.collapse_selection_to_cursor_offset(cx);
        });
        cx.notify();
    }

    fn append(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.move_cursor_by(1, cx);
        });
        self.start_insert_session(InsertKind::Append, None, window, cx);
    }

    fn normal_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // TODO 処理フローを見直す
        let leaving_insert = VimState::global(cx).mode == VimMode::Insert;
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.clear_block_selection(cx);
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        if leaving_insert {
            self.finish_insert_session(window, cx);
        }
        cx.notify();
    }

    fn visual_mode(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let anchor = self.editor.read(cx).cursor_cell();
        VimState::global_mut(cx).mode = VimMode::Visual;
        self.visual_anchor_cell = Some(anchor);
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.clear_block_selection(cx);
            editor.set_text_input_enabled(false, cx);
            editor.select_visual_range(anchor, anchor, cx);
        });
        cx.notify();
    }

    fn visual_block_mode(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let anchor = self.editor.read(cx).cursor_cell();
        VimState::global_mut(cx).mode = VimMode::VisualBlock;
        self.visual_anchor_cell = Some(anchor);
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.set_block_selection(anchor, anchor, cx);
        });
        cx.notify();
    }

    fn block_insert_before(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_block_insert_from_selection(BlockInsertKind::Before, false, window, cx);
    }

    fn block_append_after(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_block_insert_from_selection(BlockInsertKind::After, false, window, cx);
    }

    fn change_block_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_block_insert_from_selection(BlockInsertKind::Before, true, window, cx);
    }

    fn start_block_insert_from_selection(
        &mut self,
        kind: BlockInsertKind,
        delete_selection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.visual_anchor_cell else {
            return;
        };
        let (target_cells, register, delete_ranges, row_count, column_count) = {
            let editor = self.editor.read(cx);
            let register = build_block_register(editor, anchor, editor.cursor_cell());
            let target_cells =
                block_insert_target_cells(editor, anchor, editor.cursor_cell(), kind);
            let delete_ranges = if delete_selection {
                block_selection_byte_ranges(editor, anchor, editor.cursor_cell())
            } else {
                Default::default()
            };
            (
                target_cells,
                register.clone(),
                delete_ranges,
                register.row_count,
                register.column_count,
            )
        };
        if target_cells.is_empty() {
            return;
        }

        if delete_selection {
            self.yank_register = YankRegister::BlockWise(register);
            self.editor.update(cx, |editor, cx| {
                editor.begin_transaction();
                for range in delete_ranges.iter().rev() {
                    editor.replace_byte_range(range.clone(), "", cx);
                }
                editor.move_cursor_to_display_cell(target_cells[0], cx);
            });
        } else {
            self.editor.update(cx, |editor, cx| {
                editor.move_cursor_to_display_cell(target_cells[0], cx);
            });
        }
        self.start_block_insert_session(
            kind,
            row_count,
            column_count,
            delete_selection,
            target_cells,
            window,
            cx,
        );
    }

    fn begin_operator(
        &mut self,
        operator: VimOperator,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.set_pending_operator(Some(operator));
        self.set_pending_text_object_modifier(None);
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
        });
        cx.notify();
    }

    fn set_text_object_modifier(
        &mut self,
        modifier: TextObjectModifier,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_operator().is_none() {
            return;
        }
        self.set_pending_text_object_modifier(Some(modifier));
        cx.notify();
    }

    fn delete_char(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if VimState::global(cx).mode == VimMode::VisualBlock {
            self.delete_block_selection(window, cx);
            return;
        }
        let (range, yanked, repeatable) = {
            let editor = self.editor.read(cx);
            if VimState::global(cx).mode == VimMode::Visual
                && !editor.selected_byte_range().is_empty()
            {
                let range = editor.selected_byte_range();
                let yanked = editor.text_in_range(range.clone());
                (range, yanked, None)
            } else {
                let start = editor.cursor_byte_offset();
                let end = editor.offset_after_cursor();
                let range = start..end;
                let yanked = editor.text_in_range(range.clone());
                (range, yanked, Some(RepeatableCommand::DeleteChar))
            }
        };
        if range.is_empty() {
            return;
        }
        self.yank_register = YankRegister::CharWise(yanked);
        self.editor.update(cx, |editor, cx| {
            editor.replace_byte_range(range, "", cx);
        });
        self.last_change = repeatable;
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn delete_block_selection(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(anchor) = self.visual_anchor_cell else {
            return;
        };
        let (ranges, block_register) = {
            let editor = self.editor.read(cx);
            let ranges = block_selection_byte_ranges(editor, anchor, editor.cursor_cell());
            let block_register = build_block_register(editor, anchor, editor.cursor_cell());
            (ranges, block_register)
        };
        if ranges.is_empty() {
            return;
        }

        let row_count = block_register.row_count;
        let column_count = block_register.column_count;
        self.yank_register = YankRegister::BlockWise(block_register);
        self.editor.update(cx, |editor, cx| {
            for range in ranges.iter().rev() {
                editor.replace_byte_range(range.clone(), "", cx);
            }
            editor.clear_block_selection(cx);
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        self.last_change = Some(RepeatableCommand::BlockDelete {
            row_count,
            column_count,
        });
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        cx.notify();
    }

    fn yank_block_selection(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(anchor) = self.visual_anchor_cell else {
            return;
        };
        let (block_register, target_cell) = {
            let editor = self.editor.read(cx);
            let cursor_cell = editor.cursor_cell();
            (
                build_block_register(editor, anchor, cursor_cell),
                top_right_block_cell(anchor, cursor_cell, editor.rows_per_column()),
            )
        };
        self.yank_register = YankRegister::BlockWise(block_register);
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            editor.clear_block_selection(cx);
            editor.move_cursor_to_display_cell(target_cell, cx);
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn paste_after(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.yank_register, YankRegister::BlockWise(_)) {
            self.paste_block(PastePosition::After, window, cx);
            return;
        }
        if let Some((insertion_offset, target_cell, inserted_text)) =
            self.resolve_linewise_paste(PastePosition::After, cx)
        {
            self.editor.update(cx, |editor, cx| {
                editor.replace_byte_range(
                    insertion_offset..insertion_offset,
                    inserted_text.as_str(),
                    cx,
                );
                editor.move_cursor_to_display_cell(target_cell, cx);
                editor.set_text_input_enabled(false, cx);
                editor.collapse_selection_to_cursor_cell(cx);
            });
            VimState::global_mut(cx).mode = VimMode::Normal;
            self.visual_anchor_cell = None;
            self.clear_pending();
            self.last_change = Some(RepeatableCommand::PasteAfter);
            cx.notify();
            return;
        }
        let cursor = {
            let editor = self.editor.read(cx);
            editor.cursor_byte_offset()
        };
        let Some((insertion_offset, inserted_text)) =
            self.resolve_paste(cursor, PastePosition::After, cx)
        else {
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
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.last_change = Some(RepeatableCommand::PasteAfter);
        cx.notify();
    }

    fn paste_before(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.yank_register, YankRegister::BlockWise(_)) {
            self.paste_block(PastePosition::Before, window, cx);
            return;
        }
        if let Some((insertion_offset, target_cell, inserted_text)) =
            self.resolve_linewise_paste(PastePosition::Before, cx)
        {
            self.editor.update(cx, |editor, cx| {
                editor.replace_byte_range(
                    insertion_offset..insertion_offset,
                    inserted_text.as_str(),
                    cx,
                );
                editor.move_cursor_to_display_cell(target_cell, cx);
                editor.set_text_input_enabled(false, cx);
                editor.collapse_selection_to_cursor_cell(cx);
            });
            VimState::global_mut(cx).mode = VimMode::Normal;
            self.visual_anchor_cell = None;
            self.clear_pending();
            self.last_change = Some(RepeatableCommand::PasteBefore);
            cx.notify();
            return;
        }
        let cursor = {
            let editor = self.editor.read(cx);
            editor.cursor_byte_offset()
        };
        let Some((insertion_offset, inserted_text)) =
            self.resolve_paste(cursor, PastePosition::Before, cx)
        else {
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
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.last_change = Some(RepeatableCommand::PasteBefore);
        cx.notify();
    }

    fn paste_block(
        &mut self,
        position: PastePosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (base_cell, rows_per_column, register) = {
            let editor = self.editor.read(cx);
            let base_cell = editor.cursor_cell();
            let YankRegister::BlockWise(register) = &self.yank_register else {
                return;
            };
            (base_cell, editor.rows_per_column(), register.clone())
        };

        let mut inserts = block_paste_operations(base_cell, rows_per_column, &register);
        if inserts.is_empty() {
            return;
        }
        inserts.sort_by(|left, right| right.0.cmp(&left.0));

        self.editor.update(cx, |editor, cx| {
            editor.begin_transaction();
            for (cell_index, text) in &inserts {
                if text.is_empty() {
                    continue;
                }
                editor.move_cursor_to_display_cell(*cell_index, cx);
                let offset = match position {
                    PastePosition::Before => editor.cursor_byte_offset(),
                    PastePosition::After => editor.offset_after_cursor(),
                };
                editor.replace_byte_range(offset..offset, text, cx);
            }
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
            let _ = editor.commit_transaction(cx);
        });
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.last_change = Some(match position {
            PastePosition::Before => RepeatableCommand::PasteBefore,
            PastePosition::After => RepeatableCommand::PasteAfter,
        });
        cx.notify();
    }

    fn undo(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.pending_insert = None;
        self.pending_block_insert = None;
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            let _ = editor.undo(cx);
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn redo(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.pending_insert = None;
        self.pending_block_insert = None;
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.editor.update(cx, |editor, cx| {
            let _ = editor.redo(cx);
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        cx.notify();
    }

    fn repeat_last_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = self.last_change.clone() else {
            return;
        };
        self.execute_repeatable_command(command, window, cx);
    }

    fn move_by_motion(&mut self, motion: MotionKind, window: &mut Window, cx: &mut Context<Self>) {
        let (target, cursor) = {
            let editor = self.editor.read(cx);
            let cursor = editor.cursor_byte_offset();
            (resolve_motion_target(editor.rope(), cursor, motion), cursor)
        };

        if self.pending_operator().is_some() {
            self.apply_motion(motion, cursor, window, cx);
            return;
        }

        let Some(target) = target else {
            return;
        };

        if VimState::global(cx).mode == VimMode::Visual {
            self.editor.update(cx, |editor, cx| {
                editor.move_cursor_to_byte_offset(target, cx);
            });
            self.sync_visual_selection_for_current_cursor(window, cx);
        } else if VimState::global(cx).mode == VimMode::VisualBlock {
            self.editor.update(cx, |editor, cx| {
                editor.move_cursor_to_byte_offset(target, cx);
            });
            self.sync_block_selection_for_current_cursor(window, cx);
        } else {
            self.editor.update(cx, |editor, cx| {
                editor.move_cursor_to_byte_offset(target, cx);
            });
        }
    }

    fn move_by_cells(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let is_visual = VimState::global(cx).mode == VimMode::Visual;
        let is_visual_block = VimState::global(cx).mode == VimMode::VisualBlock;
        self.editor.update(cx, |editor, cx| {
            if is_visual {
                editor.select_cursor_by(delta, cx);
            } else {
                editor.move_cursor_by(delta, cx);
            }
        });
        if is_visual_block {
            self.sync_block_selection_for_current_cursor(window, cx);
        }
    }

    fn move_to_display_cell(
        &mut self,
        cell_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = VimState::global(cx).mode;
        self.editor.update(cx, |editor, cx| {
            editor.move_cursor_to_display_cell(cell_index, cx);
        });

        if mode == VimMode::Visual {
            self.sync_visual_selection_for_current_cursor(window, cx);
        } else if mode == VimMode::VisualBlock {
            self.sync_block_selection_for_current_cursor(window, cx);
        }
    }

    fn move_to_document_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to_display_cell(0, window, cx);
    }

    fn move_to_document_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target_cell = self.editor.read(cx).used_cells();
        self.move_to_display_cell(target_cell, window, cx);
    }

    fn open_next_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target_cell = {
            let editor = self.editor.read(cx);
            let rows_per_column = editor.rows_per_column();
            let used_cells = editor.used_cells();
            let current_cell = if used_cells == 0 {
                0
            } else if editor.cursor_cell() >= used_cells {
                used_cells.saturating_sub(1)
            } else {
                editor.cursor_cell()
            };
            ((current_cell / rows_per_column) + 1) * rows_per_column
        };
        self.editor.update(cx, |editor, cx| {
            editor.move_cursor_to_display_cell(target_cell, cx);
        });
        self.start_insert_session(InsertKind::Insert, None, window, cx);
    }

    fn apply_motion(
        &mut self,
        motion: MotionKind,
        cursor: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(operator) = self.pending_operator() else {
            return;
        };
        let range = {
            let editor = self.editor.read(cx);
            resolve_motion_range(editor.rope(), cursor, motion, operator)
        };
        let Some(range) = range else {
            self.clear_pending();
            cx.notify();
            return;
        };

        self.apply_operator_to_range(
            operator,
            range,
            Some(RepeatTarget::Motion(motion)),
            window,
            cx,
        );
    }

    fn apply_text_object(
        &mut self,
        target: TextObjectTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(operator) = self.pending_operator() else {
            return;
        };
        let Some(modifier) = self.pending_text_object_modifier() else {
            return;
        };

        let range = {
            let editor = self.editor.read(cx);
            let cursor = editor.cursor_byte_offset();
            resolve_text_object_range(editor.rope(), cursor, modifier, target)
        };

        let Some(range) = range else {
            self.clear_pending();
            cx.notify();
            return;
        };

        self.apply_operator_to_range(
            operator,
            range,
            Some(RepeatTarget::TextObject(modifier, target)),
            window,
            cx,
        );
    }

    fn apply_operator_to_range(
        &mut self,
        operator: VimOperator,
        range: Range<usize>,
        repeat_target: Option<RepeatTarget>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match operator {
            VimOperator::Yank => {
                let yanked = self.editor.read(cx).text_in_range(range.clone());
                self.yank_register = if repeat_target == Some(RepeatTarget::Line) {
                    let editor = self.editor.read(cx);
                    let cell_range = current_column_cell_range(
                        editor.cursor_cell(),
                        editor.rows_per_column(),
                        editor.used_cells(),
                    );
                    let leading_rows = if let Some(cell_range) = cell_range {
                        editor
                            .display_cell_for_byte(range.start)
                            .saturating_sub(cell_range.start)
                    } else {
                        0
                    };
                    YankRegister::LineWise {
                        content: yanked,
                        leading_rows,
                    }
                } else {
                    YankRegister::CharWise(yanked)
                };
            }
            VimOperator::Delete | VimOperator::Change => {
                if repeat_target == Some(RepeatTarget::Line) {
                    let yanked = self.editor.read(cx).text_in_range(range.clone());
                    let editor = self.editor.read(cx);
                    let cell_range = current_column_cell_range(
                        editor.cursor_cell(),
                        editor.rows_per_column(),
                        editor.used_cells(),
                    );
                    let leading_rows = if let Some(cell_range) = cell_range {
                        editor
                            .display_cell_for_byte(range.start)
                            .saturating_sub(cell_range.start)
                    } else {
                        0
                    };
                    self.yank_register = YankRegister::LineWise {
                        content: yanked,
                        leading_rows,
                    };
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
                        });
                    }
                    VimOperator::Yank => {}
                }
            }
        }

        self.clear_pending();
        self.visual_anchor_cell = None;
        VimState::global_mut(cx).mode = match operator {
            VimOperator::Delete => VimMode::Normal,
            VimOperator::Change => VimMode::Insert,
            VimOperator::Yank => VimMode::Normal,
        };
        cx.notify();
    }

    fn apply_current_line_operator(
        &mut self,
        operator: VimOperator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (cursor_cell, rows_per_column, used_cells) = {
            let editor = self.editor.read(cx);
            (
                editor.cursor_cell(),
                editor.rows_per_column(),
                editor.used_cells(),
            )
        };
        let Some(cell_range) = current_column_cell_range(cursor_cell, rows_per_column, used_cells)
        else {
            self.clear_pending();
            cx.notify();
            return;
        };
        let range = {
            let editor = self.editor.read(cx);
            editor.byte_offset_for_display_cell(cell_range.start)
                ..editor.byte_offset_for_display_cell(cell_range.end)
        };

        self.apply_operator_to_range(operator, range, Some(RepeatTarget::Line), window, cx);
    }

    fn finish_insert_session(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending_insert) = self.pending_insert.take() else {
            return;
        };
        let pending_block_insert = self.pending_block_insert.take();
        let (committed, inserted_text) = self.editor.update(cx, |editor, cx| {
            let inserted_text = editor
                .active_transaction_inserted_text()
                .unwrap_or_default();
            if let Some(pending_block_insert) = &pending_block_insert {
                if !inserted_text.is_empty() {
                    for &cell_index in pending_block_insert.target_cells.iter().skip(1).rev() {
                        editor.move_cursor_to_display_cell(cell_index, cx);
                        let offset = editor.cursor_byte_offset();
                        editor.replace_byte_range(offset..offset, inserted_text.as_str(), cx);
                    }
                }
                editor.collapse_selection_to_cursor_cell(cx);
            }
            let committed = editor.commit_transaction(cx);
            let inserted_text = editor
                .last_transaction_inserted_text()
                .unwrap_or(inserted_text);
            (committed, inserted_text)
        });
        if !committed {
            return;
        }

        self.last_change = if let Some(pending_block_insert) = pending_block_insert {
            if inserted_text.is_empty() {
                None
            } else if pending_block_insert.delete_selection {
                Some(RepeatableCommand::BlockChange {
                    row_count: pending_block_insert.row_count,
                    column_count: pending_block_insert.column_count,
                    inserted_text,
                })
            } else {
                Some(RepeatableCommand::BlockInsert {
                    kind: pending_block_insert.kind,
                    row_count: pending_block_insert.row_count,
                    inserted_text,
                })
            }
        } else {
            match pending_insert.change_target {
                Some(target) => Some(RepeatableCommand::Change {
                    target,
                    inserted_text,
                }),
                None if inserted_text.is_empty() => None,
                None => Some(RepeatableCommand::Insert {
                    kind: pending_insert.kind,
                    inserted_text,
                }),
            }
        };
    }

    fn execute_repeatable_command(
        &mut self,
        command: RepeatableCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            RepeatableCommand::DeleteChar => self.delete_char(window, cx),
            RepeatableCommand::Delete(target) => {
                self.execute_repeat_target(target, None, window, cx)
            }
            RepeatableCommand::BlockDelete {
                row_count,
                column_count,
            } => self.execute_block_delete(row_count, column_count, window, cx),
            RepeatableCommand::Change {
                target,
                inserted_text,
            } => self.execute_repeat_target(target, Some(inserted_text), window, cx),
            RepeatableCommand::BlockChange {
                row_count,
                column_count,
                inserted_text,
            } => self.execute_block_change(row_count, column_count, inserted_text, window, cx),
            RepeatableCommand::PasteAfter => self.paste_after(window, cx),
            RepeatableCommand::PasteBefore => self.paste_before(window, cx),
            RepeatableCommand::Insert {
                kind,
                inserted_text,
            } => self.execute_repeat_insert(kind, inserted_text, window, cx),
            RepeatableCommand::BlockInsert {
                kind,
                row_count,
                inserted_text,
            } => self.execute_block_insert(kind, row_count, inserted_text, window, cx),
        }
    }

    fn execute_block_delete(
        &mut self,
        row_count: usize,
        column_count: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (ranges, block_register) = {
            let editor = self.editor.read(cx);
            let ranges = block_byte_ranges_from_cursor(
                editor,
                editor.cursor_cell(),
                row_count,
                column_count,
            );
            let block_register = build_block_register_from_cursor(
                editor,
                editor.cursor_cell(),
                row_count,
                column_count,
            );
            (ranges, block_register)
        };
        if ranges.is_empty() {
            return;
        }
        self.yank_register = YankRegister::BlockWise(block_register);
        self.editor.update(cx, |editor, cx| {
            for range in ranges.iter().rev() {
                editor.replace_byte_range(range.clone(), "", cx);
            }
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
        });
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.last_change = Some(RepeatableCommand::BlockDelete {
            row_count,
            column_count,
        });
        cx.notify();
    }

    fn execute_block_change(
        &mut self,
        row_count: usize,
        column_count: usize,
        inserted_text: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (ranges, target_cells) = {
            let editor = self.editor.read(cx);
            (
                block_byte_ranges_from_cursor(
                    editor,
                    editor.cursor_cell(),
                    row_count,
                    column_count,
                ),
                block_insert_target_cells_from_cursor(
                    editor,
                    editor.cursor_cell(),
                    row_count,
                    BlockInsertKind::Before,
                ),
            )
        };
        if target_cells.is_empty() {
            return;
        }
        self.editor.update(cx, |editor, cx| {
            editor.begin_transaction();
            for range in ranges.iter().rev() {
                editor.replace_byte_range(range.clone(), "", cx);
            }
            for &cell_index in target_cells.iter().rev() {
                editor.move_cursor_to_display_cell(cell_index, cx);
                let offset = editor.cursor_byte_offset();
                editor.replace_byte_range(offset..offset, inserted_text.as_str(), cx);
            }
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
            let _ = editor.commit_transaction(cx);
        });
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.last_change = Some(RepeatableCommand::BlockChange {
            row_count,
            column_count,
            inserted_text,
        });
        cx.notify();
    }

    fn execute_block_insert(
        &mut self,
        kind: BlockInsertKind,
        row_count: usize,
        inserted_text: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_cells = {
            let editor = self.editor.read(cx);
            block_insert_target_cells_from_cursor(editor, editor.cursor_cell(), row_count, kind)
        };
        if target_cells.is_empty() || inserted_text.is_empty() {
            return;
        }
        self.editor.update(cx, |editor, cx| {
            editor.begin_transaction();
            for &cell_index in target_cells.iter().rev() {
                editor.move_cursor_to_display_cell(cell_index, cx);
                let offset = editor.cursor_byte_offset();
                editor.replace_byte_range(offset..offset, inserted_text.as_str(), cx);
            }
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
            let _ = editor.commit_transaction(cx);
        });
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.last_change = Some(RepeatableCommand::BlockInsert {
            kind,
            row_count,
            inserted_text,
        });
        cx.notify();
    }

    fn execute_repeat_target(
        &mut self,
        target: RepeatTarget,
        inserted_text: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let resolved_range = {
            let editor = self.editor.read(cx);
            resolve_repeat_target_range(
                editor.rope(),
                editor.cursor_byte_offset(),
                target,
                inserted_text.is_some(),
            )
        };
        let range = if target == RepeatTarget::Line {
            let (cursor_cell, rows_per_column, used_cells) = {
                let editor = self.editor.read(cx);
                (
                    editor.cursor_cell(),
                    editor.rows_per_column(),
                    editor.used_cells(),
                )
            };
            let Some(cell_range) =
                current_column_cell_range(cursor_cell, rows_per_column, used_cells)
            else {
                return;
            };
            let editor = self.editor.read(cx);
            editor.byte_offset_for_display_cell(cell_range.start)
                ..editor.byte_offset_for_display_cell(cell_range.end)
        } else {
            let Some(range) = resolved_range else {
                return;
            };
            range
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
            VimState::global_mut(cx).mode = VimMode::Normal;
            self.visual_anchor_cell = None;
            self.clear_pending();
            self.last_change = Some(RepeatableCommand::Change {
                target,
                inserted_text,
            });
        } else {
            self.apply_operator_to_range(VimOperator::Delete, range, Some(target), window, cx);
        }
    }

    fn execute_repeat_insert(
        &mut self,
        kind: InsertKind,
        inserted_text: String,
        _window: &mut Window,
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
        VimState::global_mut(cx).mode = VimMode::Normal;
        self.visual_anchor_cell = None;
        self.clear_pending();
        self.last_change = Some(RepeatableCommand::Insert {
            kind,
            inserted_text,
        });
        cx.notify();
    }

    fn resolve_paste(
        &self,
        cursor_byte_offset: usize,
        position: PastePosition,
        cx: &App,
    ) -> Option<(usize, String)> {
        match &self.yank_register {
            YankRegister::Empty => None,
            YankRegister::CharWise(content) => {
                let insertion_offset = match position {
                    PastePosition::Before => cursor_byte_offset,
                    PastePosition::After => self.editor.read(cx).offset_after_cursor(),
                };
                Some((insertion_offset, content.clone()))
            }
            YankRegister::LineWise { .. } => None,
            YankRegister::BlockWise(_) => None,
        }
    }

    fn resolve_linewise_paste(
        &self,
        position: PastePosition,
        cx: &App,
    ) -> Option<(usize, usize, String)> {
        println!("Resolving linewise paste for position: {:?}", position);
        let YankRegister::LineWise {
            content,
            leading_rows,
        } = &self.yank_register
        else {
            return None;
        };
        println!(
            "Linewise paste content: '{}', leading_rows: {}",
            content, leading_rows
        );
        let editor = self.editor.read(cx);
        let cell_range = current_column_cell_range(
            editor.cursor_cell(),
            editor.rows_per_column(),
            editor.used_cells(),
        );
        println!("Current cell range: {:?}", cell_range);
        let row_size = editor.rows_per_column();
        println!("Row size: {}", row_size);
        let target_row = editor.cursor_cell().saturating_sub(
            cell_range
                .as_ref()
                .map(|range| (range.start % row_size + 1) * row_size)
                .unwrap_or_default(),
        );
        println!("Target row for paste: {}", target_row);
        let (target_column_start, insertion_offset) = match (position, cell_range) {
            (_, None) => (0, 0),
            (PastePosition::Before, Some(cell_range)) => (
                cell_range.start,
                editor.byte_offset_for_display_cell(cell_range.start),
            ),
            (PastePosition::After, Some(cell_range)) => (
                cell_range.end,
                editor.byte_offset_for_display_cell(cell_range.end),
            ),
        };
        let inserted_text = format!(
            "{}{}",
            BLANK_CELL.to_string().repeat(*leading_rows),
            content
        );
        let target_cell = target_column_start + target_row;
        Some((insertion_offset, target_cell, inserted_text))
    }

    fn sync_visual_selection_for_current_cursor(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.visual_anchor_cell else {
            return;
        };
        let cursor = self.editor.read(cx).cursor_cell();
        self.editor.update(cx, |editor, cx| {
            editor.select_visual_range(anchor, cursor, cx);
        });
    }

    fn sync_block_selection_for_current_cursor(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.visual_anchor_cell else {
            return;
        };
        let cursor = self.editor.read(cx).cursor_cell();
        self.editor.update(cx, |editor, cx| {
            editor.set_block_selection(anchor, cursor, cx);
        });
    }

    fn vim_enter_insert_mode(
        &mut self,
        _: &VimEnterInsertMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_insert_session(InsertKind::Insert, None, window, cx);
    }

    fn vim_append(&mut self, _: &VimAppend, window: &mut Window, cx: &mut Context<Self>) {
        self.append(window, cx);
    }

    fn vim_normal_mode(&mut self, _: &VimNormalMode, window: &mut Window, cx: &mut Context<Self>) {
        self.normal_mode(window, cx);
    }

    fn vim_visual_mode(&mut self, _: &VimVisualMode, window: &mut Window, cx: &mut Context<Self>) {
        self.visual_mode(window, cx);
    }

    fn vim_visual_block_mode(
        &mut self,
        _: &VimVisualBlockMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.visual_block_mode(window, cx);
    }

    fn vim_block_insert_before(
        &mut self,
        _: &VimBlockInsertBefore,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.block_insert_before(window, cx);
    }

    fn vim_block_append_after(
        &mut self,
        _: &VimBlockAppendAfter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.block_append_after(window, cx);
    }

    fn vim_delete_char(&mut self, _: &VimDeleteChar, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_char(window, cx);
    }

    fn vim_delete_operator(
        &mut self,
        _: &VimDeleteOperator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if VimState::global(cx).mode == VimMode::VisualBlock {
            self.delete_block_selection(window, cx);
            return;
        }
        if VimState::global(cx).mode == VimMode::Visual {
            self.delete_char(window, cx);
            return;
        }
        if self.pending_operator() == Some(VimOperator::Delete) {
            self.apply_current_line_operator(VimOperator::Delete, window, cx);
            return;
        }
        self.begin_operator(VimOperator::Delete, window, cx);
    }

    fn vim_change_operator(
        &mut self,
        _: &VimChangeOperator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if VimState::global(cx).mode == VimMode::VisualBlock {
            self.change_block_selection(window, cx);
            return;
        }
        if self.pending_operator() == Some(VimOperator::Change) {
            self.apply_current_line_operator(VimOperator::Change, window, cx);
            return;
        }
        if VimState::global(cx).mode == VimMode::Visual {
            // return;
        }
        self.begin_operator(VimOperator::Change, window, cx);
    }

    fn vim_yank_operator(
        &mut self,
        _: &VimYankOperator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if VimState::global(cx).mode == VimMode::VisualBlock {
            self.yank_block_selection(window, cx);
            return;
        }
        if VimState::global(cx).mode == VimMode::Visual {
            let (range, target_cell) = {
                let editor = self.editor.read(cx);
                (
                    editor.selected_byte_range(),
                    self.visual_anchor_cell
                        .map(|anchor| anchor.min(editor.cursor_cell()))
                        .unwrap_or_else(|| editor.cursor_cell()),
                )
            };
            if range.is_empty() {
                return;
            }
            self.yank_register = YankRegister::CharWise(self.editor.read(cx).text_in_range(range));
            VimState::global_mut(cx).mode = VimMode::Normal;
            self.visual_anchor_cell = None;
            self.clear_pending();
            self.editor.update(cx, |editor, cx| {
                editor.clear_block_selection(cx);
                editor.move_cursor_to_display_cell(target_cell, cx);
                editor.set_text_input_enabled(false, cx);
                editor.collapse_selection_to_cursor_cell(cx);
            });
            cx.notify();
            return;
        }
        if self.pending_operator() == Some(VimOperator::Yank) {
            self.apply_current_line_operator(VimOperator::Yank, window, cx);
            return;
        }
        self.begin_operator(VimOperator::Yank, window, cx);
    }

    fn vim_text_object_inner(
        &mut self,
        _: &VimTextObjectInner,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_text_object_modifier(TextObjectModifier::Inner, window, cx);
    }

    fn vim_text_object_around(
        &mut self,
        _: &VimTextObjectAround,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_text_object_modifier(TextObjectModifier::Around, window, cx);
    }

    fn vim_text_object_word(
        &mut self,
        _: &VimTextObjectWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::Word, window, cx);
    }

    fn vim_text_object_big_word(
        &mut self,
        _: &VimTextObjectBigWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::BigWord, window, cx);
    }

    fn vim_text_object_double_quote(
        &mut self,
        _: &VimTextObjectDoubleQuote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::DoubleQuote, window, cx);
    }

    fn vim_text_object_single_quote(
        &mut self,
        _: &VimTextObjectSingleQuote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::SingleQuote, window, cx);
    }

    fn vim_text_object_paren(
        &mut self,
        _: &VimTextObjectParen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::Paren, window, cx);
    }

    fn vim_text_object_bracket(
        &mut self,
        _: &VimTextObjectBracket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_text_object(TextObjectTarget::Bracket, window, cx);
    }

    fn vim_move_word_forward(
        &mut self,
        _: &VimMoveWordForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::WordForward, window, cx);
    }

    fn vim_move_big_word_forward(
        &mut self,
        _: &VimMoveBigWordForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::BigWordForward, window, cx);
    }

    fn vim_move_word_end_forward(
        &mut self,
        _: &VimMoveWordEndForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::WordEndForward, window, cx);
    }

    fn vim_move_word_backward(
        &mut self,
        _: &VimMoveWordBackward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::WordBackward, window, cx);
    }

    fn vim_move_big_word_backward(
        &mut self,
        _: &VimMoveBigWordBackward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_by_motion(MotionKind::BigWordBackward, window, cx);
    }

    fn vim_move_document_start(
        &mut self,
        _: &VimMoveDocumentStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_document_start(window, cx);
    }

    fn vim_move_document_end(
        &mut self,
        _: &VimMoveDocumentEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_document_end(window, cx);
    }

    fn vim_move_left(&mut self, _: &VimMoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        let rows_per_column = self.editor.read(cx).rows_per_column() as isize;
        self.move_by_cells(rows_per_column, window, cx);
    }

    fn vim_move_down(&mut self, _: &VimMoveDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_by_cells(1, window, cx);
    }

    fn vim_move_up(&mut self, _: &VimMoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_by_cells(-1, window, cx);
    }

    fn vim_move_right(&mut self, _: &VimMoveRight, window: &mut Window, cx: &mut Context<Self>) {
        let rows_per_column = self.editor.read(cx).rows_per_column() as isize;
        self.move_by_cells(-rows_per_column, window, cx);
    }

    fn vim_paste_after(&mut self, _: &VimPasteAfter, window: &mut Window, cx: &mut Context<Self>) {
        self.paste_after(window, cx);
    }

    fn vim_paste_before(
        &mut self,
        _: &VimPasteBefore,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.paste_before(window, cx);
    }

    fn vim_open_next_column(
        &mut self,
        _: &VimOpenNextColumn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_next_column(window, cx);
    }

    fn vim_undo(&mut self, _: &VimUndo, window: &mut Window, cx: &mut Context<Self>) {
        self.undo(window, cx);
    }

    fn vim_redo(&mut self, _: &VimRedo, window: &mut Window, cx: &mut Context<Self>) {
        self.redo(window, cx);
    }

    fn vim_repeat_last_change(
        &mut self,
        _: &VimRepeatLastChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.repeat_last_change(window, cx);
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

    fn key_context(&self, _window: &mut Window, cx: &mut Context<Self>) -> &'static str {
        match (
            VimState::global(cx).mode,
            self.pending_operator,
            self.pending_text_object_modifier,
        ) {
            (VimMode::Insert, _, _) => "Genko vim_mode=insert",
            (VimMode::Visual, _, _) => "Genko vim_mode=visual",
            (VimMode::VisualBlock, _, _) => "Genko vim_mode=visual_block",
            (VimMode::Command, _, _) => "Genko vim_mode=command",
            (VimMode::Normal, None, _) => "Genko vim_mode=normal",
            (VimMode::Normal, Some(operator), modifier) => operator_key_context(operator, modifier),
        }
    }
}

fn parse_command_action(command_line: &str) -> Option<CommandAction> {
    let command_line = command_line.trim();
    match command_line {
        "%y" | "%yank" => Some(CommandAction::YankWholeBuffer),
        "w" => Some(CommandAction::Write),
        "q" => Some(CommandAction::Quit),
        "%d" | "%delete" => Some(CommandAction::DeleteWholeBuffer),
        _ => None,
    }
}

fn command_character(keystroke: &gpui::Keystroke) -> Option<&str> {
    if keystroke.modifiers.control
        || keystroke.modifiers.alt
        || keystroke.modifiers.platform
        || keystroke.modifiers.function
    {
        return None;
    }

    let key = keystroke.key.as_str();
    if matches!(key, "escape" | "enter" | "backspace" | "tab" | "delete") {
        return None;
    }

    keystroke
        .key_char
        .as_deref()
        .or(Some(key))
        .filter(|text| !text.is_empty())
}

impl Render for Vim {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.sync_text_input_enabled(window, cx);

        div()
            .track_focus(&self.editor.focus_handle(cx))
            .key_context(if AppSettings::global(cx).vim_mode {
                self.key_context(window, cx)
            } else {
                "Genko vim_mode=disabled"
            })
            .on_key_down(cx.listener(Self::handle_vim_key_down))
            .on_action(cx.listener(Self::vim_enter_insert_mode))
            .on_action(cx.listener(Self::vim_append))
            .on_action(cx.listener(Self::vim_normal_mode))
            .on_action(cx.listener(Self::vim_visual_mode))
            .on_action(cx.listener(Self::vim_visual_block_mode))
            .on_action(cx.listener(Self::vim_block_insert_before))
            .on_action(cx.listener(Self::vim_block_append_after))
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
            .on_action(cx.listener(Self::vim_move_word_backward))
            .on_action(cx.listener(Self::vim_move_big_word_backward))
            .on_action(cx.listener(Self::vim_move_document_start))
            .on_action(cx.listener(Self::vim_move_document_end))
            .on_action(cx.listener(Self::vim_move_left))
            .on_action(cx.listener(Self::vim_move_down))
            .on_action(cx.listener(Self::vim_move_up))
            .on_action(cx.listener(Self::vim_move_right))
            .on_action(cx.listener(Self::vim_paste_after))
            .on_action(cx.listener(Self::vim_paste_before))
            .on_action(cx.listener(Self::vim_open_next_column))
            .on_action(cx.listener(Self::vim_undo))
            .on_action(cx.listener(Self::vim_redo))
            .on_action(cx.listener(Self::vim_repeat_last_change))
            .child(self.editor.clone())
    }
}

impl Focusable for Vim {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

pub struct VimModeLabel {}

impl VimModeLabel {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {}
    }

    fn mode_label(&self, _window: &mut Window, cx: &mut Context<Self>) -> String {
        if let Some(command_line) = VimState::global(cx).command_line.as_ref() {
            return format!(":{}", command_line);
        }

        match VimState::global(cx).mode {
            VimMode::Normal => "-- NORMAL --".to_string(),
            VimMode::Insert => "-- INSERT --".to_string(),
            VimMode::Visual => "-- VISUAL --".to_string(),
            VimMode::VisualBlock => "-- VISUAL BLOCK --".to_string(),
            VimMode::Command => ":".to_string(),
        }
    }
}

impl Render for VimModeLabel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .right_auto()
            .py_1()
            .text_color(Theme::global(cx).black())
            .border_1()
            .rounded_sm()
            .child(self.mode_label(window, cx))
    }
}
