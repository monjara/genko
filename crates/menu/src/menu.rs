use std::path::PathBuf;

use gpui::{
    AnyWindowHandle, App, AppContext, Context, Menu, MenuItem, PathPromptOptions, Window, actions,
};
use semver::Version;
use serde::Deserialize;
use title_bar::TitleBarMenu;
use ui::{MenuBarItem, MenuBarMenu};

actions!(
    menu,
    [
        OpenSettings,
        CheckForUpdates,
        OpenFile,
        SaveFile,
        ExportTxt,
        ExportEpub,
        Quit
    ]
);

pub const APP_NAME: &str = "草稿";
const SETTINGS_MENU_LABEL: &str = "設定";
const CHECK_FOR_UPDATES_MENU_LABEL: &str = "更新を確認";
const QUIT_MENU_LABEL: &str = "終了";
const FILE_MENU_LABEL: &str = "ファイル";
const OPEN_DOCUMENT_PROMPT_LABEL: &str = "開く";
const SAVE_DOCUMENT_MENU_LABEL: &str = "保存";
const EXPORT_TXT_MENU_LABEL: &str = "txtエクスポート";
const EXPORT_EPUB_MENU_LABEL: &str = "epubエクスポート";
const FILE_PICKER_ERROR_TITLE: &str = "ファイル選択を開けませんでした";
const SAVE_PATH_PICKER_ERROR_TITLE: &str = "保存先を選択できませんでした";
const EXPORT_ERROR_TITLE: &str = "書き出しを開始できませんでした";
const UPDATE_CHECK_ERROR_TITLE: &str = "更新を確認できませんでした";
const UPDATE_NOT_AVAILABLE_TITLE: &str = "最新版を使用しています";
const RELEASES_LATEST_API_URL: &str = "https://api.github.com/repos/monjara/genko/releases/latest";

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

pub fn init(cx: &mut App) {
    cx.set_menus(vec![
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
                MenuItem::action(OPEN_DOCUMENT_PROMPT_LABEL, OpenFile),
                MenuItem::action(SAVE_DOCUMENT_MENU_LABEL, SaveFile),
                MenuItem::separator(),
                MenuItem::action(EXPORT_TXT_MENU_LABEL, ExportTxt),
                MenuItem::action(EXPORT_EPUB_MENU_LABEL, ExportEpub),
            ],
        },
    ])
}

pub fn title_bar_menus() -> Vec<TitleBarMenu> {
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
                MenuBarItem::new(OPEN_DOCUMENT_PROMPT_LABEL, |window, cx| {
                    window.dispatch_action(Box::new(OpenFile), cx);
                }),
                MenuBarItem::new(SAVE_DOCUMENT_MENU_LABEL, |window, cx| {
                    window.dispatch_action(Box::new(SaveFile), cx);
                }),
                MenuBarItem::new(EXPORT_TXT_MENU_LABEL, |window, cx| {
                    window.dispatch_action(Box::new(ExportTxt), cx);
                }),
                MenuBarItem::new(EXPORT_EPUB_MENU_LABEL, |window, cx| {
                    window.dispatch_action(Box::new(ExportEpub), cx);
                }),
            ],
        ),
    ]
}

pub trait MenuActionHandler: Sized + 'static {
    fn app_version(&self) -> &'static str;

    fn open_path_from_menu(&mut self, path: PathBuf, cx: &mut Context<Self>);

    fn save_blocking_error(&self, cx: &App) -> Option<(&'static str, String)>;

    fn active_save_path(&self, cx: &App) -> Option<PathBuf>;

    fn suggested_save_directory(&self, cx: &App) -> PathBuf;

    fn suggested_file_name(&self, cx: &App) -> String;

    fn save_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    );

    fn export_base_name(&self, cx: &App) -> String;

    fn export_initial_directory(&self, cx: &App) -> PathBuf;

    fn snapshot_text(&self, cx: &App) -> String;

    fn export_epub_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        cx: &mut Context<Self>,
    );

    fn show_menu_error(&mut self, title: &str, detail: String, cx: &mut Context<Self>);

    fn show_menu_info(&mut self, title: &str, detail: String, cx: &mut Context<Self>);

    fn show_update_available(
        &mut self,
        current_version: String,
        latest_version: String,
        release_page_url: String,
        cx: &mut Context<Self>,
    );

    fn open_file_action(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        self.open_file(window, cx);
    }

    fn save_file_action(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        self.save_file(window, cx);
    }

    fn export_txt_action(&mut self, _: &ExportTxt, window: &mut Window, cx: &mut Context<Self>) {
        self.export_txt(window, cx);
    }

    fn export_epub_action(&mut self, _: &ExportEpub, window: &mut Window, cx: &mut Context<Self>) {
        self.export_epub(window, cx);
    }

    fn check_for_updates_action(
        &mut self,
        _: &CheckForUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.check_for_updates(window, cx);
    }

    fn check_for_updates(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let app_version = self.app_version().to_string();
        cx.spawn(async move |this, cx| {
            let app_version_for_request = app_version.clone();
            let update_result = cx
                .background_spawn(async move {
                    fetch_available_update(app_version_for_request.as_str())
                })
                .await;

            if let Err(error) = this.update(cx, |this, cx| match update_result {
                Ok(Some(available_update)) => {
                    this.show_update_available(
                        format!("v{}", available_update.current_version),
                        format!("v{}", available_update.latest_version),
                        available_update.release_page_url,
                        cx,
                    );
                }
                Ok(None) => {
                    this.show_menu_info(
                        UPDATE_NOT_AVAILABLE_TITLE,
                        format!("現在のバージョン v{app_version} は最新版です。"),
                        cx,
                    );
                }
                Err(detail) => {
                    this.show_menu_error(UPDATE_CHECK_ERROR_TITLE, detail, cx);
                }
            }) {
                eprintln!("failed to show update check result: {error}");
            }
        })
        .detach();
    }

    fn open_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some(OPEN_DOCUMENT_PROMPT_LABEL.into()),
        });

        cx.spawn(async move |this, cx| {
            let Ok(result) = picker.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(Some(mut paths)) => paths.pop(),
                Ok(None) => None,
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_menu_error(FILE_PICKER_ERROR_TITLE, error.to_string(), cx);
                    }) {
                        eprintln!("failed to show file picker error: {update_error}");
                    }
                    None
                }
            }) else {
                return;
            };

            if let Err(error) = this.update(cx, |this, cx| {
                this.open_path_from_menu(path, cx);
            }) {
                eprintln!("failed to open selected path: {error}");
            }
        })
        .detach();
    }

    fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((title, detail)) = self.save_blocking_error(cx) {
            self.show_menu_error(title, detail, cx);
            return;
        }

        let window_handle = window.window_handle();
        let contents = self.snapshot_text(cx);

        if let Some(path) = self.active_save_path(cx) {
            self.save_path_from_menu(path, contents, window_handle, cx);
            return;
        }

        let initial_directory = self.suggested_save_directory(cx);
        let suggested_name = self.suggested_file_name(cx);
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));

        cx.spawn(async move |this, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_menu_error(SAVE_PATH_PICKER_ERROR_TITLE, error.to_string(), cx);
                    }) {
                        eprintln!("failed to show save path picker error: {update_error}");
                    }
                    None
                }
            }) else {
                return;
            };

            if let Err(error) = this.update(cx, |this, cx| {
                this.save_path_from_menu(path, contents, window_handle, cx);
            }) {
                eprintln!("failed to save selected path: {error}");
            }
        })
        .detach();
    }

    fn export_txt(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let suggested_name = format!("{}.txt", self.export_base_name(cx));
        let initial_directory = self.export_initial_directory(cx);
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));
        let contents = self.snapshot_text(cx);

        cx.spawn(async move |this, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_menu_error(SAVE_PATH_PICKER_ERROR_TITLE, error.to_string(), cx);
                    }) {
                        eprintln!("failed to show export path picker error: {update_error}");
                    }
                    None
                }
            }) else {
                return;
            };

            let write_result = cx
                .background_spawn(async move { std::fs::write(path.as_path(), contents) })
                .await;

            if let Err(error) = write_result {
                if let Err(update_error) = this.update(cx, |this, cx| {
                    this.show_menu_error(EXPORT_ERROR_TITLE, error.to_string(), cx);
                }) {
                    eprintln!("failed to show export error: {update_error}");
                }
            }
        })
        .detach();
    }

    fn export_epub(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let suggested_name = format!("{}.epub", self.export_base_name(cx));
        let initial_directory = self.export_initial_directory(cx);
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));
        let contents = self.snapshot_text(cx);

        cx.spawn(async move |this, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_menu_error(SAVE_PATH_PICKER_ERROR_TITLE, error.to_string(), cx);
                    }) {
                        eprintln!("failed to show epub export path picker error: {update_error}");
                    }
                    None
                }
            }) else {
                return;
            };

            if let Err(error) = this.update(cx, |this, cx| {
                this.export_epub_path_from_menu(path, contents, cx);
            }) {
                eprintln!("failed to export epub: {error}");
            }
        })
        .detach();
    }
}

fn release_tag_to_version(tag_name: &str) -> Result<Version, String> {
    let normalized_tag = tag_name.trim().trim_start_matches('v');
    Version::parse(normalized_tag)
        .map_err(|error| format!("リリースタグ {tag_name} を解析できませんでした: {error}"))
}

fn fetch_available_update(app_version: &str) -> Result<Option<AvailableUpdate>, String> {
    let current_version = Version::parse(app_version)
        .map_err(|error| format!("現在のバージョンを解析できませんでした: {error}"))?;
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("soukou/{app_version}"))
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
    let latest_version = release_tag_to_version(release.tag_name.as_str())?;

    if latest_version > current_version {
        return Ok(Some(AvailableUpdate {
            current_version,
            latest_version,
            release_page_url: release.html_url,
        }));
    }

    Ok(None)
}
