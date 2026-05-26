use std::{
    fs,
    path::{Path, PathBuf},
};

use gpui::{
    App, AppContext, Bounds, ClickEvent, Context, Decorations, Entity, FontWeight, Global,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowControlArea, WindowDecorations,
    WindowOptions, div, prelude::FluentBuilder, px, size, transparent_black,
};
use serde::{Deserialize, Serialize};
use theme::{APP_FONT_FAMILY, Theme};
use title_bar::{self as app_title_bar, TitleBar};

pub fn open_settings_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);

    let settings_window = cx
        .open_window(
            title_bar::configure_window_options(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("dev.monj.soukou".into()),
                is_movable: true,
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            }),
            move |_, cx| cx.new(SettingsWindow::new),
        )
        .unwrap();

    settings_window
        .update(cx, |_, window, cx| {
            window.activate_window();
            cx.activate(true);
        })
        .unwrap();
}

struct SettingsWindow {
    title_bar: Entity<TitleBar>,
    status: SharedString,
    selected_section: SettingsSection,
}

impl SettingsWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        let title_bar = cx.new(|cx| TitleBar::new("設定", Vec::new(), None, cx));
        Self {
            title_bar,
            status: "".into(),
            selected_section: SettingsSection::General,
        }
    }

    fn select_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.selected_section = section;
        cx.notify();
    }

    fn show_general_section(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_section(SettingsSection::General, cx);
    }

    fn show_editor_section(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_section(SettingsSection::Editor, cx);
    }

    fn show_export_section(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_section(SettingsSection::Export, cx);
    }

    fn toggle_grid_lines(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let current = AppSettings::global(cx).show_grid_lines;
        AppSettings::global_mut(cx).show_grid_lines = !current;
        self.persist_settings(cx);
    }

    fn toggle_vim_mode(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let current = AppSettings::global(cx).vim_mode;
        AppSettings::global_mut(cx).vim_mode = !current;
        self.persist_settings(cx);
    }

    fn toggle_indent_on_enter(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = AppSettings::global(cx).indent_on_enter;
        AppSettings::global_mut(cx).indent_on_enter = !current;
        self.persist_settings(cx);
    }

    fn toggle_hanging_punctuation(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = AppSettings::global(cx).hanging_punctuation;
        AppSettings::global_mut(cx).hanging_punctuation = !current;
        self.persist_settings(cx);
    }

    fn set_column_number_mode(&mut self, mode: ColumnNumberMode, cx: &mut Context<Self>) {
        AppSettings::global_mut(cx).column_number_mode = mode;
        self.persist_settings(cx);
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

    fn set_export_writing_mode(
        &mut self,
        format: ExportTargetFormat,
        mode: ExportWritingMode,
        cx: &mut Context<Self>,
    ) {
        AppSettings::global_mut(cx)
            .export_settings
            .format_mut(format)
            .writing_mode = mode;
        self.persist_settings(cx);
    }

    fn set_word_vertical(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_export_writing_mode(ExportTargetFormat::Word, ExportWritingMode::Vertical, cx);
    }

    fn set_word_horizontal(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_export_writing_mode(ExportTargetFormat::Word, ExportWritingMode::Horizontal, cx);
    }

    fn set_epub_vertical(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_export_writing_mode(ExportTargetFormat::Epub, ExportWritingMode::Vertical, cx);
    }

    fn set_epub_horizontal(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_export_writing_mode(ExportTargetFormat::Epub, ExportWritingMode::Horizontal, cx);
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
        self.persist_settings(cx);
    }

    fn increment_cell_size(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        AppSettings::global_mut(cx).cell_size =
            (AppSettings::global(cx).cell_size + 1).min(AppSettings::max_cell_size());
        self.persist_settings(cx);
    }

    fn persist_settings(&mut self, cx: &mut Context<Self>) {
        self.status = match AppSettings::global(cx).save() {
            Ok(()) => "自動保存済み".into(),
            Err(error) => error.into(),
        };
        cx.notify();
    }

    fn render_setting_row(
        &self,
        title: &'static str,
        description: impl IntoElement,
        control: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_6()
            .py_5()
            .border_b_1()
            .border_color(Theme::global(cx).text_senodary())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(
                        div()
                            .text_sm()
                            .text_color(Theme::global(cx).text_senodary())
                            .child(description),
                    ),
            )
            .child(control)
    }

    fn render_selectable_chip(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl IntoElement,
        active: bool,
        listener: fn(&mut SettingsWindow, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
    }

    fn render_toggle_row(
        &self,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        enabled: bool,
        enabled_label: &'static str,
        disabled_label: &'static str,
        listener: fn(&mut SettingsWindow, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let value_label = if enabled {
            enabled_label
        } else {
            disabled_label
        };

        self.render_setting_row(
            title,
            description,
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded_sm()
                .bg(if enabled {
                    Theme::global(cx).primary()
                } else {
                    Theme::global(cx).white()
                })
                .text_color(if enabled {
                    Theme::global(cx).white()
                } else {
                    Theme::global(cx).primary()
                })
                .cursor_pointer()
                .child(value_label)
                .on_click(cx.listener(listener)),
            cx,
        )
    }

    fn render_rows_per_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows_label = AppSettings::global(cx)
            .rows_per_column
            .unwrap_or_else(AppSettings::default_rows_per_column)
            .to_string();

        self.render_setting_row(
            "1列の文字数",
            "20文字で固定です",
            div().flex().items_center().gap_2().child(
                div()
                    .w(px(52.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .bg(Theme::global(cx).bg_senodary())
                    .child(rows_label),
            ),
            cx,
        )
    }

    fn render_cell_size(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_setting_row(
            "マスの大きさ",
            format!(
                "{}pxから{}pxの範囲で調整します。文字サイズも連動します",
                AppSettings::min_cell_size(),
                AppSettings::max_cell_size()
            ),
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
            cx,
        )
    }

    fn render_grid_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_toggle_row(
            "settings-grid-toggle",
            "グリッド線",
            "原稿用紙のマス目を表示します",
            AppSettings::global(cx).show_grid_lines,
            "表示する",
            "表示しない",
            Self::toggle_grid_lines,
            cx,
        )
    }

    fn render_vim_mode_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_toggle_row(
            "settings-vim-mode-toggle",
            "Vimモード",
            "Vimの通常モードと挿入モードで編集します",
            AppSettings::global(cx).vim_mode,
            "有効",
            "無効",
            Self::toggle_vim_mode,
            cx,
        )
    }

    fn render_indent_on_enter_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_toggle_row(
            "settings-indent-on-enter-toggle",
            "Enter時に1マス字下げ",
            "有効時は改行後に1マス分スペースを挿入します",
            AppSettings::global(cx).indent_on_enter,
            "有効",
            "無効",
            Self::toggle_indent_on_enter,
            cx,
        )
    }

    fn render_hanging_punctuation_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_toggle_row(
            "settings-hanging-punctuation-toggle",
            "ぶら下がり",
            "行頭の句読点を前のマスへぶら下げます",
            AppSettings::global(cx).hanging_punctuation,
            "有効",
            "無効",
            Self::toggle_hanging_punctuation,
            cx,
        )
    }

    fn render_column_number_mode(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = AppSettings::global(cx).column_number_mode;
        self.render_setting_row(
            "列番号",
            "選択した間隔の列だけ、原稿用紙の上に番号を表示します",
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(self.render_selectable_chip(
                    "settings-column-number-hidden",
                    ColumnNumberMode::Hidden.label(),
                    selected == ColumnNumberMode::Hidden,
                    Self::set_column_number_hidden,
                    cx,
                ))
                .child(self.render_selectable_chip(
                    "settings-column-number-every-five",
                    ColumnNumberMode::EveryFive.label(),
                    selected == ColumnNumberMode::EveryFive,
                    Self::set_column_number_every_five,
                    cx,
                ))
                .child(self.render_selectable_chip(
                    "settings-column-number-every-ten",
                    ColumnNumberMode::EveryTen.label(),
                    selected == ColumnNumberMode::EveryTen,
                    Self::set_column_number_every_ten,
                    cx,
                ))
                .child(self.render_selectable_chip(
                    "settings-column-number-all",
                    ColumnNumberMode::All.label(),
                    selected == ColumnNumberMode::All,
                    Self::set_column_number_all,
                    cx,
                )),
            cx,
        )
    }

    fn render_export_writing_mode_control(
        &self,
        title: &'static str,
        description: &'static str,
        format: ExportTargetFormat,
        vertical_listener: fn(&mut SettingsWindow, &ClickEvent, &mut Window, &mut Context<Self>),
        horizontal_listener: fn(&mut SettingsWindow, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = AppSettings::global(cx)
            .export_settings
            .format(format)
            .writing_mode;
        self.render_setting_row(
            title,
            description,
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(self.render_selectable_chip(
                    format!("settings-export-{}-vertical", format.key()),
                    ExportWritingMode::Vertical.label(),
                    selected == ExportWritingMode::Vertical,
                    vertical_listener,
                    cx,
                ))
                .child(self.render_selectable_chip(
                    format!("settings-export-{}-horizontal", format.key()),
                    ExportWritingMode::Horizontal.label(),
                    selected == ExportWritingMode::Horizontal,
                    horizontal_listener,
                    cx,
                )),
            cx,
        )
    }

    fn render_export_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child("Pro 書き出し設定"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(Theme::global(cx).text_senodary())
                            .child("各形式ごとに縦書きか横書きの出力方向を選べます"),
                    ),
            )
            .child(self.render_export_writing_mode_control(
                "Word",
                "縦書きは横向きレイアウト、横書きは現在の横書きレイアウトで出力します",
                ExportTargetFormat::Word,
                Self::set_word_vertical,
                Self::set_word_horizontal,
                cx,
            ))
            .child(self.render_export_writing_mode_control(
                "EPUB",
                "縦書きは縦書き表示対応リーダー向け、横書きは現在の横書きレイアウトで出力します",
                ExportTargetFormat::Epub,
                Self::set_epub_vertical,
                Self::set_epub_horizontal,
                cx,
            ))
    }

    fn render_sidebar_item(
        &self,
        id: &'static str,
        section: SettingsSection,
        listener: fn(&mut SettingsWindow, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.selected_section == section;
        div()
            .id(id)
            .w_full()
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .rounded_sm()
            .cursor_pointer()
            .border_1()
            .border_color(if active {
                Theme::global(cx).primary()
            } else {
                Theme::global(cx).text_senodary()
            })
            .bg(if active {
                Theme::global(cx).bg_senodary()
            } else {
                Theme::global(cx).white()
            })
            .text_color(if active {
                Theme::global(cx).primary()
            } else {
                Theme::global(cx).text_primary()
            })
            .active(|this| this.opacity(0.85))
            .child(section.label())
            .on_click(cx.listener(listener))
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(220.0))
            .h_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .border_r_1()
            .border_color(Theme::global(cx).text_senodary())
            .bg(Theme::global(cx).bg_senodary())
            .child(
                div()
                    .px_2()
                    .pb_3()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(Theme::global(cx).text_senodary())
                    .child("設定"),
            )
            .child(self.render_sidebar_item(
                "settings-sidebar-general",
                SettingsSection::General,
                Self::show_general_section,
                cx,
            ))
            .child(self.render_sidebar_item(
                "settings-sidebar-editor",
                SettingsSection::Editor,
                Self::show_editor_section,
                cx,
            ))
            .child(self.render_sidebar_item(
                "settings-sidebar-export",
                SettingsSection::Export,
                Self::show_export_section,
                cx,
            ))
    }

    fn render_content_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child(self.selected_section.label()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(Theme::global(cx).text_senodary())
                    .child(self.selected_section.description()),
            )
    }

    fn render_section_group(
        &self,
        title: &'static str,
        description: &'static str,
        children: Vec<gpui::AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut group = div().flex().flex_col().gap_3().child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().font_weight(FontWeight::BOLD).child(title))
                .child(
                    div()
                        .text_sm()
                        .text_color(Theme::global(cx).text_senodary())
                        .child(description),
                ),
        );
        for child in children {
            group = group.child(child);
        }
        group
    }

    fn render_general_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_section_group(
            "一般",
            "表示と原稿用紙の基本設定です",
            vec![
                self.render_grid_toggle(cx).into_any_element(),
                self.render_cell_size(cx).into_any_element(),
                self.render_hanging_punctuation_toggle(cx)
                    .into_any_element(),
                self.render_column_number_mode(cx).into_any_element(),
                self.render_rows_per_column(cx).into_any_element(),
            ],
            cx,
        )
    }

    fn render_editor_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_section_group(
            "エディタ",
            "編集操作と入力時の挙動を設定します",
            vec![
                self.render_vim_mode_toggle(cx).into_any_element(),
                self.render_indent_on_enter_toggle(cx).into_any_element(),
            ],
            cx,
        )
    }

    fn render_export_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_section_group(
            "書き出し",
            "Pro プラン向けのファイル書き出し設定です",
            vec![self.render_export_settings(cx).into_any_element()],
            cx,
        )
    }

    fn render_selected_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.selected_section {
            SettingsSection::General => self.render_general_content(cx).into_any_element(),
            SettingsSection::Editor => self.render_editor_content(cx).into_any_element(),
            SettingsSection::Export => self.render_export_content(cx).into_any_element(),
        }
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        app_title_bar::sync_client_window_inset(window);
        div()
            .size_full()
            .bg(transparent_black())
            .map(|this| match window.window_decorations() {
                Decorations::Server => this,
                Decorations::Client { tiling } => this
                    .when(!tiling.top, |this| {
                        this.pt(app_title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.bottom, |this| {
                        this.pb(app_title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.left, |this| {
                        this.pl(app_title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.right, |this| {
                        this.pr(app_title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    }),
            })
            .child(
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
                            })
                            .when(!tiling.is_tiled(), |this| {
                                this.shadow(app_title_bar::client_window_shadow())
                            }),
                    })
                    .child(
                        div()
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            })
                            .on_mouse_down(gpui::MouseButton::Right, |event, window, cx| {
                                cx.stop_propagation();
                                window.show_window_menu(event.position);
                            })
                            .child(self.title_bar.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .h_full()
                            .flex()
                            .child(self.render_sidebar(cx))
                            .child(
                                div()
                                    .id("settings-content-scroll")
                                    .flex_1()
                                    .size_full()
                                    .overflow_y_scroll()
                                    .child(
                                        div()
                                            .p_8()
                                            .flex()
                                            .flex_col()
                                            .gap_6()
                                            .child(self.render_content_header(cx))
                                            .child(self.render_selected_content(cx))
                                            .child(
                                                div()
                                                    .pt_2()
                                                    .flex()
                                                    .items_center()
                                                    .justify_end()
                                                    .child(
                                                        div()
                                                            .h(px(24.0))
                                                            .text_sm()
                                                            .text_color(
                                                                Theme::global(cx).text_senodary(),
                                                            )
                                                            .child(self.status.clone()),
                                                    ),
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsSection {
    General,
    Editor,
    Export,
}

impl SettingsSection {
    fn label(self) -> &'static str {
        match self {
            Self::General => "一般",
            Self::Editor => "エディタ",
            Self::Export => "書き出し",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::General => "表示や原稿用紙の基本設定を変更します",
            Self::Editor => "入力や編集の挙動を変更します",
            Self::Export => "Pro プラン向けの書き出し設定を変更します",
        }
    }
}

const DEFAULT_ROWS_PER_COLUMN: usize = 20;
const DEFAULT_CELL_SIZE: usize = 28;
const MIN_CELL_SIZE: usize = 20;
const MAX_CELL_SIZE: usize = 60;
const SETTINGS_FILE: &str = "settings.json";
const KEYMAP_FILE: &str = "keymap.json";
const DEFAULT_KEYMAP_JSON: &str = include_str!("../resources/default_keymap.json");

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColumnNumberMode {
    Hidden,
    EveryFive,
    EveryTen,
    All,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportWritingMode {
    Vertical,
    Horizontal,
}

impl ExportWritingMode {
    fn label(self) -> &'static str {
        match self {
            Self::Vertical => "縦書き",
            Self::Horizontal => "横書き",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportTargetFormat {
    Word,
    Epub,
}

impl ExportTargetFormat {
    fn key(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Epub => "epub",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ExportFormatSettings {
    pub writing_mode: ExportWritingMode,
}

impl Default for ExportFormatSettings {
    fn default() -> Self {
        Self {
            writing_mode: ExportWritingMode::Vertical,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ExportSettings {
    pub word: ExportFormatSettings,
    pub epub: ExportFormatSettings,
}

impl ExportSettings {
    pub fn format(&self, format: ExportTargetFormat) -> &ExportFormatSettings {
        match format {
            ExportTargetFormat::Word => &self.word,
            ExportTargetFormat::Epub => &self.epub,
        }
    }

    pub fn format_mut(&mut self, format: ExportTargetFormat) -> &mut ExportFormatSettings {
        match format {
            ExportTargetFormat::Word => &mut self.word,
            ExportTargetFormat::Epub => &mut self.epub,
        }
    }
}

impl ColumnNumberMode {
    fn label(self) -> &'static str {
        match self {
            Self::Hidden => "非表示",
            Self::EveryFive => "5列ごと",
            Self::EveryTen => "10列ごと",
            Self::All => "全列",
        }
    }

    pub fn should_show(self, column_number: usize) -> bool {
        match self {
            Self::Hidden => false,
            Self::EveryFive => column_number.is_multiple_of(5),
            Self::EveryTen => column_number.is_multiple_of(10),
            Self::All => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct KeymapEntry {
    pub id: String,
    pub keystroke: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub show_grid_lines: bool,
    pub hanging_punctuation: bool,
    pub column_number_mode: ColumnNumberMode,
    pub cell_size: usize,
    pub rows_per_column: Option<usize>,
    #[serde(rename = "vimMode")]
    pub vim_mode: bool,
    #[serde(rename = "indentOnEnter")]
    pub indent_on_enter: bool,
    pub export_settings: ExportSettings,
    pub keymap: Vec<KeymapEntry>,
}

impl Global for AppSettings {}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_grid_lines: true,
            hanging_punctuation: true,
            column_number_mode: ColumnNumberMode::EveryFive,
            cell_size: DEFAULT_CELL_SIZE,
            rows_per_column: Some(DEFAULT_ROWS_PER_COLUMN),
            vim_mode: false,
            indent_on_enter: false,
            export_settings: ExportSettings::default(),
            keymap: Self::default_keymap(),
        }
    }
}

pub fn init(cx: &mut App) {
    let state = AppSettings::load();
    cx.set_global::<AppSettings>(state);
}

impl AppSettings {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    pub fn default_rows_per_column() -> usize {
        DEFAULT_ROWS_PER_COLUMN
    }

    pub fn min_rows_per_column() -> usize {
        DEFAULT_ROWS_PER_COLUMN
    }

    pub fn max_rows_per_column() -> usize {
        DEFAULT_ROWS_PER_COLUMN
    }

    pub fn min_cell_size() -> usize {
        MIN_CELL_SIZE
    }

    pub fn max_cell_size() -> usize {
        MAX_CELL_SIZE
    }

    pub fn keymap_keystroke(&self, id: &str) -> SharedString {
        self.keymap
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| SharedString::from(entry.keystroke.clone()))
            .unwrap_or_default()
    }

    fn load() -> Self {
        let mut settings = Self::load_from_config_file(Self::existing_settings_file_path());
        let legacy_keymap = settings.keymap.clone();
        settings.keymap =
            Self::load_keymap(Self::existing_keymap_file_path()).unwrap_or(legacy_keymap);
        settings.keymap = Self::merged_keymap(&settings.keymap);
        settings
    }

    fn save(&self) -> Result<(), String> {
        let settings = self.normalized();
        let settings_path = Self::settings_file_path()
            .ok_or_else(|| "設定ファイルの保存先を解決できません".to_string())?;
        if let Some(settings_dir) = settings_path.parent() {
            fs::create_dir_all(settings_dir)
                .map_err(|error| format!("設定ファイルの保存先を作成できません: {error}"))?;
        }
        settings.save_to_file(&settings_path)
    }

    fn normalized(&self) -> Self {
        Self {
            show_grid_lines: self.show_grid_lines,
            hanging_punctuation: self.hanging_punctuation,
            column_number_mode: self.column_number_mode,
            cell_size: self.cell_size.clamp(MIN_CELL_SIZE, MAX_CELL_SIZE),
            rows_per_column: Some(DEFAULT_ROWS_PER_COLUMN),
            vim_mode: self.vim_mode,
            indent_on_enter: self.indent_on_enter,
            export_settings: self.export_settings.clone(),
            keymap: Self::merged_keymap(&self.keymap),
        }
    }

    fn normalize_keymap(entries: &[KeymapEntry]) -> Vec<KeymapEntry> {
        let mut normalized: Vec<KeymapEntry> = Vec::new();

        for entry in entries {
            let id = entry.id.trim();
            let keystroke = entry.keystroke.trim();
            if id.is_empty() || keystroke.is_empty() {
                continue;
            }

            if let Some(existing) = normalized.iter_mut().find(|existing| existing.id == id) {
                existing.keystroke = keystroke.to_string();
            } else {
                normalized.push(KeymapEntry {
                    id: id.to_string(),
                    keystroke: keystroke.to_string(),
                });
            }
        }

        normalized
    }

    fn merged_keymap(entries: &[KeymapEntry]) -> Vec<KeymapEntry> {
        let mut merged = Self::default_keymap();
        for entry in Self::normalize_keymap(entries) {
            if let Some(existing) = merged.iter_mut().find(|existing| existing.id == entry.id) {
                existing.keystroke = entry.keystroke;
            } else {
                merged.push(entry);
            }
        }
        merged
    }

    fn default_keymap() -> Vec<KeymapEntry> {
        serde_json::from_str(DEFAULT_KEYMAP_JSON)
            .map(|entries: Vec<KeymapEntry>| Self::normalize_keymap(&entries))
            .unwrap_or_default()
    }

    fn existing_settings_file_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            Self::settings_file_path()
        }

        #[cfg(not(target_os = "windows"))]
        {
            let xdg_dirs = xdg::BaseDirectories::with_prefix("soukou");
            xdg_dirs.find_config_file(SETTINGS_FILE)
        }
    }

    fn existing_keymap_file_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            Self::keymap_file_path()
        }

        #[cfg(not(target_os = "windows"))]
        {
            let xdg_dirs = xdg::BaseDirectories::with_prefix("soukou");
            xdg_dirs.find_config_file(KEYMAP_FILE)
        }
    }

    fn settings_file_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("APPDATA")
                .map(|appdata| PathBuf::from(appdata).join("soukou").join(SETTINGS_FILE))
        }

        #[cfg(not(target_os = "windows"))]
        {
            let xdg_dirs = xdg::BaseDirectories::with_prefix("soukou");
            xdg_dirs.place_config_file(SETTINGS_FILE).ok()
        }
    }

    fn load_from_config_file(settings_path: Option<PathBuf>) -> Self {
        let Some(settings_path) = settings_path else {
            return Self::default();
        };

        let Ok(settings_json) = fs::read_to_string(settings_path) else {
            return Self::default();
        };

        serde_json::from_str::<Self>(&settings_json)
            .map(|settings| settings.normalized())
            .unwrap_or_default()
    }

    fn load_keymap(keymap_path: Option<PathBuf>) -> Option<Vec<KeymapEntry>> {
        let keymap_path = keymap_path?;
        let keymap_json = fs::read_to_string(keymap_path).ok()?;
        serde_json::from_str::<Vec<KeymapEntry>>(&keymap_json)
            .ok()
            .map(|entries| Self::normalize_keymap(&entries))
    }

    fn save_to_file(&self, settings_path: &Path) -> Result<(), String> {
        let settings_json = serde_json::to_string_pretty(&PersistedAppSettings::from(self))
            .map_err(|error| format!("設定をJSONへ変換できません: {error}"))?;
        fs::write(settings_path, settings_json)
            .map_err(|error| format!("設定ファイルを書き込めません: {error}"))
    }
}

#[derive(Serialize)]
struct PersistedAppSettings {
    show_grid_lines: bool,
    hanging_punctuation: bool,
    column_number_mode: ColumnNumberMode,
    cell_size: usize,
    rows_per_column: Option<usize>,
    #[serde(rename = "vimMode")]
    vim_mode: bool,
    #[serde(rename = "indentOnEnter")]
    indent_on_enter: bool,
    export_settings: ExportSettings,
}

impl From<&AppSettings> for PersistedAppSettings {
    fn from(settings: &AppSettings) -> Self {
        Self {
            show_grid_lines: settings.show_grid_lines,
            hanging_punctuation: settings.hanging_punctuation,
            column_number_mode: settings.column_number_mode,
            cell_size: settings.cell_size,
            rows_per_column: settings.rows_per_column,
            vim_mode: settings.vim_mode,
            indent_on_enter: settings.indent_on_enter,
            export_settings: settings.export_settings.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
    };

    use super::{
        AppSettings, ColumnNumberMode, ExportSettings, ExportTargetFormat, ExportWritingMode,
        KeymapEntry,
    };

    fn test_settings_dir(name: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!(
            "soukou_settings_test_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn uses_default_when_settings_file_is_missing() {
        let dir = test_settings_dir("missing");
        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings, AppSettings::default());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_show_grid_lines_from_settings_file() {
        let dir = test_settings_dir("loads");
        fs::write(
            dir.join("settings.json"),
            r#"{"show_grid_lines": false, "rows_per_column": 24}"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert!(!settings.show_grid_lines);
        assert!(settings.hanging_punctuation);
        assert_eq!(settings.column_number_mode, ColumnNumberMode::EveryFive);
        assert_eq!(settings.cell_size, DEFAULT_CELL_SIZE);
        assert_eq!(settings.rows_per_column, Some(DEFAULT_ROWS_PER_COLUMN));
        assert!(!settings.vim_mode);
        assert_eq!(
            settings.export_settings.word.writing_mode,
            ExportWritingMode::Vertical
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uses_default_rows_per_column_when_missing() {
        let dir = test_settings_dir("rows_missing");
        fs::write(dir.join("settings.json"), r#"{"show_grid_lines": false}"#).unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.rows_per_column, Some(DEFAULT_ROWS_PER_COLUMN));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uses_default_rows_per_column_when_null() {
        let dir = test_settings_dir("rows_null");
        fs::write(dir.join("settings.json"), r#"{"rows_per_column": null}"#).unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.rows_per_column, Some(DEFAULT_ROWS_PER_COLUMN));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn normalizes_rows_per_column_from_settings_file() {
        let dir = test_settings_dir("rows_clamp");
        fs::write(
            dir.join("settings.json"),
            format!(r#"{{"rows_per_column": {}}}"#, DEFAULT_ROWS_PER_COLUMN + 1),
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.rows_per_column, Some(DEFAULT_ROWS_PER_COLUMN));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saves_show_grid_lines_to_settings_file() {
        let dir = test_settings_dir("saves");
        let settings_path = dir.join("settings.json");
        let settings = AppSettings {
            show_grid_lines: false,
            hanging_punctuation: false,
            column_number_mode: ColumnNumberMode::EveryFive,
            cell_size: 32,
            rows_per_column: Some(24),
            vim_mode: true,
            indent_on_enter: true,
            export_settings: ExportSettings {
                word: ExportFormatSettings::default(),
                epub: ExportFormatSettings {
                    writing_mode: ExportWritingMode::Horizontal,
                },
            },
            keymap: vec![KeymapEntry {
                id: "app.open_file.ctrl".to_string(),
                keystroke: "ctrl-shift-o".to_string(),
            }],
        };

        settings.save_to_file(&settings_path).unwrap();

        let raw = fs::read_to_string(&settings_path).unwrap();
        assert!(!raw.contains("\"keymap\""));

        let reloaded = AppSettings::load_from_config_file(Some(settings_path));
        assert!(!reloaded.show_grid_lines);
        assert!(!reloaded.hanging_punctuation);
        assert_eq!(reloaded.column_number_mode, ColumnNumberMode::EveryFive);
        assert_eq!(reloaded.cell_size, 32);
        assert_eq!(reloaded.rows_per_column, Some(DEFAULT_ROWS_PER_COLUMN));
        assert!(reloaded.vim_mode);
        assert!(reloaded.indent_on_enter);
        assert_eq!(
            reloaded.export_settings.word.writing_mode,
            ExportWritingMode::Vertical
        );
        assert_eq!(
            reloaded.export_settings.epub.writing_mode,
            ExportWritingMode::Horizontal
        );
        assert_eq!(reloaded.keymap, AppSettings::default().keymap);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_vim_mode_from_settings_file() {
        let dir = test_settings_dir("vim_mode");
        fs::write(
            dir.join("settings.json"),
            r#"{"show_grid_lines": false, "rows_per_column": 24, "vimMode": true, "indentOnEnter": true}"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert!(settings.vim_mode);
        assert!(settings.indent_on_enter);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_hanging_punctuation_from_settings_file() {
        let dir = test_settings_dir("hanging_punctuation");
        fs::write(
            dir.join("settings.json"),
            r#"{"hanging_punctuation": false}"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert!(!settings.hanging_punctuation);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clamps_cell_size_from_settings_file() {
        let dir = test_settings_dir("cell_size_clamp");
        fs::write(
            dir.join("settings.json"),
            format!(r#"{{"cell_size": {}}}"#, MAX_CELL_SIZE + 1),
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.cell_size, MAX_CELL_SIZE);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_export_writing_modes_from_settings_file() {
        let dir = test_settings_dir("export_writing_modes");
        fs::write(
            dir.join("settings.json"),
            r#"{"export_settings":{"word":{"writing_mode":"vertical"},"epub":{"writing_mode":"horizontal"}}}"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(
            settings.export_settings.word.writing_mode,
            ExportWritingMode::Vertical
        );
        assert_eq!(
            settings.export_settings.epub.writing_mode,
            ExportWritingMode::Horizontal
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_legacy_keymap_entries_from_settings_file() {
        let dir = test_settings_dir("legacy_keymap");
        fs::write(
            dir.join("settings.json"),
            r#"{
                "keymap": [
                    { "id": " app.open_file.ctrl ", "keystroke": " ctrl-o " },
                    { "id": "", "keystroke": "cmd-p" },
                    { "id": "app.open_file.ctrl", "keystroke": "ctrl-shift-o" }
                ]
            }"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(
            settings
                .keymap
                .iter()
                .find(|entry| entry.id == "app.open_file.ctrl"),
            Some(&KeymapEntry {
                id: "app.open_file.ctrl".to_string(),
                keystroke: "ctrl-shift-o".to_string(),
            })
        );
        assert_eq!(
            settings.keymap_keystroke("app.open_file.ctrl"),
            SharedString::from("ctrl-shift-o".to_string())
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_custom_keymap_file_and_merges_with_default_keymap() {
        let dir = test_settings_dir("custom_keymap");
        fs::write(
            dir.join("keymap.json"),
            r#"[
                { "id": " app.open_file.ctrl ", "keystroke": " ctrl-shift-o " },
                { "id": "vim.enter_insert_mode", "keystroke": "I" }
            ]"#,
        )
        .unwrap();

        let keymap = AppSettings::load_keymap(Some(dir.join("keymap.json"))).unwrap();
        let merged = AppSettings::merged_keymap(&keymap);

        assert_eq!(
            merged.iter().find(|entry| entry.id == "app.open_file.ctrl"),
            Some(&KeymapEntry {
                id: "app.open_file.ctrl".to_string(),
                keystroke: "ctrl-shift-o".to_string(),
            })
        );
        assert_eq!(
            merged
                .iter()
                .find(|entry| entry.id == "vim.enter_insert_mode"),
            Some(&KeymapEntry {
                id: "vim.enter_insert_mode".to_string(),
                keystroke: "I".to_string(),
            })
        );
        assert_eq!(
            merged.iter().find(|entry| entry.id == "app.save_file.ctrl"),
            Some(&KeymapEntry {
                id: "app.save_file.ctrl".to_string(),
                keystroke: "ctrl-s".to_string(),
            })
        );

        let _ = fs::remove_dir_all(dir);
    }
}
