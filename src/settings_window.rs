use gpui::{
    ClickEvent, Context, Entity, FontWeight, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div, prelude::*, px, rgb,
};

use crate::{
    GenkoApp,
    settings::{AppSettings, MAX_ROWS_PER_COLUMN, MIN_ROWS_PER_COLUMN},
};

pub(crate) struct SettingsWindow {
    app: Entity<GenkoApp>,
    draft: AppSettings,
    status: SharedString,
}

impl SettingsWindow {
    pub(crate) fn new(app: Entity<GenkoApp>, settings: AppSettings) -> Self {
        Self {
            app,
            draft: settings,
            status: "".into(),
        }
    }

    fn toggle_grid_lines(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.draft.show_grid_lines = !self.draft.show_grid_lines;
        self.status = "".into();
        cx.notify();
    }

    fn decrement_rows_per_column(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.rows_per_column = self
            .draft
            .rows_per_column
            .saturating_sub(1)
            .max(MIN_ROWS_PER_COLUMN);
        self.status = "".into();
        cx.notify();
    }

    fn increment_rows_per_column(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.rows_per_column = (self.draft.rows_per_column + 1).min(MAX_ROWS_PER_COLUMN);
        self.status = "".into();
        cx.notify();
    }

    fn apply(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match self.draft.save() {
            Ok(()) => {
                let settings = AppSettings::load();
                self.draft = settings.clone();
                self.app.update(cx, |app, cx| {
                    app.apply_settings(settings, cx);
                });
                self.status = "保存しました".into();
            }
            Err(error) => {
                self.status = error.into();
            }
        }
        cx.notify();
    }

    fn render_rows_per_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(rgb(0xd9cbb8))
            .rounded_sm()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("1列の文字数"))
                    .child(div().text_sm().text_color(rgb(0x705a4a)).child(format!(
                        "{}から{}の範囲で設定できます",
                        MIN_ROWS_PER_COLUMN, MAX_ROWS_PER_COLUMN
                    ))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
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
                            .border_color(rgb(0xd9cbb8))
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
                            .bg(rgb(0xe7ded0))
                            .child(self.draft.rows_per_column.to_string()),
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
                            .border_color(rgb(0xd9cbb8))
                            .cursor_pointer()
                            .active(|this| this.opacity(0.85))
                            .child("+")
                            .on_click(cx.listener(Self::increment_rows_per_column)),
                    ),
            )
    }

    fn render_grid_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let value_label = if self.draft.show_grid_lines {
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
            .border_color(rgb(0xd9cbb8))
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
                            .text_color(rgb(0x705a4a))
                            .child("原稿用紙のマス目を表示します"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .bg(if self.draft.show_grid_lines {
                        rgb(0x2f6fff)
                    } else {
                        rgb(0xe7ded0)
                    })
                    .text_color(if self.draft.show_grid_lines {
                        rgb(0xffffff)
                    } else {
                        rgb(0x2f241d)
                    })
                    .child(value_label),
            )
    }

    fn render_apply_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-apply")
            .px_4()
            .py_2()
            .bg(rgb(0x2f6fff))
            .text_color(rgb(0xffffff))
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
            .bg(rgb(0xfffbf2))
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .text_color(rgb(0x2f241d))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_2xl().font_weight(FontWeight::BOLD).child("設定"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x705a4a))
                            .child("変更はsettings.jsonに保存されます"),
                    ),
            )
            .child(self.render_grid_toggle(cx))
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
                            .text_color(rgb(0x705a4a))
                            .child(self.status.clone()),
                    )
                    .child(self.render_apply_button(cx)),
            )
    }
}
