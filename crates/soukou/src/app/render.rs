use gpui::{
    BoxShadow, Context, Decorations, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window, div, point, prelude::FluentBuilder, px, svg,
    transparent_black,
};
use theme::APP_FONT_FAMILY;
use theme::Theme;
use workspace::WorkspaceState;

use crate::app::{AppModal, SoukouApp, UPDATE_AVAILABLE_TITLE, mix, toolbar_border_color};

impl SoukouApp {
    fn render_unsupported_document(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let file_name = WorkspaceState::global(cx)
            .unsupported_file()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("選択したファイル")
            .to_string();

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
                    .child(div().font_weight(FontWeight::BOLD).child(file_name))
                    .child(div().text_sm().child("このファイルはサポートしていません")),
            )
    }

    fn render_active_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let modal = self.active_modal.clone()?;
        let accent = mix(Theme::global(cx).primary(), Theme::global(cx).white(), 0.84);
        let (icon_path, title, subtitle, detail, secondary_label, primary_label) = match &modal {
            AppModal::Error { title, detail } => (
                icons::MODAL_ERROR,
                title.clone(),
                "操作を完了できませんでした。".to_string(),
                detail.clone(),
                None,
                Some("閉じる".to_string()),
            ),
            AppModal::Info { title, detail } => (
                icons::MODAL_INFO,
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
}

impl Render for SoukouApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        title_bar::sync_client_window_inset(window);
        self.sync_window_title(window, cx);
        let bar_height = title_bar::platform_title_bar_height(window);
        let occupied_workspace_width = if self.workspace_pane_visible(cx) {
            WorkspaceState::global(cx).pane_width()
        } else {
            0.0
        };
        let mut editor_viewport_size = window.viewport_size();
        editor_viewport_size.width =
            px((editor_viewport_size.width.as_f32() - occupied_workspace_width).max(0.0));
        editor_viewport_size.height -= bar_height * 2.0;
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.update_viewport_size(editor_viewport_size, cx);
        });

        let content = if WorkspaceState::global(cx).unsupported_file().is_some() {
            self.render_unsupported_document(cx).into_any_element()
        } else {
            self.editor_controller.clone().into_any_element()
        };

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
                    .on_action(cx.listener(Self::open_workspace_action))
                    .on_action(cx.listener(Self::toggle_workspace_pane_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::export_txt_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
                    .child(self.title_bar.clone().into_element())
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .flex()
                            .when(self.workspace_pane_visible(cx), |this| {
                                this.child(self.workspace.clone().into_element())
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(content),
                            ),
                    )
                    .when_some(self.render_active_modal(cx), |this, modal| {
                        this.child(modal)
                    })
                    .child(self.bottom_bar.clone().into_element()),
            )
    }
}
