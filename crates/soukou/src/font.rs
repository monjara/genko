use std::borrow::Cow;

use gpui::App;

const APP_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/ZenOldMincho-Regular.ttf");
const FONT_LOAD_ERROR_MESSAGE: &str = "Failed to load fonts";

pub(crate) fn init(cx: &mut App) {
    let fonts = [APP_FONT_BYTES]
        .iter()
        .map(|bytes| Cow::Borrowed(&bytes[..]))
        .collect();

    let Ok(_) = cx.text_system().add_fonts(fonts) else {
        eprintln!("{FONT_LOAD_ERROR_MESSAGE}");
        return;
    };
}
