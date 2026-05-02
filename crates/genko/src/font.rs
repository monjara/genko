use std::borrow::Cow;

use gpui::App;

pub(crate) fn init(cx: &mut App) {
    let fonts = [include_bytes!(
        "../../../assets/fonts/ZenOldMincho-Regular.ttf"
    )]
    .iter()
    .map(|bytes| Cow::Borrowed(&bytes[..]))
    .collect();

    let Ok(_) = cx.text_system().add_fonts(fonts) else {
        eprintln!("Failed to load fonts");
        return;
    };
}
