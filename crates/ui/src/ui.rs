mod menubar;
mod text_input;

use gpui::App;
pub use menubar::*;
pub use text_input::*;

pub fn init(cx: &mut App) {
    text_input::init(cx);
}
