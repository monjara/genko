use gpui::{App, FontWeight, IntoElement, ParentElement, RenderOnce, Styled, Window, div};
use theme::Theme;
use workspace::WorkspaceState;

#[derive(IntoElement)]
pub(super) struct UnsupportedDocument {
    file_name: String,
}

impl UnsupportedDocument {
    pub(super) fn from_workspace(cx: &App) -> Self {
        let file_name = WorkspaceState::global(cx)
            .unsupported_file()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("選択したファイル")
            .to_string();

        Self { file_name }
    }
}

impl RenderOnce for UnsupportedDocument {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .text_color(Theme::global(cx).text_senodary())
                    .child(div().font_weight(FontWeight::BOLD).child(self.file_name))
                    .child(div().text_sm().child("このファイルはサポートしていません")),
            )
    }
}
