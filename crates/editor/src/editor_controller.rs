use std::ops::Range;

use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    Pixels, Render, Subscription, Window,
};
use rich_text::RichTextDocumentMeta;

use crate::{
    editor::{AppliedEditBatch, Event},
    vim::VimController,
};

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

    pub fn draft_revision(&self, cx: &App) -> u64 {
        self.vim_controller.read(cx).draft_revision(cx)
    }

    pub fn last_applied_edit_batch(&self, cx: &App) -> Option<AppliedEditBatch> {
        self.vim_controller.read(cx).last_applied_edit_batch(cx)
    }

    pub fn selected_byte_range(&self, cx: &App) -> Range<usize> {
        self.vim_controller.read(cx).selected_byte_range(cx)
    }

    pub fn rows_per_column(&self, cx: &App) -> usize {
        self.vim_controller.read(cx).rows_per_column(cx)
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
