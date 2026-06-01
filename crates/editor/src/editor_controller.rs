use std::ops::Range;

use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, IntoElement, Pixels, Render,
    Window,
};
use rich_text::RichTextDocumentMeta;

use crate::{editor::AppliedEditBatch, vim::VimController};

pub struct EditorController {
    vim_controller: Entity<VimController>,
}

impl EditorController {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            vim_controller: cx.new(VimController::new),
        }
    }

    pub fn load_plain_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.vim_controller
            .update(cx, |vim_controller, cx| vim_controller.load_text(text, cx));
    }

    pub fn snapshot_text(&self, cx: &App) -> String {
        self.vim_controller.read(cx).snapshot_text(cx)
    }

    pub fn draft_revision(&self, cx: &App) -> u64 {
        self.vim_controller.read(cx).draft_revision(cx)
    }

    pub fn last_applied_edit_batch(&self, cx: &App) -> Option<AppliedEditBatch> {
        self.vim_controller.read(cx).last_applied_edit_batch(cx)
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
