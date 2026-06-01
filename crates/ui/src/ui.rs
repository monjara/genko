mod menubar;
mod text_input;
mod tooltip;

use gpui::App;
pub use menubar::*;
pub use text_input::TextInput;
pub use tooltip::Tooltip;

pub fn init(cx: &mut App) {
    text_input::init(cx);
}
