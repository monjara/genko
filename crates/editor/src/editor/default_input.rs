use gpui::{
    Action, App, Context, CursorStyle, InteractiveElement, IntoElement, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, ScrollWheelEvent, Styled, Window,
    div,
};
use settings::AppSettings;

use super::{
    Backspace, ClearSelection, Copy, Cut, Delete, Down, Editor, End, Enter, Home, Left, Paste,
    Redo, Right, SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, ShowCharacterPalette,
    Undo, Up, invalidate_ime_position,
};
use crate::editor_canvas::EditorCanvas;
use crate::vim::{VimMode, VimNormalMode, VimState};

pub(super) fn init(cx: &mut App) {
    const EDITOR_CONTEXT: Option<&str> = Some("vim_mode == insert || vim_mode == disabled");
    fn binding<A: Action>(cx: &App, id: &str, action: A, context: Option<&str>) -> KeyBinding {
        let keystroke = AppSettings::global(cx).keymap_keystroke(id);
        KeyBinding::new(keystroke.as_ref(), action, context)
    }

    cx.bind_keys([
        binding(cx, "editor.backspace", Backspace, EDITOR_CONTEXT),
        binding(cx, "editor.delete", Delete, EDITOR_CONTEXT),
        binding(cx, "editor.up", Up, EDITOR_CONTEXT),
        binding(cx, "editor.down", Down, EDITOR_CONTEXT),
        binding(cx, "editor.left", Left, EDITOR_CONTEXT),
        binding(cx, "editor.right", Right, EDITOR_CONTEXT),
        binding(cx, "editor.select_up", SelectUp, EDITOR_CONTEXT),
        binding(cx, "editor.select_down", SelectDown, EDITOR_CONTEXT),
        binding(cx, "editor.select_left", SelectLeft, EDITOR_CONTEXT),
        binding(cx, "editor.select_right", SelectRight, EDITOR_CONTEXT),
        binding(cx, "editor.select_all.mac", SelectAll, EDITOR_CONTEXT),
        binding(cx, "editor.select_all.ctrl", SelectAll, EDITOR_CONTEXT),
        binding(cx, "editor.paste.mac", Paste, EDITOR_CONTEXT),
        binding(cx, "editor.paste.ctrl", Paste, EDITOR_CONTEXT),
        binding(cx, "editor.copy.mac", Copy, EDITOR_CONTEXT),
        binding(cx, "editor.copy.ctrl", Copy, EDITOR_CONTEXT),
        binding(cx, "editor.cut.mac", Cut, EDITOR_CONTEXT),
        binding(cx, "editor.cut.ctrl", Cut, EDITOR_CONTEXT),
        binding(cx, "editor.undo.mac", Undo, EDITOR_CONTEXT),
        binding(cx, "editor.undo.ctrl", Undo, EDITOR_CONTEXT),
        binding(cx, "editor.redo.mac", Redo, EDITOR_CONTEXT),
        binding(cx, "editor.redo.ctrl", Redo, EDITOR_CONTEXT),
        binding(cx, "editor.enter", Enter, EDITOR_CONTEXT),
        binding(cx, "editor.clear_selection", ClearSelection, EDITOR_CONTEXT),
        binding(cx, "editor.home", Home, EDITOR_CONTEXT),
        binding(cx, "editor.end", End, EDITOR_CONTEXT),
        binding(
            cx,
            "editor.show_character_palette",
            ShowCharacterPalette,
            EDITOR_CONTEXT,
        ),
    ]);
}

fn backspace(editor: &mut Editor, _: &Backspace, window: &mut Window, cx: &mut Context<Editor>) {
    editor.delete_backward_command(window, cx);
}

fn delete(editor: &mut Editor, _: &Delete, window: &mut Window, cx: &mut Context<Editor>) {
    editor.delete_forward_command(cx);
    invalidate_ime_position(window);
}

fn up(editor: &mut Editor, _: &Up, window: &mut Window, cx: &mut Context<Editor>) {
    editor.move_cursor_by_cells_command(-1, false, cx);
    invalidate_ime_position(window);
}

fn down(editor: &mut Editor, _: &Down, window: &mut Window, cx: &mut Context<Editor>) {
    editor.move_cursor_by_cells_command(1, false, cx);
    invalidate_ime_position(window);
}

fn left(editor: &mut Editor, _: &Left, window: &mut Window, cx: &mut Context<Editor>) {
    editor.move_cursor_left_cell_command(false, cx);
    invalidate_ime_position(window);
}

fn right(editor: &mut Editor, _: &Right, window: &mut Window, cx: &mut Context<Editor>) {
    editor.move_cursor_right_cell_command(false, cx);
    invalidate_ime_position(window);
}

fn select_up(
    editor: &mut Editor,
    _: &SelectUp,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.move_cursor_by_cells_command(-1, true, cx);
    invalidate_ime_position(window);
}

fn select_down(
    editor: &mut Editor,
    _: &SelectDown,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.move_cursor_by_cells_command(1, true, cx);
    invalidate_ime_position(window);
}

fn select_left(
    editor: &mut Editor,
    _: &SelectLeft,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.move_cursor_left_cell_command(true, cx);
    invalidate_ime_position(window);
}

fn select_right(
    editor: &mut Editor,
    _: &SelectRight,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.move_cursor_right_cell_command(true, cx);
    invalidate_ime_position(window);
}

fn select_all(
    editor: &mut Editor,
    _: &SelectAll,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.select_all_command(window, cx);
}

fn home(editor: &mut Editor, _: &Home, window: &mut Window, cx: &mut Context<Editor>) {
    editor.move_cursor_to_document_start_command(cx);
    invalidate_ime_position(window);
}

fn end(editor: &mut Editor, _: &End, window: &mut Window, cx: &mut Context<Editor>) {
    editor.move_cursor_to_document_end_command(cx);
    invalidate_ime_position(window);
}

fn paste(editor: &mut Editor, _: &Paste, window: &mut Window, cx: &mut Context<Editor>) {
    editor.paste_command(window, cx);
}

fn enter(editor: &mut Editor, _: &Enter, window: &mut Window, cx: &mut Context<Editor>) {
    editor.insert_newline_command(window, cx);
}

fn clear_selection_action(
    editor: &mut Editor,
    _: &ClearSelection,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    if AppSettings::global(cx).vim_mode && VimState::global(cx).mode == VimMode::Insert {
        window.dispatch_action(Box::new(VimNormalMode), cx);
        return;
    }

    editor.clear_selection_command(window, cx);
}

fn copy(editor: &mut Editor, _: &Copy, _window: &mut Window, cx: &mut Context<Editor>) {
    editor.copy_command(cx);
}

fn cut(editor: &mut Editor, _: &Cut, window: &mut Window, cx: &mut Context<Editor>) {
    editor.cut_command(window, cx);
}

fn show_character_palette(
    _: &mut Editor,
    _: &ShowCharacterPalette,
    window: &mut Window,
    _: &mut Context<Editor>,
) {
    window.show_character_palette();
}

fn undo_action(editor: &mut Editor, _: &Undo, window: &mut Window, cx: &mut Context<Editor>) {
    editor.undo_command(window, cx);
}

fn redo_action(editor: &mut Editor, _: &Redo, window: &mut Window, cx: &mut Context<Editor>) {
    editor.redo_command(window, cx);
}

fn on_board_mouse_down(
    editor: &mut Editor,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.mouse_selection_start_command(event.position, event.modifiers.shift, window, cx);
}

fn on_board_mouse_up(
    editor: &mut Editor,
    event: &MouseUpEvent,
    _window: &mut Window,
    _cx: &mut Context<Editor>,
) {
    editor.mouse_selection_end_command(event);
}

fn on_board_mouse_move(
    editor: &mut Editor,
    event: &MouseMoveEvent,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.mouse_selection_update_command(event, window, cx);
}

fn on_scroll_wheel(
    editor: &mut Editor,
    event: &ScrollWheelEvent,
    _window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.scroll_wheel_command(event, cx);
}

pub(super) fn render(editor: &mut Editor, cx: &mut Context<Editor>) -> impl IntoElement {
    div()
        .track_focus(&editor.focus_handle)
        .key_context("Soukou")
        .on_action(cx.listener(backspace))
        .on_action(cx.listener(delete))
        .on_action(cx.listener(up))
        .on_action(cx.listener(down))
        .on_action(cx.listener(left))
        .on_action(cx.listener(right))
        .on_action(cx.listener(select_up))
        .on_action(cx.listener(select_down))
        .on_action(cx.listener(select_left))
        .on_action(cx.listener(select_right))
        .on_action(cx.listener(select_all))
        .on_action(cx.listener(home))
        .on_action(cx.listener(end))
        .on_action(cx.listener(paste))
        .on_action(cx.listener(cut))
        .on_action(cx.listener(copy))
        .on_action(cx.listener(undo_action))
        .on_action(cx.listener(redo_action))
        .on_action(cx.listener(enter))
        .on_action(cx.listener(clear_selection_action))
        .on_action(cx.listener(show_character_palette))
        .on_mouse_down(MouseButton::Left, cx.listener(on_board_mouse_down))
        .on_mouse_up(MouseButton::Left, cx.listener(on_board_mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(on_board_mouse_up))
        .on_mouse_move(cx.listener(on_board_mouse_move))
        .on_scroll_wheel(cx.listener(on_scroll_wheel))
        .cursor(CursorStyle::IBeam)
        .child(EditorCanvas::new(cx.entity()))
}
