use gpui::{AppContext, AsyncApp, Context, Window};

use crate::{
    OpenAccountSettings, SignIn, SignOut,
    app::SoukouApp,
    auth,
};

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
    pub(super) fn restore_auth_session(&mut self, cx: &mut Context<Self>) {
        let auth_config = self.auth_config.clone();
        let credentials_key = self.auth_config.credentials_key().to_string();

        cx.spawn(async move |this, cx| {
            let Some(this_entity) = this.upgrade() else {
                return;
            };

            let stored_refresh_token = read_stored_refresh_token(credentials_key.clone(), cx).await;

            let refresh_token = match stored_refresh_token {
                Ok(Some(refresh_token)) => refresh_token,
                Ok(None) | Err(_) => {
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

    pub(crate) fn handle_open_urls(&mut self, urls: Vec<String>, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
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

            self.apply_auth_callback(callback, credentials_key.clone(), window_handle, cx);
        }
    }

    fn apply_auth_callback(
        &mut self,
        callback: auth::AuthCallback,
        credentials_key: String,
        _window_handle: Option<gpui::AnyWindowHandle>,
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
                                let _ = this_entity.update(cx, |this, cx| {
                                    this.show_error_modal(
                                        "認証情報を保存できませんでした",
                                        error,
                                        cx,
                                    );
                                });
                            }

                            let _ = this_entity.update(cx, |this, cx| {
                                this.set_auth_state(auth::AuthState::Authenticated(session), cx);
                            });
                        }
                        Err(error) => {
                            let _ = delete_refresh_token(credentials_key.clone(), cx).await;
                            let _ = this_entity.update(cx, |this, cx| {
                                this.set_auth_state(auth::AuthState::Anonymous, cx);
                                this.show_error_modal("ログインに失敗しました", error, cx);
                            });
                        }
                    }
                })
                .detach();
            }
        }
    }

    fn sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.auth_config.is_supabase_configured() {
            let _ = window;
            self.show_error_modal(
                "ログインを開始できませんでした",
                "SOUKOU_SUPABASE_URL と SOUKOU_SUPABASE_PUBLISHABLE_KEY を設定してください。"
                    .to_string(),
                cx,
            );
            return;
        }

        let url = self
            .auth_config
            .sign_in_url(self.auth_config.callback_url().as_str());

        self.open_external_url(url.as_str(), window, cx);
    }

    pub(super) fn open_account_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.auth_config.account_url();
        self.open_external_url(url.as_str(), window, cx);
    }

    fn sign_out(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_auth_state(auth::AuthState::Anonymous, cx);
        let credentials_key = self.auth_config.credentials_key().to_string();

        cx.spawn(move |_, cx: &mut AsyncApp| {
            let mut app = cx.clone();
            async move {
                let _ = delete_refresh_token(credentials_key, &mut app).await;
            }
        })
        .detach();
    }

    pub(super) fn sign_in_action(&mut self, _: &SignIn, window: &mut Window, cx: &mut Context<Self>) {
        self.sign_in(window, cx);
    }

    pub(super) fn open_account_settings_action(
        &mut self,
        _: &OpenAccountSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_account_settings(window, cx);
    }

    pub(super) fn sign_out_action(&mut self, _: &SignOut, window: &mut Window, cx: &mut Context<Self>) {
        self.sign_out(window, cx);
    }
}
