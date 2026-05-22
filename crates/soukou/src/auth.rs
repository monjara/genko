use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use url::{Url, form_urlencoded};

const DEFAULT_SITE_URL: &str = match option_env!("SOUKOU_SITE_URL") {
    Some(value) => value,
    None => "https://soukou.dev",
};
const DEFAULT_CALLBACK_SCHEME: &str = match option_env!("SOUKOU_AUTH_CALLBACK_SCHEME") {
    Some(value) => value,
    None => "soukou",
};
const CREDENTIALS_KEY: &str = "https://soukou.dev/native/genko";
const COMPILED_SUPABASE_URL: Option<&str> = option_env!("SOUKOU_SUPABASE_URL");
const COMPILED_SUPABASE_PUBLISHABLE_KEY: Option<&str> =
    option_env!("SOUKOU_SUPABASE_PUBLISHABLE_KEY");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthUser {
    pub id: String,
    pub email: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub user: AuthUser,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthState {
    Anonymous,
    Restoring,
    Authenticated(AuthSession),
}

#[derive(Clone, Debug)]
pub enum AuthCallback {
    SignedIn { refresh_token: String },
    SignedOut,
}

pub struct LocalCallbackListener {
    callback_url: String,
    receiver: Receiver<Result<AuthCallback, String>>,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    site_url: String,
    callback_scheme: String,
    supabase_url: Option<String>,
    supabase_publishable_key: Option<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        Self {
            site_url: std::env::var("SOUKOU_SITE_URL")
                .unwrap_or_else(|_| DEFAULT_SITE_URL.to_string()),
            callback_scheme: std::env::var("SOUKOU_AUTH_CALLBACK_SCHEME")
                .unwrap_or_else(|_| DEFAULT_CALLBACK_SCHEME.to_string()),
            supabase_url: std::env::var("SOUKOU_SUPABASE_URL")
                .ok()
                .or_else(|| COMPILED_SUPABASE_URL.map(ToOwned::to_owned)),
            supabase_publishable_key: std::env::var("SOUKOU_SUPABASE_PUBLISHABLE_KEY")
                .ok()
                .or_else(|| COMPILED_SUPABASE_PUBLISHABLE_KEY.map(ToOwned::to_owned)),
        }
    }

    pub fn credentials_key(&self) -> &'static str {
        CREDENTIALS_KEY
    }

    pub fn callback_url(&self) -> String {
        format!("{}://auth/callback", self.callback_scheme)
    }

    pub fn callback_scheme(&self) -> &str {
        self.callback_scheme.as_str()
    }

    pub fn is_supabase_configured(&self) -> bool {
        self.supabase_url.is_some() && self.supabase_publishable_key.is_some()
    }

    pub fn account_url(&self) -> String {
        self.join_site_path("/account")
    }

    pub fn sign_in_url(&self, redirect_url: &str) -> String {
        let mut url = self.join_site_path("/signin");
        url.push_str("?redirect_to=");
        url.push_str(urlencoding::encode(redirect_url).as_ref());
        url
    }

    pub fn sign_out_url(&self, redirect_url: &str) -> String {
        let mut url = self.join_site_path("/signout");
        url.push_str("?redirect_to=");
        url.push_str(urlencoding::encode(redirect_url).as_ref());
        url
    }

    fn join_site_path(&self, path: &str) -> String {
        Url::parse(self.site_url.as_str())
            .and_then(|base| base.join(path))
            .map(|url| url.to_string())
            .unwrap_or_else(|_| format!("{}{}", self.site_url.trim_end_matches('/'), path))
    }

    fn supabase_credentials(&self) -> Result<(&str, &str), String> {
        let supabase_url = self
            .supabase_url
            .as_deref()
            .ok_or_else(|| "SOUKOU_SUPABASE_URL が設定されていません。".to_string())?;
        let publishable_key = self.supabase_publishable_key.as_deref().ok_or_else(|| {
            "SOUKOU_SUPABASE_PUBLISHABLE_KEY が設定されていません。".to_string()
        })?;
        Ok((supabase_url, publishable_key))
    }
}

impl LocalCallbackListener {
    pub fn callback_url(&self) -> &str {
        self.callback_url.as_str()
    }

    pub fn wait_for_callback(self) -> Result<AuthCallback, String> {
        self.receiver
            .recv_timeout(Duration::from_secs(180))
            .map_err(|_| "認証結果の受信がタイムアウトしました。".to_string())?
    }
}

pub fn start_local_callback_server() -> Result<LocalCallbackListener, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("認証用のローカルポートを開けませんでした: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("認証用ポートを取得できませんでした: {error}"))?
        .port();

    let (sender, receiver) = mpsc::channel::<Result<AuthCallback, String>>();
    thread::spawn(move || {
        let result = handle_local_callback(listener);
        let _ = sender.send(result);
    });

    Ok(LocalCallbackListener {
        callback_url: format!("http://127.0.0.1:{port}/auth/callback"),
        receiver,
    })
}

pub fn parse_callback(url: &str, expected_scheme: &str) -> Result<Option<AuthCallback>, String> {
    let parsed =
        Url::parse(url).map_err(|error| format!("認証URLを解析できませんでした: {error}"))?;
    parse_callback_url(&parsed, expected_scheme)
}

fn parse_callback_url(parsed: &Url, expected_scheme: &str) -> Result<Option<AuthCallback>, String> {
    let is_custom_scheme_callback =
        parsed.scheme() == expected_scheme
            && parsed.host_str() == Some("auth")
            && parsed.path() == "/callback";
    let is_localhost_callback =
        (parsed.scheme() == "http" || parsed.scheme() == "https")
            && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"))
            && parsed.path() == "/auth/callback";

    if !is_custom_scheme_callback && !is_localhost_callback {
        return Ok(None);
    }

    let query_pairs = parsed.query().unwrap_or_default();
    let query = form_urlencoded::parse(query_pairs.as_bytes())
        .into_owned()
        .collect::<Vec<_>>();
    if query.iter().any(|(key, value)| key == "mode" && value == "signed_out") {
        return Ok(Some(AuthCallback::SignedOut));
    }

    let refresh_token = query
        .iter()
        .find_map(|(key, value)| (key == "refresh_token").then(|| value.clone()))
        .or_else(|| {
            let fragment = parsed.fragment().unwrap_or_default();
            let fragment_pairs = form_urlencoded::parse(fragment.as_bytes())
                .into_owned()
                .collect::<Vec<_>>();
            fragment_pairs
                .iter()
                .find_map(|(key, value)| (key == "refresh_token").then(|| value.clone()))
        });

    Ok(refresh_token.map(|refresh_token| AuthCallback::SignedIn { refresh_token }))
}

fn handle_local_callback(listener: TcpListener) -> Result<AuthCallback, String> {
    let (mut stream, _) = listener
        .accept()
        .map_err(|error| format!("認証結果の受信に失敗しました: {error}"))?;
    let mut request = [0; 4096];
    let read_len = stream
        .read(&mut request)
        .map_err(|error| format!("認証リクエストを読み取れませんでした: {error}"))?;
    let request = String::from_utf8_lossy(&request[..read_len]);
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| "認証リクエストの形式が不正です。".to_string())?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "認証リクエストのパスを取得できませんでした。".to_string())?;
    let url = Url::parse(&format!("http://127.0.0.1{path}"))
        .map_err(|error| format!("認証コールバックURLを解析できませんでした: {error}"))?;
    let callback = parse_callback_url(&url, DEFAULT_CALLBACK_SCHEME)?
        .ok_or_else(|| "認証コールバックの形式が不正です。".to_string())?;

    let body = r#"<!doctype html><html lang="ja"><head><meta charset="utf-8"><title>草稿</title></head><body><p>草稿へのログインを処理しました。アプリに戻ってください。</p></body></html>"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("認証レスポンスを返せませんでした: {error}"))?;

    Ok(callback)
}

pub fn restore_session(config: &AuthConfig, refresh_token: &str) -> Result<AuthSession, String> {
    let (supabase_url, publishable_key) = config.supabase_credentials()?;
    let client = Client::builder()
        .build()
        .map_err(|error| format!("認証クライアントを初期化できませんでした: {error}"))?;

    let token_response = client
        .post(format!(
            "{}/auth/v1/token?grant_type=refresh_token",
            supabase_url.trim_end_matches('/')
        ))
        .header("apikey", publishable_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "refresh_token": refresh_token,
        }))
        .send()
        .map_err(|error| format!("セッション更新に失敗しました: {error}"))?;

    let token_response = token_response
        .error_for_status()
        .map_err(|error| format!("セッション更新に失敗しました: {error}"))?
        .json::<TokenRefreshResponse>()
        .map_err(|error| format!("セッション更新レスポンスを解析できませんでした: {error}"))?;

    let user = client
        .get(format!("{}/auth/v1/user", supabase_url.trim_end_matches('/')))
        .header("apikey", publishable_key)
        .bearer_auth(token_response.access_token.as_str())
        .send()
        .map_err(|error| format!("ユーザー情報の取得に失敗しました: {error}"))?;

    let user = user
        .error_for_status()
        .map_err(|error| format!("ユーザー情報の取得に失敗しました: {error}"))?
        .json::<SupabaseUser>()
        .map_err(|error| format!("ユーザー情報レスポンスを解析できませんでした: {error}"))?;

    Ok(AuthSession {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        user: user.into_auth_user(),
    })
}

#[derive(Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct SupabaseUser {
    id: String,
    email: Option<String>,
    user_metadata: Option<UserMetadata>,
}

impl SupabaseUser {
    fn into_auth_user(self) -> AuthUser {
        let display_name = self
            .user_metadata
            .as_ref()
            .and_then(|metadata| {
                metadata
                    .full_name
                    .clone()
                    .or_else(|| metadata.name.clone())
                    .or_else(|| metadata.user_name.clone())
            })
            .or_else(|| self.email.clone())
            .unwrap_or_else(|| "Google ユーザー".to_string());
        let avatar_url = self
            .user_metadata
            .as_ref()
            .and_then(|metadata| metadata.avatar_url.clone().or_else(|| metadata.picture.clone()));

        AuthUser {
            id: self.id,
            email: self.email,
            display_name,
            avatar_url,
        }
    }
}

#[derive(Deserialize)]
struct UserMetadata {
    full_name: Option<String>,
    name: Option<String>,
    user_name: Option<String>,
    avatar_url: Option<String>,
    picture: Option<String>,
}
