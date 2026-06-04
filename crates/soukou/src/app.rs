use std::path::{Path, PathBuf};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::process::Command;

use crate::{
    CancelRubyEditor, DismissActiveModal, OpenModalPrimary,
    notification::{
        DismissErrorNotification, ErrorNotification, ErrorNotificationStack, ErrorPresentation,
    },
};
use bottom_bar::BottomBar;
use document::{
    ActiveDocument, DocumentError, DocumentKind,
    document_io::{
        DocumentOpenTarget, SavedDocumentTarget, classify_open_path, classify_saved_document_path,
        current_directory_or_fallback, suggested_file_name, suggested_save_directory,
    },
};
use editor::{
    ApplyRichTextBold as EditorApplyRichTextBold,
    ApplyRichTextEmphasis as EditorApplyRichTextEmphasis,
    ApplyRichTextHeading as EditorApplyRichTextHeading, ApplyRubyEdit, CancelRubyEdit,
    EditorController, Event as EditorEvent, PageBreakMenu, PageBreakMenuRequest,
    RemovePageBreakColumn, RichTextToolbar, RubyEditRequest, RubyEditorPopover,
    SetPageBreakLeftOfColumn, SetPageBreakRightOfColumn, VimCommandQuit, VimCommandWrite,
};
use gpui::{
    AnyWindowHandle, App, AppContext, BoxShadow, Context, Entity, ExternalPaths, FocusHandle,
    Focusable, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
    RenderOnce, Styled, Subscription, Window, div, point, prelude::FluentBuilder, px, svg,
    transparent_black,
};
use menu::{MenuActionHandler, RegisterAccount, SignOut};
use rich_text::{RichTextDocumentMeta, RichTextKind};
use theme::{APP_FONT_FAMILY, Theme};
use title_bar::TitleBar;
use ui::TextInput;
use workspace::{
    Event as WorkspaceEvent, OpenWorkspace, ToggleWorkspacePane, Workspace, WorkspaceState,
    scan_workspace_entries,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const UPDATE_AVAILABLE_TITLE: &str = "新しいバージョンがあります";
const WINDOW_TITLE_SEPARATOR: &str = " - ";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const SUPPORTED_OPEN_ERROR_DETAIL: &str = "現在は .txt ファイルに対応しています";
const UNSUPPORTED_DOCUMENT_SAVE_ERROR_DETAIL: &str = "サポートしていないファイルは保存できません";
const BOTTOM_BAR_TOP_GAP: f32 = 4.0;

pub(super) struct SoukouApp {
    editor_controller: Entity<EditorController>,
    workspace: Entity<Workspace>,
    active_document: ActiveDocument,
    ruby_editor: Option<RubyEditorState>,
    page_break_menu: Option<PageBreakMenuState>,
    active_modal: Option<AppModal>,
    error_notifications: Vec<ErrorNotification>,
    next_error_notification_id: u64,
    auth_config: auth::AuthConfig,
    auth_state: auth::AuthState,
    account_control: Entity<auth::TitleBarAccountControl>,
    _workspace_subscription: Subscription,
    _editor_subscription: Subscription,
    title_bar: Entity<TitleBar>,
    bottom_bar: Entity<BottomBar>,
}

#[derive(Clone, Debug)]
enum AppModal {
    Info {
        title: String,
        detail: String,
    },
    ProRequired {
        feature: ProFeature,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
        release_page_url: String,
    },
}

struct RubyEditorState {
    request: RubyEditRequest,
    input: Entity<TextInput>,
}

#[derive(Clone, Debug)]
struct PageBreakMenuState {
    request: PageBreakMenuRequest,
}

#[derive(Clone, Copy, Debug)]
enum ProFeature {
    RichText,
    ExportWord,
    ExportEpub,
}

impl SoukouApp {
    fn open_external_url(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
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
                self.show_error_modal(
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

    fn show_error_modal(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.push_error_notification(title.to_string(), detail, cx);
    }

    fn push_error_notification(&mut self, title: String, detail: String, cx: &mut Context<Self>) {
        let id = self.next_error_notification_id;
        self.next_error_notification_id = self.next_error_notification_id.saturating_add(1);
        self.error_notifications
            .push(ErrorNotification { id, title, detail });
        cx.notify();
    }

    fn show_error_from(&mut self, error: impl Into<ErrorPresentation>, cx: &mut Context<Self>) {
        let presentation = error.into();
        self.push_error_notification(presentation.title, presentation.detail, cx);
    }

    fn show_info_modal(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.active_modal = Some(AppModal::Info {
            title: title.to_string(),
            detail,
        });
        cx.notify();
    }

    fn show_pro_required_modal(&mut self, feature: ProFeature, cx: &mut Context<Self>) {
        self.active_modal = Some(AppModal::ProRequired { feature });
        cx.notify();
    }

    fn set_auth_state(&mut self, auth_state: auth::AuthState, cx: &mut Context<Self>) {
        self.auth_state = auth_state.clone();
        self.account_control.update(cx, |account_control, cx| {
            account_control.set_state(auth_state, cx);
        });
        cx.notify();
    }

    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let loaded_key_bindings = keymap::load_key_bindings(cx);
        cx.bind_keys(loaded_key_bindings.key_bindings);
        let error_notifications = loaded_key_bindings
            .error
            .map(|error| {
                let presentation = ErrorPresentation::from(error);
                vec![ErrorNotification {
                    id: 0,
                    title: presentation.title,
                    detail: presentation.detail,
                }]
            })
            .unwrap_or_default();
        let next_error_notification_id = error_notifications
            .last()
            .map(|notification| notification.id.saturating_add(1))
            .unwrap_or(0);

        let editor_controller = cx.new(EditorController::new);
        let editor_subscription =
            cx.subscribe(&editor_controller, |this, _editor_controller, event, cx| {
                this.handle_editor_event(event.clone(), cx);
            });
        let workspace = cx.new(Workspace::new);
        let workspace_subscription =
            cx.subscribe(&workspace, |this, _workspace, event, cx| match event {
                WorkspaceEvent::OpenPath(path) => this.open_workspace_path(path.clone(), cx),
            });
        let account_actions = auth::TitleBarAccountActions::new(
            |window, cx| {
                window.dispatch_action(Box::new(RegisterAccount), cx);
            },
            |window, cx| {
                window.dispatch_action(Box::new(RegisterAccount), cx);
            },
            |window, cx| {
                window.dispatch_action(Box::new(SignOut), cx);
            },
        );
        let account_control = cx.new(|_| {
            auth::TitleBarAccountControl::new(auth::AuthState::Restoring, account_actions)
        });
        let title_bar = cx.new(|cx| {
            TitleBar::new(
                menu::APP_NAME,
                menu::title_bar_menus(),
                Some(account_control.clone()),
                cx,
            )
        });
        let bottom_bar = cx.new(BottomBar::new);

        let mut app = Self {
            editor_controller,
            workspace,
            active_document: ActiveDocument::default(),
            ruby_editor: None,
            page_break_menu: None,
            active_modal: None,
            error_notifications,
            next_error_notification_id,
            auth_config: auth::AuthConfig::from_env(),
            auth_state: auth::AuthState::Restoring,
            account_control,
            _workspace_subscription: workspace_subscription,
            _editor_subscription: editor_subscription,
            title_bar,
            bottom_bar,
        };
        app.restore_auth_session(cx);
        app
    }

    fn dismiss_error_notification(&mut self, id: u64, cx: &mut Context<Self>) {
        self.error_notifications
            .retain(|notification| notification.id != id);
        cx.notify();
    }

    fn dismiss_error_notification_action(
        &mut self,
        action: &DismissErrorNotification,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_error_notification(action.id, cx);
    }

    fn window_title(&self, cx: &App) -> String {
        match WorkspaceState::global(cx)
            .active_path()
            .or_else(|| self.active_document.path())
        {
            Some(path) => format!(
                "{}{WINDOW_TITLE_SEPARATOR}{}",
                menu::APP_NAME,
                path.display()
            ),
            _ => menu::APP_NAME.to_string(),
        }
    }

    fn sync_window_title(&self, window: &mut Window, cx: &App) {
        window.set_window_title(&self.window_title(cx));
    }

    fn dismiss_active_modal_action(
        &mut self,
        _: &DismissActiveModal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_modal = None;
        cx.notify();
    }

    fn open_modal_primary_action(
        &mut self,
        _: &OpenModalPrimary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modal = self.active_modal.clone();
        self.active_modal = None;
        cx.notify();

        match modal {
            Some(AppModal::UpdateAvailable {
                release_page_url, ..
            }) => self.open_external_url(release_page_url.as_str(), window, cx),
            Some(AppModal::ProRequired { .. }) => {
                let url = self.auth_config.registration_url();
                self.open_external_url(url.as_str(), window, cx);
            }
            Some(AppModal::Info { .. }) | None => {}
        }
    }

    fn open_workspace_action(
        &mut self,
        _: &OpenWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        MenuActionHandler::open_file(self, window, cx);
    }

    fn register_account_action(
        &mut self,
        _: &RegisterAccount,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url = match &self.auth_state {
            auth::AuthState::Authenticated(_) => self.auth_config.account_url(),
            auth::AuthState::Anonymous | auth::AuthState::Restoring => {
                self.auth_config.registration_url()
            }
        };
        self.open_external_url(url.as_str(), window, cx);
    }

    fn sign_out_action(&mut self, _: &SignOut, _window: &mut Window, cx: &mut Context<Self>) {
        let credentials_key = self.auth_config.credentials_key().to_string();
        self.set_auth_state(auth::AuthState::Anonymous, cx);

        cx.spawn(async move |this, cx| {
            let delete_result = auth::delete_refresh_token(credentials_key, cx).await;

            if let Err(error) = this.update(cx, |this, cx| match delete_result {
                Ok(()) => {
                    this.show_info_modal(
                        "サインアウトしました",
                        "保存済みのログイン情報を削除しました。".to_string(),
                        cx,
                    );
                }
                Err(error) => {
                    this.show_error_modal("サインアウトできませんでした", error, cx);
                }
            }) {
                eprintln!("failed to show sign out result: {error}");
            }
        })
        .detach();
    }

    fn restore_auth_session(&mut self, cx: &mut Context<Self>) {
        let auth_config = self.auth_config.clone();
        let credentials_key = self.auth_config.credentials_key().to_string();

        cx.spawn(async move |this, cx| {
            let stored_refresh_token = auth::read_refresh_token(credentials_key.clone(), cx).await;
            let refresh_token = match stored_refresh_token {
                Ok(Some(refresh_token)) => refresh_token,
                Ok(None) => {
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Anonymous, cx);
                    }) {
                        eprintln!("failed to clear auth state: {error}");
                    }
                    return;
                }
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Anonymous, cx);
                        this.show_error_modal("ログイン情報を読み込めませんでした", error, cx);
                    }) {
                        eprintln!("failed to show auth restore error: {update_error}");
                    }
                    return;
                }
            };

            let restored_session = cx
                .background_spawn(async move {
                    auth::restore_session(&auth_config, refresh_token.as_str())
                })
                .await;

            match restored_session {
                Ok(session) => {
                    if let Err(error) = auth::write_refresh_token(
                        credentials_key.clone(),
                        session.refresh_token.clone(),
                        cx,
                    )
                    .await
                    {
                        eprintln!("failed to save refreshed auth token: {error}");
                    }
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Authenticated(session), cx);
                    }) {
                        eprintln!("failed to restore auth state: {error}");
                    }
                }
                Err(error) => {
                    if let Err(delete_error) =
                        auth::delete_refresh_token(credentials_key.clone(), cx).await
                    {
                        eprintln!("failed to delete invalid auth token: {delete_error}");
                    }
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Anonymous, cx);
                        this.show_error_modal("ログイン情報を更新できませんでした", error, cx);
                    }) {
                        eprintln!("failed to show auth restore failure: {update_error}");
                    }
                }
            }
        })
        .detach();
    }

    pub(crate) fn handle_open_urls(&mut self, urls: Vec<String>, cx: &mut Context<Self>) {
        let credentials_key = self.auth_config.credentials_key().to_string();

        for url in urls {
            let callback =
                match auth::parse_callback(url.as_str(), self.auth_config.callback_scheme()) {
                    Ok(Some(callback)) => callback,
                    Ok(None) => continue,
                    Err(error) => {
                        self.show_error_modal("ログイン情報を処理できませんでした", error, cx);
                        continue;
                    }
                };

            self.apply_auth_callback(callback, credentials_key.clone(), cx);
        }
    }

    fn apply_auth_callback(
        &mut self,
        callback: auth::AuthCallback,
        credentials_key: String,
        cx: &mut Context<Self>,
    ) {
        match callback {
            auth::AuthCallback::SignedOut => {
                self.set_auth_state(auth::AuthState::Anonymous, cx);
                cx.spawn(async move |_, cx| {
                    if let Err(error) = auth::delete_refresh_token(credentials_key, cx).await {
                        eprintln!("failed to delete auth token after sign out callback: {error}");
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

                    match restored_session {
                        Ok(session) => {
                            if let Err(error) = auth::write_refresh_token(
                                credentials_key.clone(),
                                session.refresh_token.clone(),
                                cx,
                            )
                            .await
                                && let Err(update_error) = this.update(cx, |this, cx| {
                                    this.show_error_modal(
                                        "認証情報を保存できませんでした",
                                        error,
                                        cx,
                                    );
                                })
                            {
                                eprintln!("failed to show auth save error: {update_error}");
                            }

                            if let Err(error) = this.update(cx, |this, cx| {
                                this.set_auth_state(auth::AuthState::Authenticated(session), cx);
                            }) {
                                eprintln!("failed to apply auth callback: {error}");
                            }
                        }
                        Err(error) => {
                            if let Err(delete_error) =
                                auth::delete_refresh_token(credentials_key.clone(), cx).await
                            {
                                eprintln!("failed to delete failed auth token: {delete_error}");
                            }
                            if let Err(update_error) = this.update(cx, |this, cx| {
                                this.set_auth_state(auth::AuthState::Anonymous, cx);
                                this.show_error_modal("ログインに失敗しました", error, cx);
                            }) {
                                eprintln!("failed to show auth callback failure: {update_error}");
                            }
                        }
                    }
                })
                .detach();
            }
        }
    }

    fn toggle_workspace_pane_action(
        &mut self,
        _: &ToggleWorkspacePane,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).toggle_pane();
        self.workspace.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    fn vim_command_write_action(
        &mut self,
        _: &VimCommandWrite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        MenuActionHandler::save_file(self, window, cx);
    }

    fn vim_command_quit_action(
        &mut self,
        _: &VimCommandQuit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.quit();
    }

    fn editor_rich_text_bold_action(
        &mut self,
        _: &EditorApplyRichTextBold,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.require_pro_feature(ProFeature::RichText, window, cx) {
            return;
        }

        self.apply_rich_text_kind(RichTextKind::Bold, cx);
    }

    fn editor_rich_text_emphasis_action(
        &mut self,
        _: &EditorApplyRichTextEmphasis,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.require_pro_feature(ProFeature::RichText, window, cx) {
            return;
        }

        self.apply_rich_text_kind(RichTextKind::Emphasis, cx);
    }

    fn editor_rich_text_heading_action(
        &mut self,
        _: &EditorApplyRichTextHeading,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.require_pro_feature(ProFeature::RichText, window, cx) {
            return;
        }

        self.apply_rich_text_kind(RichTextKind::Heading { level: 1 }, cx);
    }

    fn drop_external_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_dropped_paths(paths, window, cx);
    }

    fn current_document_kind(&self, _cx: &App) -> DocumentKind {
        DocumentKind::PlainText
    }

    fn pro_features_available(&self) -> bool {
        match &self.auth_state {
            auth::AuthState::Authenticated(session) => {
                session.user.plan.plan_key.supports_pro_features()
            }
            auth::AuthState::Anonymous | auth::AuthState::Restoring => false,
        }
    }

    fn require_pro_feature(
        &mut self,
        feature: ProFeature,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pro_features_available() {
            return true;
        }

        self.show_pro_required_modal(feature, cx);
        false
    }

    fn selected_byte_range(&self, cx: &App) -> std::ops::Range<usize> {
        self.editor_controller.read(cx).selected_byte_range(cx)
    }

    fn apply_rich_text_kind(&mut self, kind: RichTextKind, cx: &mut Context<Self>) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.apply_rich_text_kind(kind, cx);
        });
        self.save_rich_text_meta(cx);
        cx.notify();
    }

    fn workspace_pane_visible(&self, cx: &App) -> bool {
        WorkspaceState::global(cx).is_pane_visible()
    }

    fn load_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_plain_text(text, cx);
            editor_controller.set_rich_text_meta(rich_text_meta, cx);
        });
        self.active_document.set_path(path);
        self.ruby_editor = None;
        self.page_break_menu = None;
    }

    fn notify_workspace(&self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |_, cx| cx.notify());
    }

    fn open_workspace_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file(path.clone());
        self.load_plain_document(path, text, rich_text_meta, cx);
        self.notify_workspace(cx);
    }

    fn open_standalone_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file_without_root(path.clone());
        self.load_plain_document(path, text, rich_text_meta, cx);
        self.notify_workspace(cx);
    }

    fn open_unsupported_document(
        &mut self,
        path: PathBuf,
        preserve_workspace: bool,
        cx: &mut Context<Self>,
    ) {
        if preserve_workspace {
            WorkspaceState::global_mut(cx).open_unsupported_file(path);
        } else {
            WorkspaceState::global_mut(cx).open_unsupported_file_without_root(path);
        }
        self.notify_workspace(cx);
        cx.notify();
    }

    fn save_document_to_path(
        &mut self,
        path: PathBuf,
        contents: String,
        _window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    write_plain_document_assets(path, contents, rich_text_meta)
                })
                .await;

            match result {
                Ok(path) => {
                    if let Err(error) = this.update(cx, |this, cx| {
                        match classify_saved_document_path(
                            path.as_path(),
                            WorkspaceState::global(cx).root_dir(),
                        ) {
                            SavedDocumentTarget::Workspace => {
                                WorkspaceState::global_mut(cx).open_file(path.clone());
                            }
                            SavedDocumentTarget::Standalone => {
                                WorkspaceState::global_mut(cx).open_file_without_root(path.clone());
                            }
                        }
                        this.active_document.set_path(path);
                        this.notify_workspace(cx);
                        cx.notify();
                    }) {
                        eprintln!("failed to update app after saving document: {error}");
                    }
                }
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_error_from(error, cx);
                    }) {
                        eprintln!("failed to show save error modal: {update_error}");
                    }
                }
            }
        })
        .detach();
    }

    fn open_document_path(
        &mut self,
        path: PathBuf,
        document_kind: DocumentKind,
        preserve_workspace: bool,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move { read_plain_document_assets(path_for_read) })
                .await;

            match result {
                Ok((path, text, rich_text_meta)) => {
                    if let Err(error) = this.update(cx, |this, cx| match document_kind {
                        DocumentKind::PlainText => {
                            if preserve_workspace {
                                this.open_workspace_plain_document(path, &text, rich_text_meta, cx);
                            } else {
                                this.open_standalone_plain_document(
                                    path,
                                    &text,
                                    rich_text_meta,
                                    cx,
                                );
                            }
                        }
                    }) {
                        eprintln!("failed to update app after opening document: {error}");
                    }
                }
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_error_from(error, cx);
                    }) {
                        eprintln!("failed to show open error modal: {update_error}");
                    }
                }
            }
        })
        .detach();
    }

    fn open_directory_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let path_for_scan = path.clone();
            let result = cx
                .background_spawn(async move {
                    scan_workspace_entries(path_for_scan.as_path())
                        .map(|entries| (path_for_scan, entries))
                })
                .await;

            match result {
                Ok((path, entries)) => {
                    if let Err(error) = this.update(cx, |this, cx| {
                        WorkspaceState::global_mut(cx).open_root(path, entries);
                        this.notify_workspace(cx);
                        cx.notify();
                    }) {
                        eprintln!("failed to update app after opening directory: {error}");
                    }
                }
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_error_modal(FILE_OPEN_ERROR_TITLE, error.to_string(), cx);
                    }) {
                        eprintln!("failed to show directory open error modal: {update_error}");
                    }
                }
            }
        })
        .detach();
    }

    fn open_path_target(
        &mut self,
        target: DocumentOpenTarget,
        preserve_workspace: bool,
        cx: &mut Context<Self>,
    ) {
        match target {
            DocumentOpenTarget::Directory(path) => self.open_directory_path(path, cx),
            DocumentOpenTarget::SupportedDocument { path, kind } => {
                self.open_document_path(path, kind, preserve_workspace, cx);
            }
            DocumentOpenTarget::UnsupportedDocument(path) => {
                self.open_unsupported_document(path, preserve_workspace, cx);
            }
        }
    }

    fn open_workspace_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_path_target(classify_open_path(path), true, cx);
    }

    fn open_menu_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_path_target(classify_open_path(path), false, cx);
    }

    fn open_dropped_paths(
        &mut self,
        paths: &ExternalPaths,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = paths.paths().first().cloned() else {
            self.show_error_modal(
                FILE_OPEN_ERROR_TITLE,
                SUPPORTED_OPEN_ERROR_DETAIL.into(),
                cx,
            );
            return;
        };

        self.open_path_target(classify_open_path(path), true, cx);
    }
}

impl Focusable for SoukouApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor_controller.focus_handle(cx)
    }
}

impl MenuActionHandler for SoukouApp {
    fn app_version(&self) -> &'static str {
        APP_VERSION
    }

    fn open_path_from_menu(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_menu_path(path, cx);
    }

    fn export_epub_action(
        &mut self,
        _: &menu::ExportEpub,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.require_pro_feature(ProFeature::ExportEpub, window, cx) {
            return;
        }

        self.export_epub(window, cx);
    }

    fn export_word_action(
        &mut self,
        _: &menu::ExportWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.require_pro_feature(ProFeature::ExportWord, window, cx) {
            return;
        }

        self.export_word(window, cx);
    }

    fn export_base_name(&self, _cx: &App) -> String {
        self.active_document
            .path()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .unwrap_or("untitled")
            .to_string()
    }

    fn export_initial_directory(&self, _cx: &App) -> PathBuf {
        self.active_document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(current_directory_or_fallback)
    }

    fn snapshot_text(&self, cx: &App) -> String {
        match self.current_document_kind(cx) {
            DocumentKind::PlainText => self.editor_controller.read(cx).snapshot_text(cx),
        }
    }

    fn export_epub_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        cx: &mut Context<Self>,
    ) {
        let title = self.export_base_name(cx);
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rich_text::export_epub(
                        &path,
                        title.as_str(),
                        contents.as_str(),
                        &rich_text_meta,
                    )
                })
                .await;

            if let Err(error) = result
                && let Err(update_error) = this.update(cx, |this, cx| {
                    this.show_error_modal("epubを書き出せませんでした", error.to_string(), cx);
                })
            {
                eprintln!("failed to show epub export error: {update_error}");
            }
        })
        .detach();
    }

    fn export_word_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        cx: &mut Context<Self>,
    ) {
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rich_text::export_word(&path, contents.as_str(), &rich_text_meta)
                })
                .await;

            if let Err(error) = result
                && let Err(update_error) = this.update(cx, |this, cx| {
                    this.show_error_modal("Wordを書き出せませんでした", error.to_string(), cx);
                })
            {
                eprintln!("failed to show word export error: {update_error}");
            }
        })
        .detach();
    }

    fn save_blocking_error(&self, cx: &App) -> Option<(&'static str, String)> {
        WorkspaceState::global(cx)
            .unsupported_file()
            .is_some()
            .then(|| {
                (
                    FILE_SAVE_ERROR_TITLE,
                    UNSUPPORTED_DOCUMENT_SAVE_ERROR_DETAIL.to_string(),
                )
            })
    }

    fn active_save_path(&self, _cx: &App) -> Option<PathBuf> {
        self.active_document.path().map(Path::to_path_buf)
    }

    fn suggested_save_directory(&self, cx: &App) -> PathBuf {
        suggested_save_directory(WorkspaceState::global(cx).suggested_save_directory())
    }

    fn suggested_file_name(&self, cx: &App) -> String {
        suggested_file_name(
            WorkspaceState::global(cx).suggested_file_name(),
            self.current_document_kind(cx),
        )
    }

    fn save_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.save_document_to_path(path, contents, window_handle, cx);
    }

    fn show_menu_error(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.show_error_modal(title, detail, cx);
    }

    fn show_menu_info(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.show_info_modal(title, detail, cx);
    }

    fn show_update_available(
        &mut self,
        current_version: String,
        latest_version: String,
        release_page_url: String,
        cx: &mut Context<Self>,
    ) {
        self.active_modal = Some(AppModal::UpdateAvailable {
            current_version,
            latest_version,
            release_page_url,
        });
        cx.notify();
    }
}

impl Render for SoukouApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        title_bar::sync_client_window_inset(window);
        self.sync_window_title(window, cx);
        let title_bar_height = title_bar::platform_title_bar_height(window);
        let bottom_bar_height = bottom_bar::height(window);
        let client_shadow_padding = title_bar::client_side_shadow_padding_size(window);
        let occupied_workspace_width = if self.workspace_pane_visible(cx) {
            WorkspaceState::global(cx).pane_width()
        } else {
            0.0
        };
        let mut editor_viewport_size = window.viewport_size();
        editor_viewport_size.width = (editor_viewport_size.width
            - client_shadow_padding.width
            - px(occupied_workspace_width))
        .max(Pixels::ZERO);
        editor_viewport_size.height = (editor_viewport_size.height
            - client_shadow_padding.height
            - title_bar_height
            - bottom_bar_height
            - px(BOTTOM_BAR_TOP_GAP))
        .max(Pixels::ZERO);
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.update_viewport_size(editor_viewport_size, cx);
        });

        let content = if WorkspaceState::global(cx).unsupported_file().is_some() {
            UnsupportedDocument::from_workspace(cx).into_any_element()
        } else {
            self.editor_controller.clone().into_any_element()
        };

        div()
            .size_full()
            .bg(transparent_black())
            .map(|this| title_bar::apply_client_side_shadow_padding(this, window))
            .child(
                div()
                    .size_full()
                    .bg(Theme::global(cx).white())
                    .font_family(APP_FONT_FAMILY)
                    .flex()
                    .flex_col()
                    .items_center()
                    .overflow_hidden()
                    .map(|this| title_bar::apply_client_side_window_frame(this, window))
                    .can_drop(|value, _, _| value.is::<ExternalPaths>())
                    .on_drop(cx.listener(Self::drop_external_paths))
                    .on_action(cx.listener(Self::open_file_action))
                    .on_action(cx.listener(Self::open_workspace_action))
                    .on_action(cx.listener(Self::register_account_action))
                    .on_action(cx.listener(Self::sign_out_action))
                    .on_action(cx.listener(Self::toggle_workspace_pane_action))
                    .on_action(cx.listener(Self::dismiss_active_modal_action))
                    .on_action(cx.listener(Self::dismiss_error_notification_action))
                    .on_action(cx.listener(Self::open_modal_primary_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::export_txt_action))
                    .on_action(cx.listener(Self::export_word_action))
                    .on_action(cx.listener(Self::export_epub_action))
                    .on_action(cx.listener(Self::editor_rich_text_bold_action))
                    .on_action(cx.listener(Self::editor_rich_text_emphasis_action))
                    .on_action(cx.listener(Self::editor_rich_text_heading_action))
                    .on_action(cx.listener(Self::cancel_ruby_editor_action))
                    .on_action(cx.listener(Self::apply_ruby_editor_action))
                    .on_action(cx.listener(Self::cancel_ruby_edit_action))
                    .on_action(cx.listener(Self::set_page_break_right_of_column_action))
                    .on_action(cx.listener(Self::set_page_break_left_of_column_action))
                    .on_action(cx.listener(Self::remove_page_break_column_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
                    .child(self.title_bar.clone().into_element())
                    .child(
                        div()
                            .flex_1()
                            .h(px(0.0))
                            .w_full()
                            .mb(px(BOTTOM_BAR_TOP_GAP))
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
                    .when_some(self.active_modal.clone(), |this, modal| {
                        this.child(ActiveModal::from_modal(modal))
                    })
                    .when(!self.error_notifications.is_empty(), |this| {
                        this.child(ErrorNotificationStack::new(
                            self.error_notifications.clone(),
                            bottom_bar_height,
                        ))
                    })
                    .when_some(self.rich_text_toolbar_bounds(cx), |this, bounds| {
                        this.child(RichTextToolbar::new(bounds))
                    })
                    .when_some(self.page_break_menu.clone(), |this, page_break_menu| {
                        this.child(PageBreakMenu::new(page_break_menu.request))
                    })
                    .when_some(self.ruby_editor.as_ref(), |this, ruby_editor| {
                        this.child(RubyEditorPopover::new(
                            ruby_editor.request.bounds,
                            ruby_editor.input.clone(),
                        ))
                    })
                    .child(
                        div()
                            .flex_none()
                            .w_full()
                            .h(bottom_bar_height)
                            .child(self.bottom_bar.clone().into_element()),
                    ),
            )
    }
}

impl SoukouApp {
    fn rich_text_toolbar_bounds(&self, cx: &App) -> Option<gpui::Bounds<Pixels>> {
        if !self.pro_features_available()
            || self.selected_byte_range(cx).is_empty()
            || WorkspaceState::global(cx).unsupported_file().is_some()
        {
            return None;
        }

        self.editor_controller.read(cx).selection_bounds(cx)
    }

    fn handle_editor_event(&mut self, event: EditorEvent, cx: &mut Context<Self>) {
        match event {
            EditorEvent::RubyEditRequested(request) => self.open_ruby_editor(request, cx),
            EditorEvent::PageBreakMenuRequested(request) => self.open_page_break_menu(request, cx),
            EditorEvent::PageBreakMoved {
                from_column,
                to_column,
            } => self.move_page_break_column(from_column, to_column, cx),
        }
    }

    fn open_page_break_menu(&mut self, request: PageBreakMenuRequest, cx: &mut Context<Self>) {
        if !self.pro_features_available() || WorkspaceState::global(cx).unsupported_file().is_some()
        {
            return;
        }

        self.ruby_editor = None;
        self.page_break_menu = Some(PageBreakMenuState { request });
        cx.notify();
    }

    fn set_page_break_right_of_column(&mut self, column: usize, cx: &mut Context<Self>) {
        self.set_page_break_column(column.saturating_sub(1), cx);
    }

    fn set_page_break_right_of_column_action(
        &mut self,
        action: &SetPageBreakRightOfColumn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_page_break_right_of_column(action.column, cx);
    }

    fn set_page_break_left_of_column(&mut self, column: usize, cx: &mut Context<Self>) {
        self.set_page_break_column(column, cx);
    }

    fn set_page_break_left_of_column_action(
        &mut self,
        action: &SetPageBreakLeftOfColumn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_page_break_left_of_column(action.column, cx);
    }

    fn set_page_break_column(&mut self, column: usize, cx: &mut Context<Self>) {
        if !self.pro_features_available() {
            self.show_pro_required_modal(ProFeature::RichText, cx);
            return;
        }

        let offset = self.byte_offset_for_column(column, cx);
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_page_break_column(column, offset, cx);
        });
        self.page_break_menu = None;
        self.save_rich_text_meta(cx);
        cx.notify();
    }

    fn move_page_break_column(
        &mut self,
        from_column: usize,
        to_column: usize,
        cx: &mut Context<Self>,
    ) {
        if !self.pro_features_available() {
            self.show_pro_required_modal(ProFeature::RichText, cx);
            return;
        }

        let offset = self.byte_offset_for_column(to_column, cx);
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.move_page_break_column(from_column, to_column, offset, cx);
        });
        self.page_break_menu = None;
        self.save_rich_text_meta(cx);
        cx.notify();
    }

    fn remove_page_break_column(&mut self, column: usize, cx: &mut Context<Self>) {
        if !self.pro_features_available() {
            self.show_pro_required_modal(ProFeature::RichText, cx);
            return;
        }

        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.remove_page_break_column(column, cx);
        });
        self.page_break_menu = None;
        self.save_rich_text_meta(cx);
        cx.notify();
    }

    fn remove_page_break_column_action(
        &mut self,
        action: &RemovePageBreakColumn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_page_break_column(action.column, cx);
    }

    fn byte_offset_for_column(&self, column: usize, cx: &App) -> usize {
        let editor_controller = self.editor_controller.read(cx);
        let rows_per_column = editor_controller.rows_per_column(cx).max(1);
        editor_controller.byte_offset_for_display_cell(column * rows_per_column, cx)
    }

    fn open_ruby_editor(&mut self, request: RubyEditRequest, cx: &mut Context<Self>) {
        if !self.pro_features_available() || WorkspaceState::global(cx).unsupported_file().is_some()
        {
            return;
        }

        self.page_break_menu = None;
        let input = cx.new(TextInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder("ルビ", cx);
            input.set_text(request.text.as_str(), cx);
            input.set_vertical(true, cx);
        });
        self.ruby_editor = Some(RubyEditorState { request, input });
        cx.notify();
    }

    fn apply_ruby_editor(&mut self, cx: &mut Context<Self>) {
        if !self.pro_features_available() {
            self.ruby_editor = None;
            self.show_pro_required_modal(ProFeature::RichText, cx);
            return;
        }

        let Some(ruby_editor) = self.ruby_editor.take() else {
            return;
        };
        let ruby_text = ruby_editor.input.read(cx).text();
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_ruby(ruby_editor.request.range, ruby_text, cx);
        });
        self.save_rich_text_meta(cx);
        cx.notify();
    }

    fn apply_ruby_editor_action(
        &mut self,
        _: &ApplyRubyEdit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_ruby_editor(cx);
    }

    fn cancel_ruby_editor(&mut self, cx: &mut Context<Self>) {
        self.ruby_editor = None;
        cx.notify();
    }

    fn cancel_ruby_edit_action(
        &mut self,
        _: &CancelRubyEdit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_ruby_editor(cx);
    }

    fn cancel_ruby_editor_action(
        &mut self,
        _: &CancelRubyEditor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_ruby_editor(cx);
    }

    fn save_rich_text_meta(&self, cx: &mut Context<Self>) {
        let Some(path) = self.active_document.path().map(Path::to_path_buf) else {
            return;
        };
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.background_spawn(async move {
            if let Err(error) = rich_text::save_meta_for_text_path(&path, &rich_text_meta) {
                eprintln!("failed to save rich text metadata: {error}");
            }
        })
        .detach();
    }
}

#[derive(IntoElement)]
struct ActiveModal {
    icon_path: &'static str,
    title: String,
    subtitle: String,
    detail: String,
    secondary_label: Option<String>,
    primary_label: Option<String>,
}

impl ActiveModal {
    fn from_modal(modal: AppModal) -> Self {
        let (icon_path, title, subtitle, detail, secondary_label, primary_label) = match modal {
            AppModal::Info { title, detail } => (
                icons::MODAL_INFO,
                title,
                String::new(),
                detail,
                None,
                Some("閉じる".to_string()),
            ),
            AppModal::ProRequired { feature } => (
                icons::MODAL_INFO,
                "Proプラン限定機能です".to_string(),
                match feature {
                    ProFeature::RichText => {
                        "リッチテキスト編集は Pro プランで利用できます。".to_string()
                    }
                    ProFeature::ExportWord => {
                        "Wordエクスポートは Pro プランで利用できます。".to_string()
                    }
                    ProFeature::ExportEpub => {
                        "epubエクスポートは Pro プランで利用できます。".to_string()
                    }
                },
                "会員登録ページを開いて、利用できるプランを確認してください。".to_string(),
                Some("あとで".to_string()),
                Some("会員登録".to_string()),
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

        Self {
            icon_path,
            title,
            subtitle,
            detail,
            secondary_label,
            primary_label,
        }
    }
}

impl RenderOnce for ActiveModal {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let accent = mix(Theme::global(cx).primary(), Theme::global(cx).white(), 0.84);

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
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(DismissActiveModal), cx);
            })
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
                                            .external_path(self.icon_path)
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
                                            .child(self.title),
                                    )
                                    .when(!self.subtitle.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_color(Theme::global(cx).text_senodary())
                                                .child(self.subtitle),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(Theme::global(cx).text_senodary())
                                            .child(self.detail),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .when_some(self.secondary_label, |this, label| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(toolbar_border_color(cx))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(gpui::rgb(0xf4f5f6)))
                                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                                            window
                                                .dispatch_action(Box::new(DismissActiveModal), cx);
                                        })
                                        .child(label),
                                )
                            })
                            .when_some(self.primary_label, |this, label| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .bg(Theme::global(cx).primary())
                                        .text_color(Theme::global(cx).white())
                                        .cursor_pointer()
                                        .hover(|style| style.opacity(0.92))
                                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                                            window.dispatch_action(Box::new(OpenModalPrimary), cx);
                                        })
                                        .child(label),
                                )
                            }),
                    ),
            )
    }
}

#[derive(IntoElement)]
struct UnsupportedDocument {
    file_name: String,
}

impl UnsupportedDocument {
    fn from_workspace(cx: &App) -> Self {
        let file_name = WorkspaceState::global(cx)
            .unsupported_file()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("選択したファイル")
            .to_string();

        Self { file_name }
    }
}

impl RenderOnce for UnsupportedDocument {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
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
                    .child(div().font_weight(FontWeight::BOLD).child(self.file_name))
                    .child(div().text_sm().child("このファイルはサポートしていません")),
            )
    }
}

pub(crate) fn toolbar_border_color(cx: &App) -> gpui::Hsla {
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

fn read_document_to_string(path: PathBuf) -> Result<(PathBuf, String), DocumentError> {
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok((path, text)),
        Err(source) => Err(DocumentError::OpenFailed { path, source }),
    }
}

fn read_plain_document_assets(
    path: PathBuf,
) -> Result<(PathBuf, String, RichTextDocumentMeta), DocumentError> {
    let (path, text) = read_document_to_string(path)?;
    let rich_text_meta = rich_text::load_meta_for_text_path(path.as_path()).map_err(|source| {
        DocumentError::MetadataOpenFailed {
            path: path.clone(),
            source,
        }
    })?;
    Ok((path, text, rich_text_meta))
}

fn write_plain_document_assets(
    path: PathBuf,
    contents: String,
    rich_text_meta: RichTextDocumentMeta,
) -> Result<PathBuf, DocumentError> {
    std::fs::write(&path, contents.as_bytes()).map_err(|source| DocumentError::SaveFailed {
        path: path.clone(),
        source,
    })?;
    if !rich_text_meta.is_empty() {
        rich_text::save_meta_for_text_path(path.as_path(), &rich_text_meta).map_err(|source| {
            DocumentError::MetadataSaveFailed {
                path: path.clone(),
                source,
            }
        })?;
    }
    Ok(path)
}
