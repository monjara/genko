mod editor;
mod editor_canvas;
pub mod vim;

pub use vim::{Vim, VimCommandQuit, VimCommandWrite, VimMode, VimModeLabel, VimState};

use gpui::App;

pub fn init(cx: &mut App) {
    vim::init(cx);
    editor::init(cx);
}
