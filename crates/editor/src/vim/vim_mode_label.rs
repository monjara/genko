use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div};
use theme::Theme;

use super::{VimMode, VimState};

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
            .text_color(Theme::global(cx).text_primary())
            .border_1()
            .rounded_sm()
            .child(self.mode_label(window, cx))
    }
}
