mod editor;
mod editor_controller;
mod editor_canvas;
pub mod vim;

pub use editor_controller::EditorController;
pub use vim::{VimCommandQuit, VimCommandWrite, VimController, VimMode, VimModeLabel, VimState};

use gpui::App;

pub fn init(cx: &mut App) {
    vim::init(cx);
    editor::init(cx);
}
