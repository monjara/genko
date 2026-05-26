use gpui::{
    Anchor, App, BoxShadow, Context, Decorations, Entity, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Window, anchored, deferred, div, point,
    prelude::FluentBuilder, px, svg, transparent_black,
};
use editor::{
    ClearHeading, SetHeadingLarge, SetHeadingMedium, ToggleBold, ToggleStrikethrough,
};
use theme::APP_FONT_FAMILY;
use theme::Theme;
use ui::TextInput;

use crate::app::{
    AppModal, EPUB_METADATA_TITLE, FeatureGate, MODAL_ERROR_ICON_PATH, MODAL_INFO_ICON_PATH,
    MODAL_PRO_ICON_PATH, MODAL_UPDATE_ICON_PATH, PRO_REQUIRED_TITLE, SoukouApp,
    UPDATE_AVAILABLE_TITLE, mix, toolbar_border_color,
};

fn toolbar_button(
    label: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .rounded_sm()
        .text_size(px(12.0))
        .cursor_pointer()
        .hover(|style| style.bg(gpui::rgb(0xf3f5f7)))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(label)
}

fn render_metadata_field(label: &'static str, input: Entity<TextInput>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_sm().font_weight(FontWeight::BOLD).child(label))
        .child(
            div()
                .w_full()
                .border_1()
                .border_color(gpui::rgba(0x00000022))
                .rounded_md()
                .overflow_hidden()
                .child(input),
        )
}

impl SoukouApp {
    fn render_richtext_toolbar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let selected_range = self.editor_controller.read(cx).selected_byte_range(cx);
        if selected_range.is_empty() {
            return None;
        }
        let selection_bounds = self.editor_controller.read(cx).selection_bounds(cx)?;
        let popup_position = point(selection_bounds.left(), selection_bounds.top() - px(56.0));

        Some(deferred(
            anchored()
                .position(popup_position)
                .anchor(Anchor::TopLeft)
                .child(
                    div()
                        .id("richtext-toolbar")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_2()
                        .bg(Theme::global(cx).white())
                        .border_1()
                        .border_color(toolbar_border_color(cx))
                        .rounded_md()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .shadow(vec![BoxShadow {
                            color: Hsla {
                                h: 0.0,
                                s: 0.0,
                                l: 0.0,
                                a: 0.16,
                            },
                            offset: point(px(0.0), px(10.0)),
                            blur_radius: px(24.0),
                            spread_radius: px(0.0),
                        }])
                        .child(toolbar_button("B", |window: &mut Window, cx: &mut App| {
                            window.dispatch_action(Box::new(ToggleBold), cx);
                        }))
                        .child(toolbar_button("S", |window: &mut Window, cx: &mut App| {
                            window.dispatch_action(Box::new(ToggleStrikethrough), cx);
                        }))
                        .child(toolbar_button("大見出し", |window: &mut Window, cx: &mut App| {
                            window.dispatch_action(Box::new(SetHeadingLarge), cx);
                        }))
                        .child(toolbar_button("小見出し", |window: &mut Window, cx: &mut App| {
                            window.dispatch_action(Box::new(SetHeadingMedium), cx);
                        }))
                        .child(toolbar_button("本文", |window: &mut Window, cx: &mut App| {
                            window.dispatch_action(Box::new(ClearHeading), cx);
                        })),
                ),
        ))
    }

    fn render_active_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let modal = self.active_modal.clone()?;
        let accent = mix(Theme::global(cx).primary(), Theme::global(cx).white(), 0.84);
        let (icon_path, title, subtitle, detail, secondary_label, primary_label) = match &modal {
            AppModal::Error { title, detail } => (
                MODAL_ERROR_ICON_PATH,
                title.clone(),
                "操作を完了できませんでした。".to_string(),
                detail.clone(),
                None,
                Some("閉じる".to_string()),
            ),
            AppModal::Info { title, detail } => (
                MODAL_INFO_ICON_PATH,
                title.clone(),
                String::new(),
                detail.clone(),
                None,
                Some("閉じる".to_string()),
            ),
            AppModal::UpdateAvailable {
                current_version,
                latest_version,
                ..
            } => (
                MODAL_UPDATE_ICON_PATH,
                UPDATE_AVAILABLE_TITLE.to_string(),
                "ダウンロードページを開いて更新できます。".to_string(),
                format!(
                    "現在のバージョンは {current_version}、最新バージョンは {latest_version} です。"
                ),
                Some("あとで".to_string()),
                Some("ダウンロード".to_string()),
            ),
            AppModal::ProRequired { feature } => (
                MODAL_PRO_ICON_PATH,
                PRO_REQUIRED_TITLE.to_string(),
                match feature {
                    FeatureGate::RichText => {
                        "リッチテキスト編集は Pro プランで利用できます。".to_string()
                    }
                    FeatureGate::ExportWord => {
                        "Word書き出しは Pro プランで利用できます。".to_string()
                    }
                    FeatureGate::ExportEpub => {
                        "EPUB書き出しは Pro プランで利用できます。".to_string()
                    }
                },
                "アカウント設定を開いて、プラン管理と機能の詳細を確認できます。".to_string(),
                Some("あとで".to_string()),
                Some("アカウント設定".to_string()),
            ),
        };

        Some(
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
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(Self::dismiss_active_modal),
                )
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
                                                .external_path(icon_path)
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
                                                .child(title),
                                        )
                                        .when(!subtitle.is_empty(), |this| {
                                            this.child(
                                                div()
                                                    .text_color(Theme::global(cx).text_senodary())
                                                    .child(subtitle),
                                            )
                                        })
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(Theme::global(cx).text_senodary())
                                                .child(detail),
                                        ),
                                ),
                        )
                        .when(matches!(modal, AppModal::ProRequired { .. }), |this| {
                            this.child(
                                div()
                                    .w_full()
                                    .px_4()
                                    .py_3()
                                    .rounded_md()
                                    .bg(Theme::global(cx).bg_senodary())
                                    .text_sm()
                                    .text_color(Theme::global(cx).text_primary())
                                    .child(
                                        "Pro ではリッチテキスト編集と書き出し機能が有効になります。",
                                    ),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .when_some(secondary_label, |this, label| {
                                    this.child(
                                        div()
                                            .px_4()
                                            .py_2()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(toolbar_border_color(cx))
                                            .cursor_pointer()
                                            .hover(|style| style.bg(gpui::rgb(0xf4f5f6)))
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(Self::dismiss_active_modal),
                                            )
                                            .child(label),
                                    )
                                })
                                .when_some(primary_label, |this, label| {
                                    this.child(
                                        div()
                                            .px_4()
                                            .py_2()
                                            .rounded_sm()
                                            .bg(Theme::global(cx).primary())
                                            .text_color(Theme::global(cx).white())
                                            .cursor_pointer()
                                            .hover(|style| style.opacity(0.92))
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(Self::open_modal_primary_action),
                                            )
                                            .child(label),
                                    )
                                }),
                        ),
                ),
        )
    }

    fn render_epub_metadata_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let form = self.epub_metadata_form.as_ref()?;

        Some(
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
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(Self::dismiss_epub_metadata_modal),
                )
                .child(
                    div()
                        .w(px(560.0))
                        .max_h(px(720.0))
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
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(24.0))
                                        .font_weight(FontWeight::BOLD)
                                        .child(EPUB_METADATA_TITLE),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(Theme::global(cx).text_senodary())
                                        .child(
                                            "EPUB に埋め込むタイトル、著者、言語などの情報を入力します。",
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .children([
                                    render_metadata_field("タイトル", form.title.clone()),
                                    render_metadata_field("著者名", form.creators.clone()),
                                    render_metadata_field("言語", form.language.clone()),
                                    render_metadata_field("識別子", form.identifier.clone()),
                                    render_metadata_field("説明文", form.description.clone()),
                                    render_metadata_field("出版者", form.publisher.clone()),
                                    render_metadata_field("権利表記", form.rights.clone()),
                                    render_metadata_field("公開日", form.published_at.clone()),
                                ]),
                        )
                        .when_some(form.error_message.clone(), |this, error| {
                            this.child(
                                div()
                                    .w_full()
                                    .px_4()
                                    .py_3()
                                    .rounded_md()
                                    .bg(mix(
                                        Theme::global(cx).primary(),
                                        Theme::global(cx).white(),
                                        0.9,
                                    ))
                                    .text_sm()
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(toolbar_border_color(cx))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(gpui::rgb(0xf4f5f6)))
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(Self::dismiss_epub_metadata_modal),
                                        )
                                        .child("キャンセル"),
                                )
                                .child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .bg(Theme::global(cx).primary())
                                        .text_color(Theme::global(cx).white())
                                        .cursor_pointer()
                                        .hover(|style| style.opacity(0.92))
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(Self::confirm_epub_metadata_modal),
                                        )
                                        .child("保存先を選ぶ"),
                                ),
                        ),
                ),
        )
    }
}

impl Render for SoukouApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        title_bar::sync_client_window_inset(window);
        self.window_handle = Some(window.window_handle());
        self.sync_window_title(window, cx);
        let bar_height = title_bar::platform_title_bar_height(window);
        let mut editor_viewport_size = window.viewport_size();
        editor_viewport_size.height -= bar_height * 2.0;
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.update_viewport_size(editor_viewport_size, cx);
        });

        let content = self.editor_controller.clone().into_element();

        div()
            .size_full()
            .bg(transparent_black())
            .map(|this| match window.window_decorations() {
                Decorations::Server => this,
                Decorations::Client { tiling } => this
                    .when(!tiling.top, |this| {
                        this.pt(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.bottom, |this| {
                        this.pb(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.left, |this| {
                        this.pl(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.right, |this| {
                        this.pr(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    }),
            })
            .child(
                div()
                    .size_full()
                    .bg(Theme::global(cx).white())
                    .font_family(APP_FONT_FAMILY)
                    .flex()
                    .flex_col()
                    .items_center()
                    .overflow_hidden()
                    .map(|this| match window.window_decorations() {
                        Decorations::Server => this,
                        Decorations::Client { tiling } => this
                            .when(!(tiling.top || tiling.right), |this| {
                                this.rounded_tr(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.top || tiling.left), |this| {
                                this.rounded_tl(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.right), |this| {
                                this.rounded_br(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.left), |this| {
                                this.rounded_bl(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!tiling.is_tiled(), |this| {
                                this.shadow(title_bar::client_window_shadow())
                            }),
                    })
                    .can_drop(|value, _, _| value.is::<gpui::ExternalPaths>())
                    .on_drop(cx.listener(Self::drop_external_paths))
                    .on_action(cx.listener(Self::open_file_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::export_txt_action))
                    .on_action(cx.listener(Self::export_word_action))
                    .on_action(cx.listener(Self::export_epub_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
                    .on_action(cx.listener(Self::sign_in_action))
                    .on_action(cx.listener(Self::open_account_settings_action))
                    .on_action(cx.listener(Self::sign_out_action))
                    .on_action(cx.listener(Self::request_pro_for_richtext_action))
                    .child(self.title_bar.clone().into_element())
                    .child(
                        div().flex_1().w_full().flex().child(
                            div()
                                .flex_1()
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(content),
                        ),
                    )
                    .when_some(self.render_richtext_toolbar(cx), |this, toolbar| {
                        this.child(toolbar)
                    })
                    .when_some(self.render_epub_metadata_modal(cx), |this, modal| {
                        this.child(modal)
                    })
                    .when_some(self.render_active_modal(cx), |this, modal| {
                        this.child(modal)
                    })
                    .child(self.bottom_bar.clone().into_element()),
            )
    }
}
