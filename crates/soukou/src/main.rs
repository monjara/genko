mod auth;
mod document;
mod font;

use std::path::{Path, PathBuf};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::process::Command;
use std::{cell::RefCell, rc::Rc};

use bottom_bar::BottomBar;
use document::{ActiveDocument, DocumentKind, ExportFormat};
use document_export as export;
use editor::{EditorController, VimCommandQuit, VimCommandWrite};
use futures::StreamExt;
use gpui::{
    Anchor, App, AppContext, AsyncApp, Bounds, BoxShadow, Context, Decorations, Entity,
    ExternalPaths, FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement, KeyBinding,
    Menu, MenuItem, ParentElement, PathPromptOptions, PromptLevel, Render, Styled, WeakEntity,
    Window, WindowBounds, WindowDecorations, WindowOptions, actions, anchored, deferred, div,
    point, prelude::FluentBuilder, px, size, transparent_black,
};
use richtext::{BlockKind, InlineStyle, RichDocument, single_change};
use semver::Version;
use serde::Deserialize;
use settings::{AppSettings, ExportTargetFormat, ExportWritingMode, open_settings_window};
use theme::{APP_FONT_FAMILY, Theme};
use title_bar::{TitleBar, TitleBarAuthActions, TitleBarAuthState, TitleBarMenu, TitleBarUser};
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
const EXPORT_WORD_MENU_LABEL: &str = "Wordを書き出し";
const EXPORT_EPUB_MENU_LABEL: &str = "EPUBを書き出し";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const FILE_PICKER_ERROR_TITLE: &str = "ファイル選択を開けませんでした";
const SAVE_PATH_PICKER_ERROR_TITLE: &str = "保存先を選択できませんでした";
const EXPORT_ERROR_TITLE: &str = "書き出しを開始できませんでした";
const PRO_REQUIRED_TITLE: &str = "Proプランが必要です";
const UPDATE_CHECK_ERROR_TITLE: &str = "更新を確認できませんでした";
const UPDATE_AVAILABLE_TITLE: &str = "新しいバージョンがあります";
const UPDATE_NOT_AVAILABLE_TITLE: &str = "最新版を使用しています";
const CURRENT_DIRECTORY_FALLBACK: &str = ".";
const WINDOW_TITLE_SEPARATOR: &str = " - ";
const MAIN_WINDOW_WIDTH: f32 = 1200.0;
const MAIN_WINDOW_HEIGHT: f32 = 800.0;
const OPEN_FILE_SHORTCUT_MAC: &str = "cmd-o";
const OPEN_FILE_SHORTCUT_CTRL: &str = "ctrl-o";
const SAVE_FILE_SHORTCUT_MAC: &str = "cmd-s";
const SAVE_FILE_SHORTCUT_CTRL: &str = "ctrl-s";
const TOGGLE_BOLD_SHORTCUT_MAC: &str = "cmd-b";
const TOGGLE_BOLD_SHORTCUT_CTRL: &str = "ctrl-b";
const TOGGLE_STRIKETHROUGH_SHORTCUT_MAC: &str = "cmd-shift-x";
const TOGGLE_STRIKETHROUGH_SHORTCUT_CTRL: &str = "ctrl-shift-x";
const QUIT_SHORTCUT_MAC: &str = "cmd-q";
const OPEN_SETTINGS_SHORTCUT_CTRL: &str = "ctrl-,";
const RELEASES_LATEST_API_URL: &str =
    "https://api.github.com/repos/monjara/Soukou.app/releases/latest";

actions!(
    soukou,
    [
        OpenSettings,
        CheckForUpdates,
        OpenFile,
        SaveFile,
        ToggleBold,
        ToggleStrikethrough,
        SetHeadingLarge,
        SetHeadingMedium,
        ClearHeading,
        ExportWord,
        ExportEpub,
        Quit,
        SignIn,
        OpenAccountSettings,
        SignOut
    ]
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
    active_document: ActiveDocument,
    rich_document: Option<RichDocument>,
    last_richtext_revision: u64,
    title_bar: Entity<TitleBar>,
    bottom_bar: Entity<BottomBar>,
    window_handle: Option<gpui::AnyWindowHandle>,
    auth_state: auth::AuthState,
    auth_config: auth::AuthConfig,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FeatureGate {
    RichText,
    ExportWord,
    ExportEpub,
}

async fn read_stored_refresh_token(
    credentials_key: String,
    cx: &mut AsyncApp,
) -> Result<Option<String>, String> {
    let task = cx.update(|app| app.read_credentials(credentials_key.as_str()));
    let credentials = task
        .await
        .map_err(|error| format!("認証情報の読み込みに失敗しました: {error}"))?;

    let Some((_, password)) = credentials else {
        return Ok(None);
    };

    String::from_utf8(password)
        .map(Some)
        .map_err(|error| format!("保存済みトークンを解析できませんでした: {error}"))
}

async fn write_refresh_token(
    credentials_key: String,
    refresh_token: String,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let task = cx.update(|app| {
        app.write_credentials(
            credentials_key.as_str(),
            "refresh_token",
            refresh_token.as_bytes(),
        )
    });
    task.await
        .map_err(|error| format!("認証情報の保存に失敗しました: {error}"))?;
    Ok(())
}

async fn delete_refresh_token(credentials_key: String, cx: &mut AsyncApp) -> Result<(), String> {
    let task = cx.update(|app| app.delete_credentials(credentials_key.as_str()));
    task.await
        .map_err(|error| format!("認証情報の削除に失敗しました: {error}"))?;
    Ok(())
}

impl SoukouApp {
    fn open_external_url(&self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            let launch_result = ["xdg-open", "gio"]
                .into_iter()
                .find_map(|program| match program {
                    "xdg-open" => Command::new(program).arg(url).spawn().ok(),
                    "gio" => Command::new(program).arg("open").arg(url).spawn().ok(),
                    _ => None,
                });

            if launch_result.is_none() {
                Self::show_error(
                    window,
                    "ブラウザを起動できませんでした",
                    format!(
                        "URL を開けませんでした。`xdg-open` または `gio open` を利用できる環境か確認してください。\n\n{url}"
                    ),
                    cx,
                );
                return;
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        {
            cx.open_url(url);
        }

        window.activate_window();
    }

    fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new(QUIT_SHORTCUT_MAC, Quit, None),
            KeyBinding::new(OPEN_SETTINGS_SHORTCUT_CTRL, OpenSettings, None),
            KeyBinding::new(OPEN_FILE_SHORTCUT_MAC, OpenFile, None),
            KeyBinding::new(OPEN_FILE_SHORTCUT_CTRL, OpenFile, None),
            KeyBinding::new(SAVE_FILE_SHORTCUT_MAC, SaveFile, None),
            KeyBinding::new(SAVE_FILE_SHORTCUT_CTRL, SaveFile, None),
            KeyBinding::new(TOGGLE_BOLD_SHORTCUT_MAC, ToggleBold, None),
            KeyBinding::new(TOGGLE_BOLD_SHORTCUT_CTRL, ToggleBold, None),
            KeyBinding::new(
                TOGGLE_STRIKETHROUGH_SHORTCUT_MAC,
                ToggleStrikethrough,
                None,
            ),
            KeyBinding::new(
                TOGGLE_STRIKETHROUGH_SHORTCUT_CTRL,
                ToggleStrikethrough,
                None,
            ),
        ]);

        let editor_controller = cx.new(EditorController::new);
        let title_bar = cx.new(|cx| {
            TitleBar::new(
                APP_NAME,
                Self::title_bar_menus(),
                Some(TitleBarAuthActions::new(
                    |window, cx| window.dispatch_action(Box::new(SignIn), cx),
                    |window, cx| window.dispatch_action(Box::new(OpenAccountSettings), cx),
                    |window, cx| window.dispatch_action(Box::new(SignOut), cx),
                )),
                cx,
            )
        });
        let bottom_bar = cx.new(BottomBar::new);
        let auth_config = auth::AuthConfig::from_env();

        let mut app = Self {
            editor_controller,
            active_document: ActiveDocument::default(),
            rich_document: None,
            last_richtext_revision: 0,
            title_bar,
            bottom_bar,
            window_handle: None,
            auth_state: auth::AuthState::Restoring,
            auth_config,
        };
        app.sync_title_bar_auth_state(cx);
        app.sync_bottom_bar_plan(cx);
        app.restore_auth_session(cx);
        app
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
                    MenuBarItem::new(EXPORT_WORD_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(ExportWord), cx);
                    }),
                    MenuBarItem::new(EXPORT_EPUB_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(ExportEpub), cx);
                    }),
                ],
            ),
        ]
    }

    fn window_title(&self, _cx: &App) -> String {
        match self.active_document.path() {
            Some(path) => format!("{APP_NAME}{WINDOW_TITLE_SEPARATOR}{}", path.display()),
            _ => APP_NAME.to_string(),
        }
    }

    fn sync_window_title(&self, window: &mut Window, cx: &App) {
        window.set_window_title(&self.window_title(cx));
    }

    fn set_auth_state(&mut self, auth_state: auth::AuthState, cx: &mut Context<Self>) {
        self.auth_state = auth_state;
        self.sync_title_bar_auth_state(cx);
        self.sync_bottom_bar_plan(cx);
        cx.notify();
    }

    fn sync_title_bar_auth_state(&mut self, cx: &mut Context<Self>) {
        let title_bar_auth_state = match &self.auth_state {
            auth::AuthState::Authenticated(session) => {
                TitleBarAuthState::Authenticated(TitleBarUser {
                    display_name: session.user.display_name.clone(),
                    email: session.user.email.clone(),
                    avatar_url: session.user.avatar_url.clone(),
                })
            }
            auth::AuthState::Anonymous | auth::AuthState::Restoring => TitleBarAuthState::Anonymous,
        };

        self.title_bar.update(cx, |title_bar, cx| {
            title_bar.set_auth_state(title_bar_auth_state, cx);
        });
    }

    fn sync_bottom_bar_plan(&mut self, cx: &mut Context<Self>) {
        let label = self.current_plan_key().label().to_string();
        self.bottom_bar.update(cx, |bottom_bar, cx| {
            bottom_bar.set_plan_label(label, cx);
        });
    }

    fn current_plan_key(&self) -> auth::PlanKey {
        match &self.auth_state {
            auth::AuthState::Authenticated(session) => session.user.plan.plan_key,
            auth::AuthState::Anonymous | auth::AuthState::Restoring => auth::PlanKey::Free,
        }
    }

    fn is_feature_available(&self, feature: FeatureGate) -> bool {
        let plan = self.current_plan_key();
        match feature {
            FeatureGate::RichText
            | FeatureGate::ExportWord
            | FeatureGate::ExportEpub => plan.supports_rich_text(),
        }
    }

    fn sync_editor_richtext_projection(&mut self, cx: &mut Context<Self>) {
        let rich_document = self.rich_document.clone();
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_richtext_document(rich_document.as_ref(), cx);
        });
        self.last_richtext_revision = self.editor_controller.read(cx).draft_revision(cx);
    }

    fn sync_richtext_from_editor(&mut self, cx: &mut Context<Self>) {
        if self.active_document.kind() != DocumentKind::RichText {
            return;
        }

        let revision = self.editor_controller.read(cx).draft_revision(cx);
        if revision == self.last_richtext_revision {
            return;
        }

        let text = self.editor_controller.read(cx).snapshot_text(cx);
        if let Some(document) = self.rich_document.as_mut() {
            if let Some((range, replacement)) = single_change(document.plain_text(), &text) {
                document.replace_text(range, replacement.as_str());
            } else if document.plain_text() != text {
                *document = RichDocument::new(text);
            }
        }

        self.sync_editor_richtext_projection(cx);
    }

    fn ensure_richtext_document(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_feature_available(FeatureGate::RichText) {
            return false;
        }

        self.sync_richtext_from_editor(cx);
        if self.active_document.kind() == DocumentKind::RichText && self.rich_document.is_some() {
            return true;
        }

        let text = self.editor_controller.read(cx).snapshot_text(cx);
        self.active_document.set_kind(DocumentKind::RichText);
        self.rich_document = Some(RichDocument::new(text));
        self.sync_editor_richtext_projection(cx);
        true
    }

    fn apply_inline_style(
        &mut self,
        style: InlineStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_feature_available(FeatureGate::RichText) {
            self.prompt_pro_required(FeatureGate::RichText, window, cx);
            return;
        }
        if !self.ensure_richtext_document(cx) {
            return;
        }

        let selected_range = self.editor_controller.read(cx).selected_byte_range(cx);
        if selected_range.is_empty() {
            return;
        }

        if let Some(document) = self.rich_document.as_mut() {
            document.toggle_inline_style(selected_range, style);
        }
        self.sync_editor_richtext_projection(cx);
    }

    fn apply_block_kind(
        &mut self,
        kind: BlockKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_feature_available(FeatureGate::RichText) {
            self.prompt_pro_required(FeatureGate::RichText, window, cx);
            return;
        }
        if !self.ensure_richtext_document(cx) {
            return;
        }

        let selected_range = self.editor_controller.read(cx).selected_byte_range(cx);
        if let Some(document) = self.rich_document.as_mut() {
            document.set_block_kind_for_range(selected_range, kind);
        }
        self.sync_editor_richtext_projection(cx);
    }

    fn restore_auth_session(&mut self, cx: &mut Context<Self>) {
        let auth_config = self.auth_config.clone();
        let credentials_key = self.auth_config.credentials_key().to_string();

        cx.spawn(async move |this, cx| {
            let Some(this_entity) = this.upgrade() else {
                return;
            };

            let stored_refresh_token = read_stored_refresh_token(credentials_key.clone(), cx).await;

            let refresh_token = match stored_refresh_token {
                Ok(Some(refresh_token)) => refresh_token,
                Ok(None) => {
                    let _ = this_entity.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Anonymous, cx);
                    });
                    return;
                }
                Err(_) => {
                    let _ = this_entity.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Anonymous, cx);
                    });
                    return;
                }
            };

            let restored_session = cx
                .background_spawn({
                    let auth_config = auth_config.clone();
                    async move { auth::restore_session(&auth_config, refresh_token.as_str()) }
                })
                .await;

            match restored_session {
                Ok(session) => {
                    let _ = write_refresh_token(
                        credentials_key.clone(),
                        session.refresh_token.clone(),
                        cx,
                    )
                    .await;
                    let _ = this_entity.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Authenticated(session), cx);
                    });
                }
                Err(_) => {
                    let _ = delete_refresh_token(credentials_key.clone(), cx).await;
                    let _ = this_entity.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Anonymous, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn handle_open_urls(&mut self, urls: Vec<String>, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let credentials_key = self.auth_config.credentials_key().to_string();

        for url in urls {
            let callback =
                match auth::parse_callback(url.as_str(), self.auth_config.callback_scheme()) {
                    Ok(Some(callback)) => callback,
                    Ok(None) => continue,
                    Err(error) => {
                        if let Some(window_handle) = window_handle {
                            let _ = cx.update_window(window_handle, |_, window, cx| {
                                Self::show_error(
                                    window,
                                    "ログイン情報を処理できませんでした",
                                    error,
                                    cx,
                                );
                            });
                        }
                        continue;
                    }
                };

            self.apply_auth_callback(callback, credentials_key.clone(), window_handle, cx);
        }
    }

    fn apply_auth_callback(
        &mut self,
        callback: auth::AuthCallback,
        credentials_key: String,
        window_handle: Option<gpui::AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        match callback {
            auth::AuthCallback::SignedOut => {
                self.set_auth_state(auth::AuthState::Anonymous, cx);
                cx.spawn(move |_, cx: &mut AsyncApp| {
                    let mut app = cx.clone();
                    async move {
                        let _ = delete_refresh_token(credentials_key, &mut app).await;
                    }
                })
                .detach();
            }
            auth::AuthCallback::SignedIn { refresh_token } => {
                let auth_config = self.auth_config.clone();
                self.set_auth_state(auth::AuthState::Restoring, cx);

                cx.spawn(async move |this, cx| {
                    let restored_session = cx
                        .background_spawn(async move {
                            auth::restore_session(&auth_config, refresh_token.as_str())
                        })
                        .await;

                    let Some(this_entity) = this.upgrade() else {
                        return;
                    };

                    match restored_session {
                        Ok(session) => {
                            let save_result = write_refresh_token(
                                credentials_key.clone(),
                                session.refresh_token.clone(),
                                cx,
                            )
                            .await;

                            if let Err(error) = save_result {
                                if let Some(window_handle) = window_handle {
                                    let _ = cx.update_window(window_handle, |_, window, cx| {
                                        Self::show_error(
                                            window,
                                            "認証情報を保存できませんでした",
                                            error,
                                            cx,
                                        );
                                    });
                                }
                            }

                            let _ = this_entity.update(cx, |this, cx| {
                                this.set_auth_state(auth::AuthState::Authenticated(session), cx);
                            });
                        }
                        Err(error) => {
                            let _ = delete_refresh_token(credentials_key.clone(), cx).await;
                            let _ = this_entity.update(cx, |this, cx| {
                                this.set_auth_state(auth::AuthState::Anonymous, cx);
                            });
                            if let Some(window_handle) = window_handle {
                                let _ = cx.update_window(window_handle, |_, window, cx| {
                                    Self::show_error(window, "ログインに失敗しました", error, cx);
                                });
                            }
                        }
                    }
                })
                .detach();
            }
        }
    }

    fn sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.auth_config.is_supabase_configured() {
            Self::show_error(
                window,
                "ログインを開始できませんでした",
                "SOUKOU_SUPABASE_URL と SOUKOU_SUPABASE_PUBLISHABLE_KEY を設定してください。"
                    .to_string(),
                cx,
            );
            return;
        }

        let callback_listener = match auth::start_local_callback_server() {
            Ok(callback_listener) => callback_listener,
            Err(error) => {
                Self::show_error(window, "ログインを開始できませんでした", error, cx);
                return;
            }
        };

        let url = self
            .auth_config
            .sign_in_url(callback_listener.callback_url());
        let credentials_key = self.auth_config.credentials_key().to_string();
        let window_handle = self.window_handle;
        self.set_auth_state(auth::AuthState::Restoring, cx);

        cx.spawn(async move |this, cx| {
            let callback_result = cx
                .background_spawn(async move { callback_listener.wait_for_callback() })
                .await;

            let Some(this_entity) = this.upgrade() else {
                return;
            };

            match callback_result {
                Ok(callback) => {
                    let _ = this_entity.update(cx, |this, cx| {
                        this.apply_auth_callback(
                            callback,
                            credentials_key.clone(),
                            window_handle,
                            cx,
                        );
                    });
                }
                Err(error) => {
                    let _ = this_entity.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Anonymous, cx);
                        if let Some(window_handle) = window_handle {
                            let _ = cx.update_window(window_handle, |_, window, cx| {
                                Self::show_error(window, "ログインに失敗しました", error, cx);
                            });
                        }
                    });
                }
            }
        })
        .detach();

        self.open_external_url(url.as_str(), window, cx);
    }

    fn open_account_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.auth_config.account_url();
        self.open_external_url(url.as_str(), window, cx);
    }

    fn sign_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_auth_state(auth::AuthState::Anonymous, cx);

        let callback_listener = match auth::start_local_callback_server() {
            Ok(callback_listener) => callback_listener,
            Err(error) => {
                Self::show_error(window, "ログアウトを開始できませんでした", error, cx);
                return;
            }
        };

        let mut redirect_url = callback_listener.callback_url().to_string();
        redirect_url.push_str("?mode=signed_out");
        let sign_out_url = self.auth_config.sign_out_url(redirect_url.as_str());
        let credentials_key = self.auth_config.credentials_key().to_string();
        let window_handle = self.window_handle;

        cx.spawn(move |_, cx: &mut AsyncApp| {
            let mut app = cx.clone();
            async move {
                let _ = delete_refresh_token(credentials_key, &mut app).await;
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let callback_result = cx
                .background_spawn(async move { callback_listener.wait_for_callback() })
                .await;

            let Some(this_entity) = this.upgrade() else {
                return;
            };

            if let Err(error) = callback_result {
                let _ = this_entity.update(cx, |_, cx| {
                    if let Some(window_handle) = window_handle {
                        let _ = cx.update_window(window_handle, |_, window, cx| {
                            Self::show_error(window, "ログアウトを完了できませんでした", error, cx);
                        });
                    }
                });
            }
        })
        .detach();
        self.open_external_url(sign_out_url.as_str(), window, cx);
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

    fn is_supported_document_path(path: &Path) -> bool {
        DocumentKind::from_path(path).is_some()
    }

    fn load_plain_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_text(text, cx)
        });
        self.rich_document = None;
        self.active_document.set_path(path);
        self.sync_editor_richtext_projection(cx);
    }

    fn load_rich_document(&mut self, path: PathBuf, document: RichDocument, cx: &mut Context<Self>) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_text(document.plain_text(), cx)
        });
        self.rich_document = Some(document);
        self.active_document.set_path(path);
        self.sync_editor_richtext_projection(cx);
    }

    fn open_standalone_plain_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.load_plain_document(path, text, cx);
    }

    fn open_standalone_rich_document(
        &mut self,
        path: PathBuf,
        document: RichDocument,
        cx: &mut Context<Self>,
    ) {
        self.load_rich_document(path, document, cx);
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
                        this.active_document.set_path(path);
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
        let Some(document_kind) = DocumentKind::from_path(path.as_path()) else {
            let _ = cx.update_window(window_handle, |_, window, cx| {
                Self::show_error(
                    window,
                    FILE_OPEN_ERROR_TITLE,
                    DocumentKind::supported_open_error_detail().into(),
                    cx,
                );
            });
            return;
        };

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
                        match document_kind {
                            DocumentKind::PlainText => {
                                if preserve_workspace {
                                    this.load_plain_document(path, &text, cx);
                                } else {
                                    this.open_standalone_plain_document(path, &text, cx);
                                }
                            }
                            DocumentKind::RichText => {
                                match RichDocument::from_json(text.as_str()) {
                                    Ok(document) => {
                                        if preserve_workspace {
                                            this.load_rich_document(path, document, cx);
                                        } else {
                                            this.open_standalone_rich_document(path, document, cx);
                                        }
                                    }
                                    Err(error) => {
                                        if let Some(window_handle) = this.window_handle {
                                            let detail =
                                                format!("リッチテキスト文書を解析できませんでした: {error}");
                                            let _ = cx.update_window(
                                                window_handle,
                                                |_, window, cx| {
                                                    Self::show_error(
                                                        window,
                                                        FILE_OPEN_ERROR_TITLE,
                                                        detail,
                                                        cx,
                                                    );
                                                },
                                            );
                                        }
                                    }
                                }
                            }
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
            .find(|path| Self::is_supported_document_path(path.as_path()))
            .cloned()
        else {
            Self::show_error(
                window,
                FILE_OPEN_ERROR_TITLE,
                DocumentKind::supported_open_error_detail().into(),
                cx,
            );
            return;
        };

        self.open_document_path(path, true, window.window_handle(), cx);
    }

    fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_richtext_from_editor(cx);
        let window_handle = window.window_handle();
        let contents = match self.active_document.kind() {
            DocumentKind::PlainText => self.editor_controller.read(cx).snapshot_text(cx),
            DocumentKind::RichText => match self.rich_document.as_ref().and_then(|document| document.to_json().ok()) {
                Some(json) => json,
                None => {
                    Self::show_error(
                        window,
                        FILE_SAVE_ERROR_TITLE,
                        "リッチテキスト文書を保存形式へ変換できませんでした".to_string(),
                        cx,
                    );
                    return;
                }
            },
        };

        if let Some(path) = self.active_document.path().map(Path::to_path_buf) {
            self.save_document_to_path(path, contents, window_handle, cx);
            return;
        }

        let initial_directory = self
            .active_document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK));
        let suggested_name = self
            .active_document
            .path()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or(self.active_document.kind().default_file_name())
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

    fn prompt_pro_required(&mut self, feature: FeatureGate, window: &mut Window, cx: &mut Context<Self>) {
        let feature_label = match feature {
            FeatureGate::RichText => "リッチテキスト編集",
            FeatureGate::ExportWord => "Word書き出し",
            FeatureGate::ExportEpub => "EPUB書き出し",
        };
        let detail = format!(
            "{feature_label} は Pro プランで利用できます。\n\nアカウント設定を開いてプラン管理を確認しますか？"
        );
        let account_url = self.auth_config.account_url();

        cx.spawn_in(window, async move |_, cx| {
            let answer = cx.prompt(
                PromptLevel::Info,
                PRO_REQUIRED_TITLE,
                Some(detail.as_str()),
                &["アカウント設定", CANCEL_BUTTON_LABEL],
            );

            if answer.await.ok() == Some(0) {
                let _ = cx.update(|_, cx| {
                    cx.open_url(account_url.as_str());
                });
            }
        })
        .detach();
    }

    fn export_document(&mut self, format: ExportFormat, window: &mut Window, cx: &mut Context<Self>) {
        let feature = match format {
            ExportFormat::Word => FeatureGate::ExportWord,
            ExportFormat::Epub => FeatureGate::ExportEpub,
        };

        if !self.is_feature_available(feature) {
            self.prompt_pro_required(feature, window, cx);
            return;
        }
        self.sync_richtext_from_editor(cx);

        let base_name = self
            .active_document
            .path()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .unwrap_or("untitled");
        let suggested_name = format!(
            "{}.{}",
            base_name,
            match format {
                ExportFormat::Word => export::ExportFormat::Word.file_extension(),
                ExportFormat::Epub => export::ExportFormat::Epub.file_extension(),
            }
        );
        let initial_directory = self
            .active_document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK));
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));
        let window_handle = window.window_handle();
        let rich_document = self
            .rich_document
            .clone()
            .unwrap_or_else(|| RichDocument::new(self.editor_controller.read(cx).snapshot_text(cx)));
        let export_format = match format {
            ExportFormat::Word => export::ExportFormat::Word,
            ExportFormat::Epub => export::ExportFormat::Epub,
        };
        let export_target = match format {
            ExportFormat::Word => ExportTargetFormat::Word,
            ExportFormat::Epub => ExportTargetFormat::Epub,
        };
        let export_options = export::ExportOptions {
            writing_mode: match AppSettings::global(cx)
                .export_settings
                .format(export_target)
                .writing_mode
            {
                ExportWritingMode::Vertical => export::ExportWritingMode::Vertical,
                ExportWritingMode::Horizontal => export::ExportWritingMode::Horizontal,
            },
        };

        cx.spawn(async move |_, cx| {
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

            let write_result = cx
                .background_spawn(async move {
                    export::write_export(path.as_path(), export_format, &rich_document, export_options)
                })
                .await;

            if let Err(error) = write_result {
                let detail = error.to_string();
                let _ = cx.update_window(window_handle, |_, window, cx| {
                    Self::show_error(window, EXPORT_ERROR_TITLE, detail, cx);
                });
            }
        })
        .detach();
    }

    fn open_file_action(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        self.open_file(window, cx);
    }

    fn save_file_action(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        self.save_file(window, cx);
    }

    fn toggle_bold_action(&mut self, _: &ToggleBold, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_inline_style(InlineStyle::Bold, window, cx);
    }

    fn toggle_strikethrough_action(
        &mut self,
        _: &ToggleStrikethrough,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_style(InlineStyle::Strikethrough, window, cx);
    }

    fn set_heading_large_action(
        &mut self,
        _: &SetHeadingLarge,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_block_kind(BlockKind::HeadingLarge, window, cx);
    }

    fn set_heading_medium_action(
        &mut self,
        _: &SetHeadingMedium,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_block_kind(BlockKind::HeadingMedium, window, cx);
    }

    fn clear_heading_action(
        &mut self,
        _: &ClearHeading,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_block_kind(BlockKind::Body, window, cx);
    }

    fn export_word_action(
        &mut self,
        _: &ExportWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document(ExportFormat::Word, window, cx);
    }

    fn export_epub_action(
        &mut self,
        _: &ExportEpub,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document(ExportFormat::Epub, window, cx);
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

    fn sign_in_action(&mut self, _: &SignIn, window: &mut Window, cx: &mut Context<Self>) {
        self.sign_in(window, cx);
    }

    fn open_account_settings_action(
        &mut self,
        _: &OpenAccountSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_account_settings(window, cx);
    }

    fn sign_out_action(&mut self, _: &SignOut, window: &mut Window, cx: &mut Context<Self>) {
        self.sign_out(window, cx);
    }

    fn render_richtext_toolbar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.is_feature_available(FeatureGate::RichText) {
            return None;
        }

        let selected_range = self.editor_controller.read(cx).selected_byte_range(cx);
        if selected_range.is_empty() {
            return None;
        }
        let selection_bounds = self.editor_controller.read(cx).selection_bounds(cx)?;
        let popup_position = point(
            selection_bounds.left(),
            selection_bounds.top() - px(56.0),
        );

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
                        .child(toolbar_button("B", |window, cx| {
                            window.dispatch_action(Box::new(ToggleBold), cx);
                        }))
                        .child(toolbar_button("S", |window, cx| {
                            window.dispatch_action(Box::new(ToggleStrikethrough), cx);
                        }))
                        .child(toolbar_button("大見出し", |window, cx| {
                            window.dispatch_action(Box::new(SetHeadingLarge), cx);
                        }))
                        .child(toolbar_button("小見出し", |window, cx| {
                            window.dispatch_action(Box::new(SetHeadingMedium), cx);
                        }))
                        .child(toolbar_button("本文", |window, cx| {
                            window.dispatch_action(Box::new(ClearHeading), cx);
                        })),
                ),
        ))
    }
}

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

fn toolbar_border_color(cx: &App) -> gpui::Hsla {
    mix(Theme::global(cx).black(), Theme::global(cx).white(), 0.72).into()
}

fn mix(left: gpui::Rgba, right: gpui::Rgba, ratio: f32) -> gpui::Rgba {
    let ratio = ratio.clamp(0.0, 1.0);
    let inv = 1.0 - ratio;
    gpui::Rgba {
        r: left.r * inv + right.r * ratio,
        g: left.g * inv + right.g * ratio,
        b: left.b * inv + right.b * ratio,
        a: left.a * inv + right.a * ratio,
    }
}

impl Render for SoukouApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_richtext_from_editor(cx);
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
                    .on_action(cx.listener(Self::toggle_bold_action))
                    .on_action(cx.listener(Self::toggle_strikethrough_action))
                    .on_action(cx.listener(Self::set_heading_large_action))
                    .on_action(cx.listener(Self::set_heading_medium_action))
                    .on_action(cx.listener(Self::clear_heading_action))
                    .on_action(cx.listener(Self::export_word_action))
                    .on_action(cx.listener(Self::export_epub_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
                    .on_action(cx.listener(Self::sign_in_action))
                    .on_action(cx.listener(Self::open_account_settings_action))
                    .on_action(cx.listener(Self::sign_out_action))
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
    if env::development_mode() {
        let _ = dotenvy::dotenv();
    }

    let (open_url_tx, open_url_rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
    let application = gpui_platform::application();
    application.on_open_urls(move |urls| {
        let _ = open_url_tx.unbounded_send(urls);
    });

    application.run(move |cx: &mut App| {
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
                        MenuItem::separator(),
                        MenuItem::action(EXPORT_WORD_MENU_LABEL, ExportWord),
                        MenuItem::action(EXPORT_EPUB_MENU_LABEL, ExportEpub),
                    ],
                },
            ]);

        let main_app = Rc::new(RefCell::new(None::<WeakEntity<SoukouApp>>));
        let main_app_for_build = main_app.clone();

        let main_window = cx
            .open_window(
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
                move |_, cx| {
                    let entity = cx.new(SoukouApp::new);
                    *main_app_for_build.borrow_mut() = Some(entity.downgrade());
                    entity
                },
            )
            .expect("Failed to open main window");

        main_window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.activate(true);
            })
            .expect("Failed to focus main window");

        let mut open_url_rx = open_url_rx;
        let main_app = main_app
            .borrow()
            .clone()
            .expect("Main app should be available after opening the window");

        cx.spawn(move |cx: &mut AsyncApp| {
            let mut app = cx.clone();
            async move {
                while let Some(urls) = open_url_rx.next().await {
                    let Some(main_app) = main_app.upgrade() else {
                        break;
                    };
                    let _ = main_app.update(&mut app, |this: &mut SoukouApp, cx| {
                        this.handle_open_urls(urls, cx);
                    });
                }
            }
        })
        .detach();

        let callback_prefix = format!("{}://", auth::AuthConfig::from_env().callback_scheme());
        let startup_urls = std::env::args()
            .skip(1)
            .filter(|arg| arg.starts_with(callback_prefix.as_str()))
            .collect::<Vec<_>>();
        if !startup_urls.is_empty() {
            let _ = main_window.update(cx, |this, _, cx| {
                this.handle_open_urls(startup_urls, cx);
            });
        }
    })
}
