mod editor;
mod editor_canvas;
mod editor_controller;
mod editor_ui;
mod search;
pub mod vim;

pub use editor::{Event, PlainTextLoadSettings, PreparedPlainText};
pub use editor_controller::EditorController;
pub use editor_ui::{CloseSearch, FindNext, FindPrevious, OpenSearch};
pub use vim::{VimCommandQuit, VimCommandWrite, VimController, VimMode, VimModeLabel, VimState};

use gpui::App;

pub fn init(cx: &mut App) {
    vim::init(cx);
    editor::init(cx);
}
