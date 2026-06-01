mod capabilities;
mod editor;
mod editor_canvas;
mod editor_controller;
mod perf;
pub mod vim;

pub use editor_controller::EditorController;
pub use vim::{VimCommandQuit, VimCommandWrite, VimController, VimMode, VimModeLabel, VimState};

use gpui::App;

pub fn init(cx: &mut App) {
    capabilities::AppCapabilities::init(cx);
    vim::init(cx);
    editor::init(cx);
}
