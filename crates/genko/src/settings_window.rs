use gpui::{
    ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, rgb,
};

use settings::AppSettings;

use theme::{
    ACCENT_PRIMARY, APP_FONT_FAMILY, BORDER_MUTED, PANEL_BACKGROUND, PAPER_BACKGROUND,
    TEXT_INVERSE, TEXT_PRIMARY, TEXT_SECONDARY,
};

pub(crate) struct SettingsWindow {
    status: SharedString,
}

impl SettingsWindow {
    pub(crate) fn new() -> Self {
        Self { status: "".into() }
    }

    fn toggle_grid_lines(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // TODO AppSettings内で関数を作成
        AppSettings::global_mut(cx).show_grid_lines = !AppSettings::global(cx).show_grid_lines;
        self.status = "".into();
        cx.notify();
    }

    fn toggle_vim_mode(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // TODO AppSettings内で関数を作成
        AppSettings::global_mut(cx).vim_mode = !AppSettings::global(cx).vim_mode;
        self.status = "".into();
        cx.notify();
    }

    fn toggle_rows_auto(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // TODO AppSettings内で関数を作成
        AppSettings::global_mut(cx).rows_per_column = AppSettings::global(cx)
            .rows_per_column
            .map_or_else(|| Some(AppSettings::default_rows_per_column()), |_| None);
        self.status = "".into();
        cx.notify();
    }

    fn decrement_rows_per_column(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = AppSettings::global(cx)
            .rows_per_column
            .unwrap_or_else(AppSettings::default_rows_per_column);

        AppSettings::global_mut(cx).rows_per_column = Some(
            rows.saturating_sub(1)
                .max(AppSettings::min_rows_per_column()),
        );

        self.status = "".into();
        cx.notify();
    }

    fn increment_rows_per_column(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = AppSettings::global(cx)
            .rows_per_column
            .unwrap_or_else(AppSettings::default_rows_per_column);

        AppSettings::global_mut(cx).rows_per_column =
            Some((rows + 1).min(AppSettings::max_rows_per_column()));
        self.status = "".into();
        cx.notify();
    }

    fn apply(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match AppSettings::global(cx).save() {
            Ok(()) => {
                let settings = AppSettings::load();
                self.apply_settings(settings, cx);
                self.status = "保存しました".into();
            }
            Err(error) => {
                self.status = error.into();
            }
        }
        cx.notify();
    }

    fn apply_settings(&mut self, settings: AppSettings, cx: &mut Context<Self>) {
        let row_settings = AppSettings::global_mut(cx);
        // let was_vim_mode = self.settings.vim_mode;
        // old_settings = settings.normalized();
        // if self.settings.vim_mode != was_vim_mode {
        // self.vim.reset_for_enabled(self.settings.vim_mode);
        // }
        if let Some(rows_per_column) = settings.rows_per_column {
            row_settings.rows_per_column = Some(rows_per_column);
        }
        // self.ensure_cursor_visible();
        cx.notify();
    }

    fn render_rows_per_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows_label = AppSettings::global(cx)
            .rows_per_column
            .map_or_else(|| "自動".to_string(), |rows| rows.to_string());

        let mode_label = if AppSettings::global(cx).rows_per_column.is_some() {
            "固定"
        } else {
            "自動"
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(rgb(BORDER_MUTED))
            .rounded_sm()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("1列の文字数"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(format!(
                                "未指定ならウィンドウの高さに合わせます。固定は{}から{}の範囲です",
                                AppSettings::min_rows_per_column(),
                                AppSettings::max_rows_per_column()
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("settings-rows-auto-toggle")
                            .px_3()
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(BORDER_MUTED))
                            .cursor_pointer()
                            .active(|this| this.opacity(0.85))
                            .child(mode_label)
                            .on_click(cx.listener(Self::toggle_rows_auto)),
                    )
                    .child(
                        div()
                            .id("settings-rows-decrement")
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(BORDER_MUTED))
                            .cursor_pointer()
                            .active(|this| this.opacity(0.85))
                            .child("-")
                            .on_click(cx.listener(Self::decrement_rows_per_column)),
                    )
                    .child(
                        div()
                            .w(px(52.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .bg(rgb(PANEL_BACKGROUND))
                            .child(rows_label),
                    )
                    .child(
                        div()
                            .id("settings-rows-increment")
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(BORDER_MUTED))
                            .cursor_pointer()
                            .active(|this| this.opacity(0.85))
                            .child("+")
                            .on_click(cx.listener(Self::increment_rows_per_column)),
                    ),
            )
    }

    fn render_grid_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let value_label = if AppSettings::global(cx).show_grid_lines {
            "表示する"
        } else {
            "表示しない"
        };

        div()
            .id("settings-grid-toggle")
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(rgb(BORDER_MUTED))
            .rounded_sm()
            .cursor_pointer()
            .on_click(cx.listener(Self::toggle_grid_lines))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("グリッド線"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child("原稿用紙のマス目を表示します"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .bg(if AppSettings::global(cx).show_grid_lines {
                        rgb(ACCENT_PRIMARY)
                    } else {
                        rgb(PANEL_BACKGROUND)
                    })
                    .text_color(if AppSettings::global(cx).show_grid_lines {
                        rgb(TEXT_INVERSE)
                    } else {
                        rgb(TEXT_PRIMARY)
                    })
                    .child(value_label),
            )
    }

    fn render_vim_mode_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let value_label = if AppSettings::global(cx).vim_mode {
            "有効"
        } else {
            "無効"
        };

        div()
            .id("settings-vim-mode-toggle")
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(rgb(BORDER_MUTED))
            .rounded_sm()
            .cursor_pointer()
            .on_click(cx.listener(Self::toggle_vim_mode))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Vimモード"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child("Vimの通常モードと挿入モードで編集します"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .bg(if AppSettings::global(cx).vim_mode {
                        rgb(ACCENT_PRIMARY)
                    } else {
                        rgb(PANEL_BACKGROUND)
                    })
                    .text_color(if AppSettings::global(cx).vim_mode {
                        rgb(TEXT_INVERSE)
                    } else {
                        rgb(TEXT_PRIMARY)
                    })
                    .child(value_label),
            )
    }

    fn render_apply_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-apply")
            .px_4()
            .py_2()
            .bg(rgb(ACCENT_PRIMARY))
            .text_color(rgb(TEXT_INVERSE))
            .rounded_sm()
            .cursor_pointer()
            .active(|this| this.opacity(0.85))
            .child("適用")
            .on_click(cx.listener(Self::apply))
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(PAPER_BACKGROUND))
            .font_family(APP_FONT_FAMILY)
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .text_color(rgb(TEXT_PRIMARY))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_2xl().font_weight(FontWeight::BOLD).child("設定"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child("変更はsettings.jsonに保存されます"),
                    ),
            )
            .child(self.render_grid_toggle(cx))
            .child(self.render_vim_mode_toggle(cx))
            .child(self.render_rows_per_column(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .h(px(24.0))
                            .text_sm()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(self.status.clone()),
                    )
                    .child(self.render_apply_button(cx)),
            )
    }
}
