use std::ops::Range;

use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, IntoElement, Pixels, Render,
    Window,
};
use rich_text::RichTextDocumentMeta;

use crate::editor::{PlainTextLoadSettings, PreparedPlainText};
use crate::vim::VimController;

pub struct EditorController {
    vim_controller: Entity<VimController>,
}

impl EditorController {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let vim_controller = cx.new(VimController::new);
        Self { vim_controller }
    }

    pub fn load_plain_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.vim_controller
            .update(cx, |vim_controller, cx| vim_controller.load_text(text, cx));
    }

    pub fn plain_text_load_settings(&self, cx: &App) -> PlainTextLoadSettings {
        self.vim_controller.read(cx).plain_text_load_settings(cx)
    }

    pub fn load_prepared_plain_text(
        &mut self,
        prepared: PreparedPlainText,
        cx: &mut Context<Self>,
    ) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.load_prepared_text(prepared, cx);
        });
    }

    pub fn snapshot_text(&self, cx: &App) -> String {
        self.vim_controller.read(cx).snapshot_text(cx)
    }

    pub fn rich_text_meta(&self, cx: &App) -> RichTextDocumentMeta {
        self.vim_controller.read(cx).rich_text_meta(cx)
    }

    pub fn selected_byte_range(&self, cx: &App) -> Range<usize> {
        self.vim_controller.read(cx).selected_byte_range(cx)
    }

    pub fn replace_byte_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        cx: &mut Context<Self>,
    ) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.replace_byte_range(range, new_text, cx);
        });
    }

    pub fn set_rich_text_meta(
        &mut self,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.set_rich_text_meta(rich_text_meta, cx);
        });
    }

    pub fn selection_bounds(&self, cx: &App) -> Option<Bounds<Pixels>> {
        self.vim_controller.read(cx).selection_bounds(cx)
    }

    pub fn update_viewport_size(&mut self, size: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.update_viewport_size(size, cx);
        });
    }
}

impl Render for EditorController {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.vim_controller.clone()
    }
}

impl Focusable for EditorController {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.vim_controller.focus_handle(cx)
    }
}
