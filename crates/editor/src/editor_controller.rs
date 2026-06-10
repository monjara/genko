use std::ops::Range;

use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    Pixels, Render, Subscription, Window,
};
use rich_text::{RichTextDocumentMeta, RichTextKind};

use crate::{editor::Event, vim::VimController};

pub struct EditorController {
    vim_controller: Entity<VimController>,
    _vim_subscription: Subscription,
}

impl EditorController {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let vim_controller = cx.new(VimController::new);
        let _vim_subscription = cx
            .subscribe(&vim_controller, |_, _vim_controller, event: &Event, cx| {
                cx.emit(event.clone())
            });
        Self {
            vim_controller,
            _vim_subscription,
        }
    }

    pub fn load_plain_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.vim_controller
            .update(cx, |vim_controller, cx| vim_controller.load_text(text, cx));
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

    pub fn rows_per_column(&self, cx: &App) -> usize {
        self.vim_controller.read(cx).rows_per_column(cx)
    }

    pub fn cursor_column(&self, cx: &App) -> usize {
        self.vim_controller.read(cx).cursor_column(cx)
    }

    pub fn byte_offset_for_display_cell(&self, display_cell_index: usize, cx: &App) -> usize {
        self.vim_controller
            .read(cx)
            .byte_offset_for_display_cell(display_cell_index, cx)
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

    pub fn apply_rich_text_kind(&mut self, kind: RichTextKind, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.apply_rich_text_kind(kind, cx);
        });
    }

    pub fn set_ruby(&mut self, range: Range<usize>, text: String, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.set_ruby(range, text, cx);
        });
    }

    pub fn set_page_break_column(&mut self, column: usize, offset: usize, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.set_page_break_column(column, offset, cx);
        });
    }

    pub fn move_page_break_column(
        &mut self,
        from_column: usize,
        to_column: usize,
        offset: usize,
        cx: &mut Context<Self>,
    ) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.move_page_break_column(from_column, to_column, offset, cx);
        });
    }

    pub fn remove_page_break_column(&mut self, column: usize, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.remove_page_break_column(column, cx);
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

impl EventEmitter<Event> for EditorController {}

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
