mod modal;
use modal::{ActiveModal, AppModal, EpubMetaFormOverlay, ProFeature};

use std::path::{Path, PathBuf};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::process::Command;
use std::time::Duration;

use crate::{
    ConfirmEpubMeta, DismissActiveModal, DismissEpubMetaForm, OpenModalPrimary,
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
    EditorController, PreparedPlainText, RichTextToolbar, VimCommandQuit, VimCommandWrite,
};
use gpui::{
    AnyWindowHandle, App, AppContext, Context, Entity, ExternalPaths, FocusHandle, Focusable,
    FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, Render, RenderOnce, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px, transparent_black,
};
use menu::{MenuActionHandler, RegisterAccount, SignOut};
use rich_text::{EpubBookMeta, RichTextDocumentMeta};
use settings::AppSettings;
use theme::{APP_FONT_FAMILY, Theme, ThemeMode};
use title_bar::TitleBar;
use workspace::{
    Event as WorkspaceEvent, OpenWorkspace, ToggleWorkspacePane, Workspace, WorkspaceState,
    scan_workspace_entries,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const WINDOW_TITLE_SEPARATOR: &str = " - ";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const SUPPORTED_OPEN_ERROR_DETAIL: &str = "現在は .txt ファイルに対応しています";
const UNSUPPORTED_DOCUMENT_SAVE_ERROR_DETAIL: &str = "サポートしていないファイルは保存できません";
const BOTTOM_BAR_TOP_GAP: f32 = 4.0;
const AUTH_REVALIDATION_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub(super) struct SoukouApp {
    editor_controller: Entity<EditorController>,
    workspace: Entity<Workspace>,
    active_document: ActiveDocument,
    epub_meta_form: Option<EpubMetaFormState>,
    active_modal: Option<AppModal>,
    error_notifications: Vec<ErrorNotification>,
    next_error_notification_id: u64,
    auth_config: auth::AuthConfig,
    auth_state: auth::AuthState,
    verified_pro_feature: Option<ProFeature>,
    account_control: Entity<auth::TitleBarAccountControl>,
    _workspace_subscription: Subscription,
    appearance_subscription: Option<Subscription>,
    title_bar: Entity<TitleBar>,
    bottom_bar: Entity<BottomBar>,
}

struct EpubMetaFormState {
    title_input: Entity<ui::TextInput>,
    author_input: Entity<ui::TextInput>,
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
        self.verified_pro_feature = None;
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
        let title_bar =
            cx.new(|cx| TitleBar::new(menu::title_bar_menus(), Some(account_control.clone()), cx));
        let bottom_bar = cx.new(BottomBar::new);

        let mut app = Self {
            editor_controller,
            workspace,
            active_document: ActiveDocument::default(),
            epub_meta_form: None,
            active_modal: None,
            error_notifications,
            next_error_notification_id,
            auth_config: auth::AuthConfig::from_env(),
            auth_state: auth::AuthState::Restoring,
            verified_pro_feature: None,
            account_control,
            _workspace_subscription: workspace_subscription,
            appearance_subscription: None,
            title_bar,
            bottom_bar,
        };
        app.restore_auth_session(cx);
        app.start_auth_revalidation_timer(cx);
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

    fn start_auth_revalidation_timer(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(AUTH_REVALIDATION_INTERVAL)
                    .await;

                let session_input = match this.update(cx, |this, _cx| {
                    let auth::AuthState::Authenticated(session) = &this.auth_state else {
                        return None;
                    };
                    Some((
                        this.auth_config.clone(),
                        this.auth_config.credentials_key().to_string(),
                        session.refresh_token.clone(),
                    ))
                }) {
                    Ok(session_input) => session_input,
                    Err(error) => {
                        eprintln!("failed to read auth state for periodic revalidation: {error}");
                        break;
                    }
                };

                let Some((auth_config, credentials_key, refresh_token)) = session_input else {
                    continue;
                };

                let revalidated_session = cx
                    .background_spawn(async move {
                        auth::restore_session(&auth_config, refresh_token.as_str())
                    })
                    .await;

                let session = match revalidated_session {
                    Ok(session) => session,
                    Err(error) => {
                        eprintln!("periodic auth revalidation failed: {error}");
                        continue;
                    }
                };

                if let Err(error) =
                    auth::write_refresh_token(credentials_key, session.refresh_token.clone(), cx)
                        .await
                {
                    eprintln!("failed to save periodically refreshed auth token: {error}");
                }

                if let Err(error) = this.update(cx, |this, cx| {
                    this.set_auth_state(auth::AuthState::Authenticated(session), cx);
                }) {
                    eprintln!("failed to apply periodic auth revalidation: {error}");
                    break;
                }
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
        if env::development_mode() {
            return true;
        }
        match &self.auth_state {
            auth::AuthState::Authenticated(session) => {
                session.user.plan.plan_key.supports_pro_features()
            }
            auth::AuthState::Anonymous | auth::AuthState::Restoring => false,
        }
    }

    fn consume_verified_pro_feature(&mut self, feature: ProFeature) -> bool {
        if self.verified_pro_feature == Some(feature) {
            self.verified_pro_feature = None;
            return true;
        }
        false
    }

    fn verify_pro_feature_for_action(
        &mut self,
        feature: ProFeature,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if env::development_mode() {
            self.verified_pro_feature = Some(feature);
            self.dispatch_pro_feature_action(feature, window_handle, cx);
            return;
        }

        let auth::AuthState::Authenticated(session) = &self.auth_state else {
            self.show_pro_required_modal(feature, cx);
            return;
        };

        let auth_config = self.auth_config.clone();
        let credentials_key = self.auth_config.credentials_key().to_string();
        let refresh_token = session.refresh_token.clone();

        cx.spawn(async move |this, cx| {
            let verified_session = cx
                .background_spawn(async move {
                    auth::restore_session(&auth_config, refresh_token.as_str())
                })
                .await;

            match verified_session {
                Ok(session) => {
                    if let Err(error) = auth::write_refresh_token(
                        credentials_key,
                        session.refresh_token.clone(),
                        cx,
                    )
                    .await
                    {
                        eprintln!("failed to save verified auth token: {error}");
                    }

                    let supports_pro_features = session.user.plan.plan_key.supports_pro_features();
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.set_auth_state(auth::AuthState::Authenticated(session), cx);
                        if supports_pro_features {
                            this.verified_pro_feature = Some(feature);
                        } else {
                            this.show_pro_required_modal(feature, cx);
                        }
                    }) {
                        eprintln!("failed to apply pro feature verification: {error}");
                        return;
                    }

                    if supports_pro_features
                        && let Err(error) =
                            window_handle.update(cx, |_, window, cx| match feature {
                                ProFeature::ExportWord => {
                                    window.dispatch_action(Box::new(menu::ExportWord), cx);
                                }
                                ProFeature::ExportEpub => {
                                    window.dispatch_action(Box::new(menu::ExportEpub), cx);
                                }
                            })
                    {
                        eprintln!("failed to continue pro feature action: {error}");
                    }
                }
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_error_modal("会員情報を確認できませんでした", error, cx);
                    }) {
                        eprintln!("failed to show pro feature verification error: {update_error}");
                    }
                }
            }
        })
        .detach();
    }

    fn dispatch_pro_feature_action(
        &self,
        feature: ProFeature,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = window_handle.update(cx, |_, window, cx| match feature {
            ProFeature::ExportWord => {
                window.dispatch_action(Box::new(menu::ExportWord), cx);
            }
            ProFeature::ExportEpub => {
                window.dispatch_action(Box::new(menu::ExportEpub), cx);
            }
        }) {
            eprintln!("failed to dispatch verified pro feature action: {error}");
        }
    }

    fn selected_byte_range(&self, cx: &App) -> std::ops::Range<usize> {
        self.editor_controller.read(cx).selected_byte_range(cx)
    }

    fn workspace_pane_visible(&self, cx: &App) -> bool {
        WorkspaceState::global(cx).is_pane_visible()
    }

    fn key_context(&self) -> String {
        let mut context = String::from("SoukouApp");
        if self.active_modal.is_some() {
            context.push_str(" active_modal");
        }
        if self.epub_meta_form.is_some() {
            context.push_str(" epub_meta_form");
        }
        context
    }

    fn load_plain_document(
        &mut self,
        path: PathBuf,
        prepared_text: PreparedPlainText,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_prepared_plain_text(prepared_text, cx);
            editor_controller.set_rich_text_meta(rich_text_meta, cx);
        });
        self.active_document.set_path(path);
    }

    fn notify_workspace(&self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |_, cx| cx.notify());
    }

    fn open_workspace_plain_document(
        &mut self,
        path: PathBuf,
        prepared_text: PreparedPlainText,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file(path.clone());
        self.load_plain_document(path, prepared_text, rich_text_meta, cx);
        self.notify_workspace(cx);
    }

    fn open_standalone_plain_document(
        &mut self,
        path: PathBuf,
        prepared_text: PreparedPlainText,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file_without_root(path.clone());
        self.load_plain_document(path, prepared_text, rich_text_meta, cx);
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

    fn save_document_to_path(&mut self, path: PathBuf, contents: String, cx: &mut Context<Self>) {
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        let save_rich_text_meta = self.pro_features_available();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    write_plain_document_assets(path, contents, rich_text_meta, save_rich_text_meta)
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
                                WorkspaceState::global_mut(cx).open_saved_file(path.clone());
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
        let plain_text_load_settings = self.editor_controller.read(cx).plain_text_load_settings(cx);
        cx.spawn(async move |this, cx| {
            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move {
                    read_plain_document_assets(path_for_read).map(|(path, text, rich_text_meta)| {
                        (
                            path,
                            PreparedPlainText::new(text, plain_text_load_settings),
                            rich_text_meta,
                        )
                    })
                })
                .await;

            match result {
                Ok((path, prepared_text, rich_text_meta)) => {
                    if let Err(error) = this.update(cx, |this, cx| match document_kind {
                        DocumentKind::PlainText => {
                            if preserve_workspace {
                                this.open_workspace_plain_document(
                                    path,
                                    prepared_text,
                                    rich_text_meta,
                                    cx,
                                );
                            } else {
                                this.open_standalone_plain_document(
                                    path,
                                    prepared_text,
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
        if !self.consume_verified_pro_feature(ProFeature::ExportEpub) {
            self.verify_pro_feature_for_action(ProFeature::ExportEpub, window.window_handle(), cx);
            return;
        }

        self.show_epub_meta_form(window, cx);
    }

    fn export_word_action(
        &mut self,
        _: &menu::ExportWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.consume_verified_pro_feature(ProFeature::ExportWord) {
            self.verify_pro_feature_for_action(ProFeature::ExportWord, window.window_handle(), cx);
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
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rich_text::export_epub(&path, contents.as_str(), &rich_text_meta)
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

    fn save_path_from_menu(&mut self, path: PathBuf, contents: String, cx: &mut Context<Self>) {
        self.save_document_to_path(path, contents, cx);
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
        if self.appearance_subscription.is_none() {
            self.appearance_subscription =
                Some(cx.observe_window_appearance(window, |_, window, cx| {
                    if AppSettings::global(cx).theme_mode == ThemeMode::System {
                        theme::apply_mode_for_window_appearance(
                            ThemeMode::System,
                            window.appearance(),
                            cx,
                        );
                        cx.refresh_windows();
                    }
                }));
        }
        self.sync_window_title(window, cx);
        let title_bar_height = title_bar::platform_title_bar_height(window);
        let bottom_bar_height = bottom_bar::height(window);
        let key_context = self.key_context();
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
                    .key_context(key_context.as_str())
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
                    .on_action(cx.listener(Self::confirm_epub_meta_action))
                    .on_action(cx.listener(Self::dismiss_epub_meta_form_action))
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
                    .when_some(self.epub_meta_form.as_ref(), |this, form| {
                        this.child(EpubMetaFormOverlay::new(
                            form.title_input.clone(),
                            form.author_input.clone(),
                        ))
                    })
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
        if self.selected_byte_range(cx).is_empty()
            || WorkspaceState::global(cx).unsupported_file().is_some()
        {
            return None;
        }

        self.editor_controller.read(cx).selection_bounds(cx)
    }

    fn show_epub_meta_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let existing_epub_meta = self
            .active_document
            .path()
            .and_then(|path| {
                rich_text::load_meta_for_text_path(path)
                    .ok()
                    .map(|meta| meta.epub)
            })
            .unwrap_or_default();

        let default_title = if existing_epub_meta.title.is_empty() {
            self.export_base_name(cx)
        } else {
            existing_epub_meta.title.clone()
        };

        let title_input = cx.new(ui::TextInput::new);
        title_input.update(cx, |input, cx| {
            input.set_placeholder("タイトル", cx);
            input.set_text(default_title.as_str(), cx);
        });

        let author_input = cx.new(ui::TextInput::new);
        author_input.update(cx, |input, cx| {
            input.set_placeholder("著者名", cx);
            input.set_text(existing_epub_meta.author.as_str(), cx);
        });

        let title_focus = title_input.focus_handle(cx);
        window.focus(&title_focus, cx);

        self.epub_meta_form = Some(EpubMetaFormState {
            title_input,
            author_input,
        });
        cx.notify();
    }

    fn confirm_epub_meta(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.epub_meta_form.take() else {
            return;
        };
        let title = form.title_input.read(cx).text();
        let author = form.author_input.read(cx).text();

        let mut rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        rich_text_meta.epub = EpubBookMeta { title, author };
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_rich_text_meta(rich_text_meta, cx);
        });

        cx.notify();

        MenuActionHandler::export_epub(self, window, cx);
    }

    fn confirm_epub_meta_action(
        &mut self,
        _: &ConfirmEpubMeta,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_epub_meta(window, cx);
    }

    fn dismiss_epub_meta_form(&mut self, cx: &mut Context<Self>) {
        self.epub_meta_form = None;
        cx.notify();
    }

    fn dismiss_epub_meta_form_action(
        &mut self,
        _: &DismissEpubMetaForm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_epub_meta_form(cx);
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

pub(super) fn mix(left: gpui::Rgba, right: gpui::Rgba, ratio: f32) -> gpui::Rgba {
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
    save_rich_text_meta: bool,
) -> Result<PathBuf, DocumentError> {
    std::fs::write(&path, contents.as_bytes()).map_err(|source| DocumentError::SaveFailed {
        path: path.clone(),
        source,
    })?;
    if save_rich_text_meta {
        rich_text::save_meta_for_text_path(path.as_path(), &rich_text_meta).map_err(|source| {
            DocumentError::MetadataSaveFailed {
                path: path.clone(),
                source,
            }
        })?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use rich_text::{RichTextDocumentMeta, RichTextKind};

    use super::write_plain_document_assets;

    fn unique_temp_dir(test_name: &str) -> Result<std::path::PathBuf, Box<dyn Error>> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("soukou-{test_name}-{timestamp}"));
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn cleanup_temp_dir(path: &std::path::Path) -> Result<(), Box<dyn Error>> {
        match fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Box::new(error)),
        }
    }

    #[test]
    fn saving_plain_document_writes_rich_text_meta_when_enabled() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("writes-meta")?;
        let text_path = dir.join("draft.txt");
        let meta_path = rich_text::meta_path_for_text_path(&text_path);
        let mut rich_text_meta = RichTextDocumentMeta::default();
        rich_text_meta.add_mark(0..2, RichTextKind::Bold);

        write_plain_document_assets(text_path.clone(), "本文".to_string(), rich_text_meta, true)?;

        assert_eq!(fs::read_to_string(&text_path)?, "本文");
        assert!(meta_path.exists());

        cleanup_temp_dir(&dir)?;
        Ok(())
    }

    #[test]
    fn saving_plain_document_does_not_write_meta_when_disabled() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("skips-meta")?;
        let text_path = dir.join("draft.txt");
        let meta_path = rich_text::meta_path_for_text_path(&text_path);

        write_plain_document_assets(
            text_path.clone(),
            "本文".to_string(),
            RichTextDocumentMeta::default(),
            false,
        )?;

        assert_eq!(fs::read_to_string(&text_path)?, "本文");
        assert!(!meta_path.exists());

        cleanup_temp_dir(&dir)?;
        Ok(())
    }

    #[test]
    fn saving_plain_document_writes_empty_meta_when_enabled() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("writes-empty-meta")?;
        let text_path = dir.join("draft.txt");
        let meta_path = rich_text::meta_path_for_text_path(&text_path);

        write_plain_document_assets(
            text_path.clone(),
            "本文".to_string(),
            RichTextDocumentMeta::default(),
            true,
        )?;

        assert_eq!(fs::read_to_string(&text_path)?, "本文");
        assert!(meta_path.exists());
        assert_eq!(
            rich_text::load_meta_for_text_path(&text_path)?,
            RichTextDocumentMeta::default()
        );

        cleanup_temp_dir(&dir)?;
        Ok(())
    }
}
