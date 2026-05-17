use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{App, BorrowAppContext, Global};
use url::Url;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AuthState {
    #[default]
    LoggedOut,
    Authorizing,
    LoggedIn(AuthenticatedUser),
    Error(AuthError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthError {
    CallbackError(String),
    CallbackStateMismatch,
    InvalidCallbackUrl,
    InvalidSignInUrl,
    MissingCallbackState,
    MissingSignInUrl,
    MissingAccountUrl,
    MissingUserEmail,
    MissingUserId,
}

#[derive(Default)]
pub struct AuthStore {
    pending_sign_in_state: Option<String>,
    state: AuthState,
}

impl Global for AuthStore {}

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthManager;

pub fn init(cx: &mut App) {
    cx.set_global::<AuthStore>(AuthStore::default());
}

impl AuthStore {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn state(&self) -> &AuthState {
        &self.state
    }

    fn pending_sign_in_state(&self) -> Option<&str> {
        self.pending_sign_in_state.as_deref()
    }

    fn set_pending_sign_in_state(&mut self, pending_sign_in_state: Option<String>) {
        self.pending_sign_in_state = pending_sign_in_state;
    }

    fn set_state(&mut self, state: AuthState) {
        self.state = state;
    }
}

impl AuthManager {
    pub fn new() -> Self {
        Self
    }

    pub fn restore_session(&self, cx: &mut App) {
        cx.update_global::<AuthStore, _>(|store: &mut AuthStore, _| {
            store.set_pending_sign_in_state(None);
            store.set_state(AuthState::LoggedOut)
        });
    }

    pub fn start_browser_sign_in(&self, cx: &mut App) -> Result<(), AuthError> {
        let Some(sign_in_url) = env::auth_sign_in_url() else {
            cx.update_global::<AuthStore, _>(|store: &mut AuthStore, _| {
                store.set_pending_sign_in_state(None);
                store.set_state(AuthState::Error(AuthError::MissingSignInUrl));
            });
            return Err(AuthError::MissingSignInUrl);
        };

        let pending_sign_in_state = new_sign_in_state();
        let callback_url = env::auth_callback_url();
        let full_sign_in_url =
            build_sign_in_url(&sign_in_url, &callback_url, &pending_sign_in_state)?;

        cx.update_global::<AuthStore, _>(|store: &mut AuthStore, _| {
            store.set_pending_sign_in_state(Some(pending_sign_in_state));
            store.set_state(AuthState::Authorizing)
        });
        cx.open_url(&full_sign_in_url);
        Ok(())
    }

    pub fn open_account(&self, cx: &mut App) -> Result<(), AuthError> {
        let Some(account_url) = env::auth_account_url() else {
            return Err(AuthError::MissingAccountUrl);
        };

        cx.open_url(&account_url);
        Ok(())
    }

    pub fn sign_out(&self, cx: &mut App) {
        cx.update_global::<AuthStore, _>(|store: &mut AuthStore, _| {
            store.set_pending_sign_in_state(None);
            store.set_state(AuthState::LoggedOut)
        });
    }

    pub fn complete_callback(&self, callback_url: &str, cx: &mut App) -> Result<(), AuthError> {
        let callback_url = Url::parse(callback_url).map_err(|_| AuthError::InvalidCallbackUrl)?;
        let mut error = None;
        let mut returned_state = None;
        let mut user_id = None;
        let mut email = None;
        let mut display_name = None;
        let mut avatar_url = None;

        for (key, value) in callback_url.query_pairs() {
            match key.as_ref() {
                "error" => error = Some(value.into_owned()),
                "state" => returned_state = Some(value.into_owned()),
                "user_id" => user_id = Some(value.into_owned()),
                "email" => email = Some(value.into_owned()),
                "display_name" => display_name = Some(value.into_owned()),
                "avatar_url" => avatar_url = Some(value.into_owned()),
                _ => {}
            }
        }

        if let Some(error) = error {
            return self.fail_authorization(AuthError::CallbackError(error), cx);
        }

        let returned_state = returned_state.ok_or(AuthError::MissingCallbackState)?;
        let expected_state = AuthStore::global(cx)
            .pending_sign_in_state()
            .ok_or(AuthError::MissingCallbackState)?;

        if returned_state != expected_state {
            return self.fail_authorization(AuthError::CallbackStateMismatch, cx);
        }

        let user_id = user_id.ok_or(AuthError::MissingUserId)?;
        let email = email.ok_or(AuthError::MissingUserEmail)?;

        self.complete_sign_in(
            AuthenticatedUser {
                user_id,
                email,
                display_name,
                avatar_url,
            },
            cx,
        );
        Ok(())
    }

    pub fn complete_sign_in(&self, user: AuthenticatedUser, cx: &mut App) {
        cx.update_global::<AuthStore, _>(|store: &mut AuthStore, _| {
            store.set_pending_sign_in_state(None);
            store.set_state(AuthState::LoggedIn(user))
        });
    }

    pub fn fail_authorization(&self, error: AuthError, cx: &mut App) -> Result<(), AuthError> {
        cx.update_global::<AuthStore, _>(|store: &mut AuthStore, _| {
            store.set_pending_sign_in_state(None);
            store.set_state(AuthState::Error(error.clone()))
        });
        Err(error)
    }

    pub fn has_account_url(&self) -> bool {
        env::auth_account_url().is_some()
    }
}

fn build_sign_in_url(
    sign_in_url: &str,
    callback_url: &str,
    state: &str,
) -> Result<String, AuthError> {
    let mut sign_in_url = Url::parse(sign_in_url).map_err(|_| AuthError::InvalidSignInUrl)?;
    sign_in_url
        .query_pairs_mut()
        .append_pair("callback_url", callback_url)
        .append_pair("state", state);
    Ok(sign_in_url.into())
}

fn new_sign_in_state() -> String {
    let unix_timestamp_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    format!("soukou-{unix_timestamp_millis}-{}", std::process::id())
}
