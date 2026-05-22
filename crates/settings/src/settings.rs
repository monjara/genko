use std::{
    env, fs,
    path::{Path, PathBuf},
};

use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Bounds, ClickEvent, Context, Decorations, Entity, FontWeight, Global,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowDecorations, WindowOptions,
    div, px, size, transparent_black,
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
}

impl SettingsWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        let title_bar = cx.new(|cx| TitleBar::new("設定", Vec::new(), cx));
        Self {
            title_bar,
            status: "".into(),
        }
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

    fn toggle_indent_on_enter(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        AppSettings::global_mut(cx).indent_on_enter = !AppSettings::global(cx).indent_on_enter;
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
        row_settings.rows_per_column = settings.rows_per_column;
        cx.notify();
    }

    fn render_rows_per_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows_label = AppSettings::global(cx)
            .rows_per_column
            .unwrap_or_else(AppSettings::default_rows_per_column)
            .to_string();

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
                            .child("20文字で固定です"),
                    ),
            )
            .child(
                div().flex().items_center().gap_2().child(
                    div()
                        .w(px(52.0))
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(Theme::global(cx).white())
                        .child(rows_label),
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

    fn render_indent_on_enter_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let value_label = if AppSettings::global(cx).indent_on_enter {
            "有効"
        } else {
            "無効"
        };

        div()
            .id("settings-indent-on-enter-toggle")
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_3()
            .border_1()
            .border_color(Theme::global(cx).primary())
            .rounded_sm()
            .cursor_pointer()
            .on_click(cx.listener(Self::toggle_indent_on_enter))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Enter時に1マス字下げ"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(Theme::global(cx).text_senodary())
                            .child("有効時は改行後に1マス分スペースを挿入します"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .bg(if AppSettings::global(cx).indent_on_enter {
                        Theme::global(cx).primary()
                    } else {
                        Theme::global(cx).white()
                    })
                    .text_color(if AppSettings::global(cx).indent_on_enter {
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
                    .child(self.title_bar.clone())
                    .child(
                        div()
                            .id("settings-content-scroll")
                            .flex_1()
                            .w_full()
                            .overflow_y_scroll()
                            .p_6()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::BOLD)
                                            .child("設定"),
                                    )
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
                            .child(self.render_indent_on_enter_toggle(cx))
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
                    ),
            )
    }
}

const DEFAULT_ROWS_PER_COLUMN: usize = 20;
const DEFAULT_CELL_SIZE: usize = 28;
const MIN_CELL_SIZE: usize = 20;
const MAX_CELL_SIZE: usize = 60;
const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColumnNumberMode {
    Hidden,
    EveryFive,
    EveryTen,
    All,
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
}

impl Global for AppSettings {}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_grid_lines: true,
            hanging_punctuation: true,
            column_number_mode: ColumnNumberMode::Hidden,
            cell_size: DEFAULT_CELL_SIZE,
            rows_per_column: Some(DEFAULT_ROWS_PER_COLUMN),
            vim_mode: false,
            indent_on_enter: false,
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

    fn load() -> Self {
        Self::load_from_config_file(Self::existing_settings_file_path())
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
        }
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

    fn settings_file_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            env::var_os("APPDATA")
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

    fn save_to_file(&self, settings_path: &Path) -> Result<(), String> {
        let settings_json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("設定をJSONへ変換できません: {error}"))?;
        fs::write(settings_path, settings_json)
            .map_err(|error| format!("設定ファイルを書き込めません: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

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
        assert_eq!(settings.column_number_mode, ColumnNumberMode::Hidden);
        assert_eq!(settings.cell_size, DEFAULT_CELL_SIZE);
        assert_eq!(settings.rows_per_column, Some(DEFAULT_ROWS_PER_COLUMN));
        assert!(!settings.vim_mode);

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
        };

        settings.save_to_file(&settings_path).unwrap();

        let reloaded = AppSettings::load_from_config_file(Some(settings_path));
        assert!(!reloaded.show_grid_lines);
        assert!(!reloaded.hanging_punctuation);
        assert_eq!(reloaded.column_number_mode, ColumnNumberMode::EveryFive);
        assert_eq!(reloaded.cell_size, 32);
        assert_eq!(reloaded.rows_per_column, Some(DEFAULT_ROWS_PER_COLUMN));
        assert!(reloaded.vim_mode);
        assert!(reloaded.indent_on_enter);

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
}
