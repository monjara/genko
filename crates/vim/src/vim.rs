use editor::Editor;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, ParentElement, Render, Window, actions, div,
    prelude::*,
};

actions!(
    genko,
    [
        VimEnterInsertMode,
        VimAppend,
        VimNormalMode,
        VimVisualMode,
        VimDeleteChar,
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VimState {
    mode: VimMode,
    visual_anchor_cell: Option<usize>,
}

impl VimState {
    pub fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            visual_anchor_cell: None,
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

    fn key_context(&self) -> &'static str {
        match self.mode {
            VimMode::Normal => "Genko vim_mode=normal",
            VimMode::Insert => "Genko vim_mode=insert",
            VimMode::Visual => "Genko vim_mode=visual",
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
            gpui::KeyBinding::new("i", VimEnterInsertMode, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("a", VimAppend, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("escape", VimNormalMode, Some("Genko && vim_mode == insert")),
            gpui::KeyBinding::new("escape", VimNormalMode, Some("Genko && vim_mode == visual")),
            gpui::KeyBinding::new("v", VimVisualMode, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("v", VimNormalMode, Some("Genko && vim_mode == visual")),
            gpui::KeyBinding::new("h", editor::Right, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("j", editor::Down, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("k", editor::Up, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("l", editor::Left, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("h", editor::Right, Some("Genko && vim_mode == visual")),
            gpui::KeyBinding::new("j", editor::Down, Some("Genko && vim_mode == visual")),
            gpui::KeyBinding::new("k", editor::Up, Some("Genko && vim_mode == visual")),
            gpui::KeyBinding::new("l", editor::Left, Some("Genko && vim_mode == visual")),
            gpui::KeyBinding::new("x", VimDeleteChar, Some("Genko && vim_mode == normal")),
            gpui::KeyBinding::new("x", VimDeleteChar, Some("Genko && vim_mode == visual")),
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
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.select_visual_range(anchor, anchor, cx);
        });
        cx.notify();
    }

    fn delete_char(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.delete_forward_command(cx);
        });
        self.state.set_mode(VimMode::Normal);
        self.state.set_visual_anchor_cell(None);
        self.editor.update(cx, |editor, cx| {
            editor.set_text_input_enabled(false, cx);
            editor.collapse_selection_to_cursor_cell(cx);
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
