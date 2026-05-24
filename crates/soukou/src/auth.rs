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
    pub plan: AccountPlan,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlanKey {
    #[default]
    Free,
    Pro,
    Studio,
}

impl PlanKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Pro => "Pro",
            Self::Studio => "Studio",
        }
    }

    pub fn supports_rich_text(self) -> bool {
        matches!(self, Self::Pro | Self::Studio)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountPlan {
    pub plan_key: PlanKey,
    pub subscription: Option<SubscriptionSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriptionSummary {
    pub plan_key: PlanKey,
    pub status: String,
    pub billing_interval: String,
    pub cancel_at_period_end: bool,
}

#[derive(Clone, Debug)]
pub enum AuthCallback {
    SignedIn { refresh_token: String },
    SignedOut,
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

    pub fn callback_url(&self) -> String {
        format!("{}://auth/callback", self.callback_scheme)
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
        let publishable_key = self
            .supabase_publishable_key
            .as_deref()
            .ok_or_else(|| "SOUKOU_SUPABASE_PUBLISHABLE_KEY が設定されていません。".to_string())?;
        Ok((supabase_url, publishable_key))
    }
}

pub fn parse_callback(url: &str, expected_scheme: &str) -> Result<Option<AuthCallback>, String> {
    let parsed =
        Url::parse(url).map_err(|error| format!("認証URLを解析できませんでした: {error}"))?;
    parse_callback_url(&parsed, expected_scheme)
}

fn parse_callback_url(parsed: &Url, expected_scheme: &str) -> Result<Option<AuthCallback>, String> {
    let is_custom_scheme_callback = parsed.scheme() == expected_scheme
        && parsed.host_str() == Some("auth")
        && parsed.path() == "/callback";

    if !is_custom_scheme_callback {
        return Ok(None);
    }

    let query_pairs = parsed.query().unwrap_or_default();
    let query = form_urlencoded::parse(query_pairs.as_bytes())
        .into_owned()
        .collect::<Vec<_>>();
    if query
        .iter()
        .any(|(key, value)| key == "mode" && value == "signed_out")
    {
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
        .get(format!(
            "{}/auth/v1/user",
            supabase_url.trim_end_matches('/')
        ))
        .header("apikey", publishable_key)
        .bearer_auth(token_response.access_token.as_str())
        .send()
        .map_err(|error| format!("ユーザー情報の取得に失敗しました: {error}"))?;

    let user = user
        .error_for_status()
        .map_err(|error| format!("ユーザー情報の取得に失敗しました: {error}"))?
        .json::<SupabaseUser>()
        .map_err(|error| format!("ユーザー情報レスポンスを解析できませんでした: {error}"))?;
    let plan = fetch_account_plan(
        &client,
        supabase_url.trim_end_matches('/'),
        publishable_key,
        token_response.access_token.as_str(),
        user.id.as_str(),
    )
    .unwrap_or_default();

    Ok(AuthSession {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        user: user.into_auth_user(plan),
    })
}

fn fetch_account_plan(
    client: &Client,
    supabase_url: &str,
    publishable_key: &str,
    access_token: &str,
    user_id: &str,
) -> Result<AccountPlan, String> {
    let profile = client
        .get(format!("{supabase_url}/rest/v1/profiles"))
        .header("apikey", publishable_key)
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .query(&[
            ("select", "plan_key"),
            ("id", &format!("eq.{user_id}")),
            ("limit", "1"),
        ])
        .send()
        .map_err(|error| format!("プロフィール情報の取得に失敗しました: {error}"))?;

    let profile = profile
        .error_for_status()
        .map_err(|error| format!("プロフィール情報の取得に失敗しました: {error}"))?
        .json::<Vec<ProfileRow>>()
        .map_err(|error| format!("プロフィール情報レスポンスを解析できませんでした: {error}"))?;

    let subscription = client
        .get(format!("{supabase_url}/rest/v1/subscriptions"))
        .header("apikey", publishable_key)
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .query(&[
            (
                "select",
                "plan_key,status,billing_interval,cancel_at_period_end",
            ),
            ("user_id", &format!("eq.{user_id}")),
            ("status", "in.(trialing,active,past_due,incomplete)"),
            ("order", "created_at.desc"),
            ("limit", "1"),
        ])
        .send()
        .map_err(|error| format!("サブスクリプション情報の取得に失敗しました: {error}"))?;

    let subscription = subscription
        .error_for_status()
        .map_err(|error| format!("サブスクリプション情報の取得に失敗しました: {error}"))?
        .json::<Vec<SubscriptionRow>>()
        .map_err(|error| format!("サブスクリプション情報レスポンスを解析できませんでした: {error}"))?;

    let profile_plan = profile
        .into_iter()
        .next()
        .map(|row| row.plan_key)
        .unwrap_or_default();
    let subscription = subscription.into_iter().next().map(Into::into);
    let plan_key = subscription
        .as_ref()
        .map(|subscription: &SubscriptionSummary| subscription.plan_key)
        .unwrap_or(profile_plan);

    Ok(AccountPlan {
        plan_key,
        subscription,
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
    fn into_auth_user(self, plan: AccountPlan) -> AuthUser {
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
        let avatar_url = self.user_metadata.as_ref().and_then(|metadata| {
            metadata
                .avatar_url
                .clone()
                .or_else(|| metadata.picture.clone())
        });

        AuthUser {
            id: self.id,
            email: self.email,
            display_name,
            avatar_url,
            plan,
        }
    }
}

#[derive(Deserialize)]
struct ProfileRow {
    #[serde(default)]
    plan_key: PlanKey,
}

#[derive(Deserialize)]
struct SubscriptionRow {
    #[serde(default)]
    plan_key: PlanKey,
    status: String,
    billing_interval: String,
    #[serde(default)]
    cancel_at_period_end: bool,
}

impl From<SubscriptionRow> for SubscriptionSummary {
    fn from(value: SubscriptionRow) -> Self {
        Self {
            plan_key: value.plan_key,
            status: value.status,
            billing_interval: value.billing_interval,
            cancel_at_period_end: value.cancel_at_period_end,
        }
    }
}

impl<'de> Deserialize<'de> for PlanKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "pro" => Self::Pro,
            "studio" => Self::Studio,
            _ => Self::Free,
        })
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
