mod font;

use std::path::{Path, PathBuf};

use bottom_bar::BottomBar;
use editor::{EditorController, VimCommandQuit, VimCommandWrite};
use gpui::{
    App, AppContext, Bounds, Context, Decorations, Entity, ExternalPaths, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, Menu, MenuItem, ParentElement, PathPromptOptions,
    PromptLevel, Render, Styled, Window, WindowBounds, WindowDecorations, WindowOptions, actions,
    div, prelude::FluentBuilder, px, size, transparent_black,
};
use semver::Version;
use serde::Deserialize;
use settings::open_settings_window;
use theme::{APP_FONT_FAMILY, Theme};
use title_bar::{TitleBar, TitleBarMenu};
use ui::{MenuBarItem, MenuBarMenu};

const APP_NAME: &str = "草稿";
const APP_ID: &str = "dev.monj.soukou";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const OK_BUTTON_LABEL: &str = "OK";
const CANCEL_BUTTON_LABEL: &str = "キャンセル";
const DOWNLOAD_BUTTON_LABEL: &str = "ダウンロード";
const OPEN_PROMPT_LABEL: &str = "開く";
const SETTINGS_MENU_LABEL: &str = "設定";
const CHECK_FOR_UPDATES_MENU_LABEL: &str = "更新を確認";
const QUIT_MENU_LABEL: &str = "終了";
const FILE_MENU_LABEL: &str = "ファイル";
const SAVE_MENU_LABEL: &str = "保存";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const FILE_PICKER_ERROR_TITLE: &str = "ファイル選択を開けませんでした";
const SAVE_PATH_PICKER_ERROR_TITLE: &str = "保存先を選択できませんでした";
const UPDATE_CHECK_ERROR_TITLE: &str = "更新を確認できませんでした";
const UPDATE_AVAILABLE_TITLE: &str = "新しいバージョンがあります";
const UPDATE_NOT_AVAILABLE_TITLE: &str = "最新版を使用しています";
const UNSUPPORTED_TEXT_FILE_ERROR_DETAIL: &str = "現在は .txt ファイルのみ対応しています";
const SUPPORTED_TEXT_FILE_EXTENSION: &str = "txt";
const DEFAULT_NEW_FILE_NAME: &str = "untitled.txt";
const CURRENT_DIRECTORY_FALLBACK: &str = ".";
const WINDOW_TITLE_SEPARATOR: &str = " - ";
const MAIN_WINDOW_WIDTH: f32 = 1200.0;
const MAIN_WINDOW_HEIGHT: f32 = 800.0;
const OPEN_FILE_SHORTCUT_MAC: &str = "cmd-o";
const OPEN_FILE_SHORTCUT_CTRL: &str = "ctrl-o";
const SAVE_FILE_SHORTCUT_MAC: &str = "cmd-s";
const SAVE_FILE_SHORTCUT_CTRL: &str = "ctrl-s";
const QUIT_SHORTCUT_MAC: &str = "cmd-q";
const OPEN_SETTINGS_SHORTCUT_CTRL: &str = "ctrl-,";
const RELEASES_LATEST_API_URL: &str =
    "https://api.github.com/repos/monjara/Soukou.app/releases/latest";

actions!(
    soukou,
    [OpenSettings, CheckForUpdates, OpenFile, SaveFile, Quit]
);

#[derive(Deserialize)]
struct GitHubRelease {
    html_url: String,
    tag_name: String,
}

struct AvailableUpdate {
    current_version: Version,
    latest_version: Version,
    release_page_url: String,
}

struct SoukouApp {
    editor_controller: Entity<EditorController>,
    active_file: Option<PathBuf>,
    title_bar: Entity<TitleBar>,
    bottom_bar: Entity<BottomBar>,
    window_handle: Option<gpui::AnyWindowHandle>,
}

impl SoukouApp {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new(QUIT_SHORTCUT_MAC, Quit, None),
            KeyBinding::new(OPEN_SETTINGS_SHORTCUT_CTRL, OpenSettings, None),
            KeyBinding::new(OPEN_FILE_SHORTCUT_MAC, OpenFile, None),
            KeyBinding::new(OPEN_FILE_SHORTCUT_CTRL, OpenFile, None),
            KeyBinding::new(SAVE_FILE_SHORTCUT_MAC, SaveFile, None),
            KeyBinding::new(SAVE_FILE_SHORTCUT_CTRL, SaveFile, None),
        ]);

        let editor_controller = cx.new(EditorController::new);
        let title_bar = cx.new(|cx| TitleBar::new(APP_NAME, Self::title_bar_menus(), cx));
        let bottom_bar = cx.new(BottomBar::new);

        Self {
            editor_controller,
            active_file: None,
            title_bar,
            bottom_bar,
            window_handle: None,
        }
    }

    fn title_bar_menus() -> Vec<TitleBarMenu> {
        vec![
            MenuBarMenu::new(
                APP_NAME,
                vec![
                    MenuBarItem::new(SETTINGS_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(OpenSettings), cx);
                    }),
                    MenuBarItem::new(CHECK_FOR_UPDATES_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(CheckForUpdates), cx);
                    }),
                    MenuBarItem::new(QUIT_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(Quit), cx);
                    }),
                ],
            ),
            MenuBarMenu::new(
                FILE_MENU_LABEL,
                vec![
                    MenuBarItem::new(OPEN_PROMPT_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(OpenFile), cx);
                    }),
                    MenuBarItem::new(SAVE_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(SaveFile), cx);
                    }),
                ],
            ),
        ]
    }

    fn window_title(&self, _cx: &App) -> String {
        match &self.active_file {
            Some(path) => format!("{APP_NAME}{WINDOW_TITLE_SEPARATOR}{}", path.display()),
            _ => APP_NAME.to_string(),
        }
    }

    fn sync_window_title(&self, window: &mut Window, cx: &App) {
        window.set_window_title(&self.window_title(cx));
    }

    // TODO Future
    fn show_error(window: &mut Window, message: &str, detail: String, cx: &mut App) {
        let _ = window.prompt(
            PromptLevel::Warning,
            message,
            Some(detail.as_str()),
            &[OK_BUTTON_LABEL],
            cx,
        );
    }

    fn is_supported_text_file(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case(SUPPORTED_TEXT_FILE_EXTENSION))
    }

    fn load_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_text(text, cx)
        });
        self.active_file = Some(path);
    }

    fn open_standalone_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_text(text, cx)
        });
        self.active_file = Some(path);
    }

    fn release_tag_to_version(tag_name: &str) -> Result<Version, String> {
        let normalized_tag = tag_name.trim().trim_start_matches('v');
        Version::parse(normalized_tag)
            .map_err(|error| format!("リリースタグ {tag_name} を解析できませんでした: {error}"))
    }

    fn fetch_available_update() -> Result<Option<AvailableUpdate>, String> {
        let current_version = Version::parse(APP_VERSION)
            .map_err(|error| format!("現在のバージョンを解析できませんでした: {error}"))?;
        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("soukou/{APP_VERSION}"))
            .build()
            .map_err(|error| format!("HTTPクライアントを初期化できませんでした: {error}"))?;
        let release = client
            .get(RELEASES_LATEST_API_URL)
            .send()
            .map_err(|error| format!("GitHub Release の取得に失敗しました: {error}"))?
            .error_for_status()
            .map_err(|error| format!("GitHub Release の取得に失敗しました: {error}"))?
            .json::<GitHubRelease>()
            .map_err(|error| format!("GitHub Release の応答を解析できませんでした: {error}"))?;
        let latest_version = Self::release_tag_to_version(release.tag_name.as_str())?;

        if latest_version > current_version {
            return Ok(Some(AvailableUpdate {
                current_version,
                latest_version,
                release_page_url: release.html_url,
            }));
        }

        Ok(None)
    }

    fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |_, cx| {
            let update_result = cx.background_spawn(async { Self::fetch_available_update() }).await;

            match update_result {
                Ok(Some(available_update)) => {
                    let detail = format!(
                        "現在のバージョン: v{}\n最新バージョン: v{}\n\nダウンロードページを開きますか？",
                        available_update.current_version, available_update.latest_version
                    );
                    let answer = cx.prompt(
                        PromptLevel::Info,
                        UPDATE_AVAILABLE_TITLE,
                        Some(detail.as_str()),
                        &[DOWNLOAD_BUTTON_LABEL, CANCEL_BUTTON_LABEL],
                    );

                    if answer.await.ok() == Some(0) {
                        let _ = cx.update(|_, cx| {
                            cx.open_url(available_update.release_page_url.as_str());
                        });
                    }
                }
                Ok(None) => {
                    let detail = format!("現在のバージョン v{APP_VERSION} は最新版です。");
                    let _ = cx.prompt(
                        PromptLevel::Info,
                        UPDATE_NOT_AVAILABLE_TITLE,
                        Some(detail.as_str()),
                        &[OK_BUTTON_LABEL],
                    );
                }
                Err(detail) => {
                    let _ = cx.prompt(
                        PromptLevel::Warning,
                        UPDATE_CHECK_ERROR_TITLE,
                        Some(detail.as_str()),
                        &[OK_BUTTON_LABEL],
                    );
                }
            }
        })
        .detach();
    }

    fn save_document_to_path(
        &mut self,
        path: PathBuf,
        contents: String,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    std::fs::write(&path, contents.as_bytes()).map(|_| path)
                })
                .await;

            match result {
                Ok(path) => {
                    let _ = this.update(cx, |this, cx| {
                        this.active_file = Some(path);
                        cx.notify();
                    });
                }
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, FILE_SAVE_ERROR_TITLE, detail, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn open_document_path(
        &mut self,
        path: PathBuf,
        preserve_workspace: bool,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if !Self::is_supported_text_file(path.as_path()) {
            let _ = cx.update_window(window_handle, |_, window, cx| {
                Self::show_error(
                    window,
                    FILE_OPEN_ERROR_TITLE,
                    UNSUPPORTED_TEXT_FILE_ERROR_DETAIL.into(),
                    cx,
                );
            });
            return;
        }

        cx.spawn(async move |this, cx| {
            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move {
                    std::fs::read_to_string(&path_for_read).map(|text| (path_for_read, text))
                })
                .await;

            match result {
                Ok((path, text)) => {
                    let _ = this.update(cx, |this, cx| {
                        if preserve_workspace {
                            this.load_document(path, &text, cx);
                        } else {
                            this.open_standalone_document(path, &text, cx);
                        }
                    });
                }
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, FILE_OPEN_ERROR_TITLE, detail, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(OPEN_PROMPT_LABEL.into()),
        });
        let window_handle = window.window_handle();

        cx.spawn(async move |this, cx| {
            let Ok(result) = picker.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(Some(mut paths)) => paths.pop(),
                Ok(None) => None,
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, FILE_PICKER_ERROR_TITLE, detail, cx);
                    });
                    None
                }
            }) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.open_document_path(path, false, window_handle, cx);
            });
        })
        .detach();
    }

    fn open_dropped_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = paths
            .paths()
            .iter()
            .find(|path| Self::is_supported_text_file(path.as_path()))
            .cloned()
        else {
            Self::show_error(
                window,
                FILE_OPEN_ERROR_TITLE,
                UNSUPPORTED_TEXT_FILE_ERROR_DETAIL.into(),
                cx,
            );
            return;
        };

        self.open_document_path(path, true, window.window_handle(), cx);
    }

    fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let contents = self.editor_controller.read(cx).snapshot_text(cx);
        let window_handle = window.window_handle();

        if let Some(path) = self.active_file.clone() {
            self.save_document_to_path(path, contents, window_handle, cx);
            return;
        }

        let initial_directory = self
            .active_file
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK));
        let suggested_name = self
            .active_file
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or(DEFAULT_NEW_FILE_NAME)
            .to_string();
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));

        cx.spawn(async move |this, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, SAVE_PATH_PICKER_ERROR_TITLE, detail, cx);
                    });
                    None
                }
            }) else {
                return;
            };

            let _ = this.update(cx, |this, cx| {
                this.save_document_to_path(path, contents, window_handle, cx);
            });
        })
        .detach();
    }

    fn open_file_action(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        self.open_file(window, cx);
    }

    fn save_file_action(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        self.save_file(window, cx);
    }

    fn check_for_updates_action(
        &mut self,
        _: &CheckForUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.check_for_updates(window, cx);
    }

    fn vim_command_write_action(
        &mut self,
        _: &VimCommandWrite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_file(window, cx);
    }

    fn vim_command_quit_action(
        &mut self,
        _: &VimCommandQuit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.quit();
    }

    fn drop_external_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_dropped_paths(paths, window, cx);
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
                    .can_drop(|value, _, _| value.is::<ExternalPaths>())
                    .on_drop(cx.listener(Self::drop_external_paths))
                    .on_action(cx.listener(Self::open_file_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
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
                    .child(self.bottom_bar.clone().into_element()),
            )
    }
}

impl Focusable for SoukouApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor_controller.focus_handle(cx)
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        font::init(cx);
        theme::init(cx);
        settings::init(cx);
        editor::init(cx);

        cx.on_action(|_: &Quit, cx| cx.quit())
            .on_action(|_: &OpenSettings, cx| open_settings_window(cx))
            .set_menus(vec![
                Menu {
                    disabled: false,
                    name: APP_NAME.into(),
                    items: vec![
                        MenuItem::action(SETTINGS_MENU_LABEL, OpenSettings),
                        MenuItem::action(CHECK_FOR_UPDATES_MENU_LABEL, CheckForUpdates),
                        MenuItem::separator(),
                        MenuItem::action(QUIT_MENU_LABEL, Quit),
                    ],
                },
                Menu {
                    disabled: false,
                    name: FILE_MENU_LABEL.into(),
                    items: vec![
                        MenuItem::action(OPEN_PROMPT_LABEL, OpenFile),
                        MenuItem::action(SAVE_MENU_LABEL, SaveFile),
                    ],
                },
            ]);

        cx.open_window(
            title_bar::configure_window_options(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(MAIN_WINDOW_WIDTH), px(MAIN_WINDOW_HEIGHT)),
                    cx,
                ))),
                app_id: Some(APP_ID.into()),
                is_movable: true,
                is_resizable: true,
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            }),
            |_, cx| cx.new(SoukouApp::new),
        )
        .and_then(|window| {
            window.update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.activate(true);
            })
        })
        .expect("Failed to open main window")
    })
}
