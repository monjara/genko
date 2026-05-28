use std::ops::Range;

use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, IntoElement, Pixels,
    Render, Window,
};
use richtext::{EpubMetadata, RichDocument};

use crate::vim::VimController;

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

    pub fn load_rich_document(&mut self, document: RichDocument, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.load_rich_document(document, cx);
        });
    }

    pub fn snapshot_text(&self, cx: &App) -> String {
        self.vim_controller.read(cx).snapshot_text(cx)
    }

    pub fn selected_byte_range(&self, cx: &App) -> Range<usize> {
        self.vim_controller.read(cx).selected_byte_range(cx)
    }

    pub fn selection_bounds(&self, cx: &App) -> Option<Bounds<Pixels>> {
        self.vim_controller.read(cx).selection_bounds(cx)
    }

    pub fn update_viewport_size(
        &mut self,
        size: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.update_viewport_size(size, cx);
        });
    }

    pub fn has_richtext_document(&self, cx: &App) -> bool {
        self.vim_controller.read(cx).has_richtext_document(cx)
    }

    pub fn richtext_document(&mut self, cx: &mut Context<Self>) -> Option<RichDocument> {
        self.vim_controller
            .update(cx, |vim_controller, cx| vim_controller.richtext_document(cx))
    }

    pub fn current_epub_metadata(&mut self, cx: &mut Context<Self>) -> Option<EpubMetadata> {
        self.vim_controller
            .update(cx, |vim_controller, cx| vim_controller.current_epub_metadata(cx))
    }

    pub fn first_heading_title(&mut self, cx: &mut Context<Self>) -> Option<String> {
        self.vim_controller
            .update(cx, |vim_controller, cx| vim_controller.first_heading_title(cx))
    }

    pub fn set_epub_metadata(&mut self, metadata: EpubMetadata, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.set_epub_metadata(metadata, cx);
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
