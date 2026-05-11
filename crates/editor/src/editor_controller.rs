use crate::vim::VimController;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Window, div,
};

pub struct EditorController {
    vim_controller: Entity<VimController>,
}

impl EditorController {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let vim_controller = cx.new(VimController::new);
        Self { vim_controller }
    }

    pub fn load_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.vim_controller
            .update(cx, |vim_controller, cx| vim_controller.load_text(text, cx));
    }

    pub fn snapshot_text(&self, cx: &App) -> String {
        self.vim_controller.read(cx).snapshot_text(cx)
    }

    pub fn update_viewport_size(&mut self, size: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) {
        self.vim_controller.update(cx, |vim_controller, cx| {
            vim_controller.update_viewport_size(size, cx);
        });
    }
}

impl Render for EditorController {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.vim_controller.clone())
    }
}

impl Focusable for EditorController {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.vim_controller.focus_handle(cx)
    }
}
