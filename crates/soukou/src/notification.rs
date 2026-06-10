use document::DocumentError;
use gpui::{
    App, BoxShadow, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Styled, Window, div, point, px, svg,
};
use theme::Theme;

use crate::app::toolbar_border_color;

const KEYMAP_LOAD_ERROR_TITLE: &str = "キーマップを読み込めませんでした";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = soukou, no_json, no_register)]
pub(super) struct DismissErrorNotification {
    pub(super) id: u64,
}

#[derive(Clone, Debug)]
pub(super) struct ErrorNotification {
    pub(super) id: u64,
    pub(super) title: String,
    pub(super) detail: String,
}

pub(super) struct ErrorPresentation {
    pub(super) title: String,
    pub(super) detail: String,
}

impl From<keymap::Error> for ErrorPresentation {
    fn from(error: keymap::Error) -> Self {
        Self {
            title: KEYMAP_LOAD_ERROR_TITLE.to_string(),
            detail: error.to_string(),
        }
    }
}

impl From<DocumentError> for ErrorPresentation {
    fn from(error: DocumentError) -> Self {
        let title = match &error {
            DocumentError::OpenFailed { .. } | DocumentError::MetadataOpenFailed { .. } => {
                FILE_OPEN_ERROR_TITLE
            }
            DocumentError::SaveFailed { .. } | DocumentError::MetadataSaveFailed { .. } => {
                FILE_SAVE_ERROR_TITLE
            }
        };

        Self {
            title: title.to_string(),
            detail: error.to_string(),
        }
    }
}

#[derive(IntoElement)]
pub(super) struct ErrorNotificationStack {
    notifications: Vec<ErrorNotification>,
    bottom_bar_height: Pixels,
}

impl ErrorNotificationStack {
    pub(super) fn new(notifications: Vec<ErrorNotification>, bottom_bar_height: Pixels) -> Self {
        Self {
            notifications,
            bottom_bar_height,
        }
    }
}

impl RenderOnce for ErrorNotificationStack {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.notifications.into_iter().fold(
            div()
                .absolute()
                .right(px(14.0))
                .bottom(self.bottom_bar_height + px(14.0))
                .w(px(360.0))
                .flex()
                .flex_col()
                .gap_2(),
            |stack, notification| stack.child(ErrorNotificationView::new(notification)),
        )
    }
}

#[derive(IntoElement)]
struct ErrorNotificationView {
    notification: ErrorNotification,
}

impl ErrorNotificationView {
    fn new(notification: ErrorNotification) -> Self {
        Self { notification }
    }
}

impl RenderOnce for ErrorNotificationView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let notification_id = self.notification.id;

        div()
            .w_full()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .bg(Theme::global(cx).white())
            .border_1()
            .border_color(toolbar_border_color(cx))
            .rounded_md()
            .shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.14,
                },
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(24.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(Theme::global(cx).text_primary())
                            .child(self.notification.title),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size_5()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(gpui::rgb(0xf4f5f6)))
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                window.dispatch_action(
                                    Box::new(DismissErrorNotification {
                                        id: notification_id,
                                    }),
                                    cx,
                                );
                                cx.stop_propagation();
                            })
                            .child(
                                svg()
                                    .external_path(icons::X)
                                    .size_3()
                                    .text_color(Theme::global(cx).text_senodary()),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(Theme::global(cx).text_senodary())
                    .child(self.notification.detail),
            )
    }
}
