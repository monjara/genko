mod editor;
mod editor_canvas;
mod editor_state;
pub mod vim;

pub use vim::{Vim, VimMode, VimModeLabel, VimState};

use gpui::App;

pub fn init(cx: &mut App) {
    vim::init(cx);
    editor::init(cx);
}
