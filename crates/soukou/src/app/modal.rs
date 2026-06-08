use gpui::{
    App, BoxShadow, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    Styled, Window, div, point, prelude::FluentBuilder, px, svg,
};
use theme::Theme;

use crate::{DismissActiveModal, OpenModalPrimary};
use super::{mix, toolbar_border_color};

const UPDATE_AVAILABLE_TITLE: &str = "新しいバージョンがあります";

#[derive(Clone, Debug)]
pub(super) enum AppModal {
    Info {
        title: String,
        detail: String,
    },
    ProRequired {
        feature: ProFeature,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
        release_page_url: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ProFeature {
    ExportWord,
    ExportEpub,
}

#[derive(IntoElement)]
pub(super) struct ActiveModal {
    icon_path: &'static str,
    title: String,
    subtitle: String,
    detail: String,
    secondary_label: Option<String>,
    primary_label: Option<String>,
}

impl ActiveModal {
    pub(super) fn from_modal(modal: AppModal) -> Self {
        let (icon_path, title, subtitle, detail, secondary_label, primary_label) = match modal {
            AppModal::Info { title, detail } => (
                icons::MODAL_INFO,
                title,
                String::new(),
                detail,
                None,
                Some("閉じる".to_string()),
            ),
            AppModal::ProRequired { feature } => (
                icons::MODAL_INFO,
                "Proプラン限定機能です".to_string(),
                match feature {
                    ProFeature::ExportWord => {
                        "Wordエクスポートは Pro プランで利用できます。".to_string()
                    }
                    ProFeature::ExportEpub => {
                        "epubエクスポートは Pro プランで利用できます。".to_string()
                    }
                },
                "会員登録ページを開いて、利用できるプランを確認してください。".to_string(),
                Some("あとで".to_string()),
                Some("会員登録".to_string()),
            ),
            AppModal::UpdateAvailable {
                current_version,
                latest_version,
                ..
            } => (
                icons::MODAL_UPDATE,
                UPDATE_AVAILABLE_TITLE.to_string(),
                "ダウンロードページを開いて更新できます。".to_string(),
                format!(
                    "現在のバージョンは {current_version}、最新バージョンは {latest_version} です。"
                ),
                Some("あとで".to_string()),
                Some("ダウンロード".to_string()),
            ),
        };

        Self {
            icon_path,
            title,
            subtitle,
            detail,
            secondary_label,
            primary_label,
        }
    }
}

impl RenderOnce for ActiveModal {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let accent = mix(Theme::global(cx).primary(), Theme::global(cx).white(), 0.84);

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .bg(Hsla {
                h: 0.61,
                s: 0.32,
                l: 0.08,
                a: 0.58,
            })
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(DismissActiveModal), cx);
            })
            .child(
                div()
                    .w(px(420.0))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .bg(Theme::global(cx).white())
                    .border_1()
                    .border_color(toolbar_border_color(cx))
                    .rounded_lg()
                    .shadow(vec![BoxShadow {
                        color: Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 0.0,
                            a: 0.18,
                        },
                        offset: point(px(0.0), px(18.0)),
                        blur_radius: px(42.0),
                        spread_radius: px(0.0),
                    }])
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(46.0))
                                    .h(px(46.0))
                                    .rounded_full()
                                    .bg(accent)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .external_path(self.icon_path)
                                            .size_6()
                                            .text_color(Theme::global(cx).primary()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .child(self.title),
                                    )
                                    .when(!self.subtitle.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_color(Theme::global(cx).text_senodary())
                                                .child(self.subtitle),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(Theme::global(cx).text_senodary())
                                            .child(self.detail),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .when_some(self.secondary_label, |this, label| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(toolbar_border_color(cx))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(gpui::rgb(0xf4f5f6)))
                                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                                            window
                                                .dispatch_action(Box::new(DismissActiveModal), cx);
                                        })
                                        .child(label),
                                )
                            })
                            .when_some(self.primary_label, |this, label| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .bg(Theme::global(cx).primary())
                                        .text_color(Theme::global(cx).white())
                                        .cursor_pointer()
                                        .hover(|style| style.opacity(0.92))
                                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                                            window.dispatch_action(Box::new(OpenModalPrimary), cx);
                                        })
                                        .child(label),
                                )
                            }),
                    ),
            )
    }
}
