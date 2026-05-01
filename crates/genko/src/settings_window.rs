use gpui::{
    ClickEvent, Context, Decorations, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::*, px,
};

use settings::{AppSettings, ColumnNumberMode};

use theme::{APP_FONT_FAMILY, Theme};
use title_bar as app_title_bar;

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

    fn toggle_hanging_punctuation(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        AppSettings::global_mut(cx).hanging_punctuation =
            !AppSettings::global(cx).hanging_punctuation;
        self.status = "".into();
        cx.notify();
    }

    fn set_column_number_mode(&mut self, mode: ColumnNumberMode, cx: &mut Context<Self>) {
        AppSettings::global_mut(cx).column_number_mode = mode;
        self.status = "".into();
        cx.notify();
    }

    fn set_column_number_hidden(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_column_number_mode(ColumnNumberMode::Hidden, cx);
    }

    fn set_column_number_every_five(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_column_number_mode(ColumnNumberMode::EveryFive, cx);
    }

    fn set_column_number_every_ten(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_column_number_mode(ColumnNumberMode::EveryTen, cx);
    }

    fn set_column_number_all(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_column_number_mode(ColumnNumberMode::All, cx);
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

    fn decrement_cell_size(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        AppSettings::global_mut(cx).cell_size = AppSettings::global(cx)
            .cell_size
            .saturating_sub(1)
            .max(AppSettings::min_cell_size());
        self.status = "".into();
        cx.notify();
    }

    fn increment_cell_size(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        AppSettings::global_mut(cx).cell_size =
            (AppSettings::global(cx).cell_size + 1).min(AppSettings::max_cell_size());
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
        row_settings.cell_size = settings.cell_size;
        row_settings.hanging_punctuation = settings.hanging_punctuation;
        row_settings.column_number_mode = settings.column_number_mode;
        row_settings.show_grid_lines = settings.show_grid_lines;
        row_settings.vim_mode = settings.vim_mode;
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
            .border_color(Theme::global(cx).primary())
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
                            .text_color(Theme::global(cx).text_senodary())
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
                            .border_color(Theme::global(cx).primary())
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
                            .border_color(Theme::global(cx).primary())
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
                            .bg(Theme::global(cx).white())
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
                            .border_color(Theme::global(cx).primary())
                            .cursor_pointer()
                            .active(|this| this.opacity(0.85))
                            .child("+")
                            .on_click(cx.listener(Self::increment_rows_per_column)),
                    ),
            )
    }

    fn render_cell_size(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(Theme::global(cx).primary())
            .rounded_sm()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("マスの大きさ"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(Theme::global(cx).text_senodary())
                            .child(format!(
                                "{}pxから{}pxの範囲で調整します。文字サイズも連動します",
                                AppSettings::min_cell_size(),
                                AppSettings::max_cell_size()
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
                            .id("settings-cell-size-decrement")
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(Theme::global(cx).primary())
                            .cursor_pointer()
                            .active(|this| this.opacity(0.85))
                            .child("-")
                            .on_click(cx.listener(Self::decrement_cell_size)),
                    )
                    .child(
                        div()
                            .w(px(64.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .bg(Theme::global(cx).bg_senodary())
                            .child(format!("{}px", AppSettings::global(cx).cell_size)),
                    )
                    .child(
                        div()
                            .id("settings-cell-size-increment")
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(Theme::global(cx).primary())
                            .cursor_pointer()
                            .active(|this| this.opacity(0.85))
                            .child("+")
                            .on_click(cx.listener(Self::increment_cell_size)),
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
            .border_color(Theme::global(cx).primary())
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
                            .text_color(Theme::global(cx).text_senodary())
                            .child("原稿用紙のマス目を表示します"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .bg(if AppSettings::global(cx).show_grid_lines {
                        Theme::global(cx).primary()
                    } else {
                        Theme::global(cx).white()
                    })
                    .text_color(if AppSettings::global(cx).show_grid_lines {
                        // TODO どこ？
                        Theme::global(cx).white()
                    } else {
                        Theme::global(cx).primary()
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
            .border_color(Theme::global(cx).primary())
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
                            .text_color(Theme::global(cx).text_senodary())
                            .child("Vimの通常モードと挿入モードで編集します"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .bg(if AppSettings::global(cx).vim_mode {
                        Theme::global(cx).primary()
                    } else {
                        Theme::global(cx).white()
                    })
                    .text_color(if AppSettings::global(cx).vim_mode {
                        Theme::global(cx).white()
                    } else {
                        Theme::global(cx).primary()
                    })
                    .child(value_label),
            )
    }

    fn render_hanging_punctuation_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let value_label = if AppSettings::global(cx).hanging_punctuation {
            "有効"
        } else {
            "無効"
        };

        div()
            .id("settings-hanging-punctuation-toggle")
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(Theme::global(cx).primary())
            .rounded_sm()
            .cursor_pointer()
            .on_click(cx.listener(Self::toggle_hanging_punctuation))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("ぶら下がり"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(Theme::global(cx).text_senodary())
                            .child("行頭の句読点を前のマスへぶら下げます"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .bg(if AppSettings::global(cx).hanging_punctuation {
                        Theme::global(cx).primary()
                    } else {
                        Theme::global(cx).white()
                    })
                    .text_color(if AppSettings::global(cx).hanging_punctuation {
                        Theme::global(cx).white()
                    } else {
                        Theme::global(cx).primary()
                    })
                    .child(value_label),
            )
    }

    fn render_column_number_mode(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = AppSettings::global(cx).column_number_mode;
        let option = |id: &'static str,
                      label: &'static str,
                      active: bool,
                      listener: fn(
            &mut SettingsWindow,
            &ClickEvent,
            &mut Window,
            &mut Context<Self>,
        )| {
            div()
                .id(id)
                .px_3()
                .h(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .border_1()
                .border_color(Theme::global(cx).primary())
                .bg(if active {
                    Theme::global(cx).primary()
                } else {
                    Theme::global(cx).white()
                })
                .text_color(if active {
                    Theme::global(cx).white()
                } else {
                    Theme::global(cx).text_primary()
                })
                .cursor_pointer()
                .active(|this| this.opacity(0.85))
                .child(label)
                .on_click(cx.listener(listener))
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(Theme::global(cx).primary())
            .rounded_sm()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("列番号"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(Theme::global(cx).text_senodary())
                            .child("選択した間隔の列だけ、原稿用紙の上に番号を表示します"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(option(
                        "settings-column-number-hidden",
                        ColumnNumberMode::Hidden.label(),
                        selected == ColumnNumberMode::Hidden,
                        Self::set_column_number_hidden,
                    ))
                    .child(option(
                        "settings-column-number-every-five",
                        ColumnNumberMode::EveryFive.label(),
                        selected == ColumnNumberMode::EveryFive,
                        Self::set_column_number_every_five,
                    ))
                    .child(option(
                        "settings-column-number-every-ten",
                        ColumnNumberMode::EveryTen.label(),
                        selected == ColumnNumberMode::EveryTen,
                        Self::set_column_number_every_ten,
                    ))
                    .child(option(
                        "settings-column-number-all",
                        ColumnNumberMode::All.label(),
                        selected == ColumnNumberMode::All,
                        Self::set_column_number_all,
                    )),
            )
    }

    fn render_apply_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-apply")
            .px_4()
            .py_2()
            .bg(Theme::global(cx).primary())
            .text_color(Theme::global(cx).white())
            .rounded_sm()
            .cursor_pointer()
            .active(|this| this.opacity(0.85))
            .child("適用")
            .on_click(cx.listener(Self::apply))
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(Theme::global(cx).white())
            .font_family(APP_FONT_FAMILY)
            .flex()
            .flex_col()
            .text_color(Theme::global(cx).text_primary())
            .overflow_hidden()
            .map(|this| match window.window_decorations() {
                Decorations::Server => this,
                Decorations::Client { tiling } => this
                    .when(!(tiling.top || tiling.right), |this| {
                        this.rounded_tr(app_title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.top || tiling.left), |this| {
                        this.rounded_tl(app_title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.bottom || tiling.right), |this| {
                        this.rounded_br(app_title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.bottom || tiling.left), |this| {
                        this.rounded_bl(app_title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                    }),
            })
            .children(app_title_bar::render("Settings", None, window, cx))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_2xl().font_weight(FontWeight::BOLD).child("設定"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(Theme::global(cx).text_senodary())
                                    .child("変更はsettings.jsonに保存されます"),
                            ),
                    )
                    .child(self.render_grid_toggle(cx))
                    .child(self.render_cell_size(cx))
                    .child(self.render_hanging_punctuation_toggle(cx))
                    .child(self.render_column_number_mode(cx))
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
                                    .text_color(Theme::global(cx).text_senodary())
                                    .child(self.status.clone()),
                            )
                            .child(self.render_apply_button(cx)),
                    ),
            )
    }
}
