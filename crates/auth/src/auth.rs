use std::fmt;
use std::rc::Rc;

use gpui::AsyncApp;
use gpui::{
    Anchor, AnyElement, App, BoxShadow, ClickEvent, Context, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, Render, Rgba, StatefulInteractiveElement, Styled, Window,
    anchored, deferred, div, point, prelude::FluentBuilder, px,
};
use reqwest::blocking::Client;
use serde::Deserialize;
use theme::Theme;
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
const ACCOUNT_MENU_MIN_WIDTH: Pixels = px(220.0);
const ACCOUNT_MENU_VERTICAL_OFFSET: Pixels = px(12.0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthUser {
    pub id: String,
    pub email: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub plan: AccountPlan,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub user: AuthUser,
}

impl fmt::Debug for AuthSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthSession")
            .field("user", &self.user)
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
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
    pub fn supports_pro_features(self) -> bool {
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
            supabase_url: runtime_or_compiled_env("SOUKOU_SUPABASE_URL", COMPILED_SUPABASE_URL),
            supabase_publishable_key: runtime_or_compiled_env(
                "SOUKOU_SUPABASE_PUBLISHABLE_KEY",
                COMPILED_SUPABASE_PUBLISHABLE_KEY,
            ),
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

    pub fn registration_url(&self) -> String {
        let callback_url = self.callback_url();
        let mut url = self.join_site_path("/signin");
        let separator = if url.contains('?') { '&' } else { '?' };
        url.push(separator);
        url.push_str("redirect_to=");
        url.extend(form_urlencoded::byte_serialize(callback_url.as_bytes()));
        url
    }

    pub fn account_url(&self) -> String {
        self.join_site_path("/account")
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

#[derive(Clone)]
pub struct TitleBarAccountActions {
    sign_in: Rc<dyn Fn(&mut Window, &mut App)>,
    open_account_settings: Rc<dyn Fn(&mut Window, &mut App)>,
    sign_out: Rc<dyn Fn(&mut Window, &mut App)>,
}

impl TitleBarAccountActions {
    pub fn new(
        sign_in: impl Fn(&mut Window, &mut App) + 'static,
        open_account_settings: impl Fn(&mut Window, &mut App) + 'static,
        sign_out: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            sign_in: Rc::new(sign_in),
            open_account_settings: Rc::new(open_account_settings),
            sign_out: Rc::new(sign_out),
        }
    }
}

pub struct TitleBarAccountControl {
    state: AuthState,
    actions: TitleBarAccountActions,
    menu_open: bool,
    menu_position: Point<Pixels>,
}

impl TitleBarAccountControl {
    pub fn new(state: AuthState, actions: TitleBarAccountActions) -> Self {
        Self {
            state,
            actions,
            menu_open: false,
            menu_position: point(px(0.0), px(0.0)),
        }
    }

    pub fn set_state(&mut self, state: AuthState, cx: &mut Context<Self>) {
        self.state = state;
        if !matches!(self.state, AuthState::Authenticated(_)) {
            self.menu_open = false;
        }
        cx.notify();
    }

    fn toggle_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.menu_open {
            self.menu_open = false;
            cx.notify();
            return;
        }

        self.menu_open = true;
        self.menu_position = point(
            position.x - px(180.0),
            position.y + ACCOUNT_MENU_VERTICAL_OFFSET,
        );
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu_open {
            self.menu_open = false;
            cx.notify();
        }
    }

    fn render_trigger(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.state {
            AuthState::Anonymous => self.render_sign_in_button(cx).into_any_element(),
            AuthState::Restoring => div()
                .id("title-bar-account-restoring")
                .h(px(26.0))
                .px_3()
                .rounded_sm()
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(text_color(cx))
                .child("確認中")
                .into_any_element(),
            AuthState::Authenticated(session) => self.render_user_badge(&session.user, cx),
        }
    }

    fn render_sign_in_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let actions = self.actions.clone();

        div()
            .id("title-bar-sign-in")
            .px_3()
            .h(px(26.0))
            .rounded_sm()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .text_color(text_color(cx))
            .border_1()
            .border_color(border_color(cx))
            .bg(Theme::global(cx).white())
            .cursor_pointer()
            .hover(|style| {
                style.bg(mix(
                    Theme::global(cx).bg_senodary(),
                    Theme::global(cx).white(),
                    0.14,
                ))
            })
            .active(|style| style.opacity(0.85))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(move |_, window, cx| {
                (actions.sign_in)(window, cx);
            })
            .child("会員登録")
    }

    fn render_user_badge(&self, user: &AuthUser, cx: &mut Context<Self>) -> AnyElement {
        let display_name = user.display_name.clone();
        let initial = account_initial(display_name.as_str());

        div()
            .id("title-bar-account-trigger")
            .w(px(30.0))
            .h(px(30.0))
            .rounded_full()
            .border_1()
            .border_color(border_color(cx))
            .bg(mix(
                Theme::global(cx).primary(),
                Theme::global(cx).white(),
                0.78,
            ))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .text_color(text_color(cx))
            .cursor_pointer()
            .hover(|style| {
                style.bg(mix(
                    Theme::global(cx).primary(),
                    Theme::global(cx).white(),
                    0.68,
                ))
            })
            .active(|style| style.opacity(0.9))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(event.position(), cx);
            }))
            .child(initial)
            .into_any_element()
    }

    fn render_menu_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let AuthState::Authenticated(session) = &self.state else {
            return None;
        };
        if !self.menu_open {
            return None;
        }

        let user = session.user.clone();
        let actions = self.actions.clone();
        let display_name = user.display_name.clone();
        let email = user.email.clone();
        let plan_label = plan_label(user.plan.plan_key);
        let initial = account_initial(display_name.as_str());
        let item_hover_background = mix(
            Theme::global(cx).bg_senodary(),
            Theme::global(cx).white(),
            0.12,
        );

        Some(
            deferred(
                anchored()
                    .position(self.menu_position)
                    .anchor(Anchor::TopLeft)
                    .child(
                        div()
                            .id("title-bar-account-popup")
                            .min_w(ACCOUNT_MENU_MIN_WIDTH)
                            .py_2()
                            .bg(Theme::global(cx).white())
                            .border_1()
                            .border_color(border_color(cx))
                            .rounded_md()
                            .shadow(vec![BoxShadow {
                                color: Hsla {
                                    h: 0.0,
                                    s: 0.0,
                                    l: 0.0,
                                    a: 0.18,
                                },
                                offset: point(px(0.0), px(8.0)),
                                blur_radius: px(24.0),
                                spread_radius: px(0.0),
                            }])
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.close_menu(cx);
                            }))
                            .child(
                                div()
                                    .px_3()
                                    .pb_2()
                                    .flex()
                                    .flex_row()
                                    .gap_3()
                                    .items_center()
                                    .child(
                                        div()
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(border_color(cx))
                                            .bg(mix(
                                                Theme::global(cx).primary(),
                                                Theme::global(cx).white(),
                                                0.78,
                                            ))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(13.0))
                                            .text_color(text_color(cx))
                                            .child(initial),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(text_color(cx))
                                                    .child(display_name),
                                            )
                                            .when_some(email, |this, email| {
                                                this.child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(secondary_text_color(cx))
                                                        .child(email),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(secondary_text_color(cx))
                                                    .child(plan_label),
                                            ),
                                    ),
                            )
                            .child(div().h(px(1.0)).mx_2().bg(border_color(cx)))
                            .child(
                                div()
                                    .mt_1()
                                    .child(
                                        div()
                                            .id("title-bar-account-settings")
                                            .w_full()
                                            .px_3()
                                            .py_1p5()
                                            .rounded_sm()
                                            .text_size(px(12.0))
                                            .text_color(text_color(cx))
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(item_hover_background))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.close_menu(cx);
                                                (actions.open_account_settings)(window, cx);
                                            }))
                                            .child("アカウント設定"),
                                    )
                                    .child(
                                        div()
                                            .id("title-bar-sign-out")
                                            .w_full()
                                            .px_3()
                                            .py_1p5()
                                            .rounded_sm()
                                            .text_size(px(12.0))
                                            .text_color(text_color(cx))
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(item_hover_background))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.close_menu(cx);
                                                (actions.sign_out)(window, cx);
                                            }))
                                            .child("サインアウト"),
                                    ),
                            ),
                    ),
            )
            .into_any_element(),
        )
    }
}

impl Render for TitleBarAccountControl {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .child(self.render_trigger(cx))
            .children(self.render_menu_popup(cx))
    }
}

fn account_initial(display_name: &str) -> String {
    display_name
        .chars()
        .next()
        .map(|character| character.to_uppercase().collect())
        .unwrap_or_else(|| "U".to_string())
}

fn plan_label(plan_key: PlanKey) -> &'static str {
    match plan_key {
        PlanKey::Free => "Free",
        PlanKey::Pro => "Pro",
        PlanKey::Studio => "Studio",
    }
}

fn border_color(cx: &App) -> Hsla {
    mix(Theme::global(cx).black(), Theme::global(cx).white(), 0.75).into()
}

fn text_color(cx: &App) -> Hsla {
    Theme::global(cx).text_primary().into()
}

fn secondary_text_color(cx: &App) -> Hsla {
    Theme::global(cx).text_senodary().into()
}

fn mix(left: Rgba, right: Rgba, ratio: f32) -> Rgba {
    let ratio = ratio.clamp(0.0, 1.0);
    let inv = 1.0 - ratio;

    Rgba {
        r: left.r * inv + right.r * ratio,
        g: left.g * inv + right.g * ratio,
        b: left.b * inv + right.b * ratio,
        a: left.a * inv + right.a * ratio,
    }
}

fn runtime_or_compiled_env(key: &str, compiled_value: Option<&str>) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            compiled_value
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
        })
}

pub async fn read_refresh_token(
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

pub async fn write_refresh_token(
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

pub async fn delete_refresh_token(
    credentials_key: String,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let task = cx.update(|app| app.delete_credentials(credentials_key.as_str()));
    task.await
        .map_err(|error| format!("認証情報の削除に失敗しました: {error}"))?;
    Ok(())
}

pub fn parse_callback(url: &str, expected_scheme: &str) -> Result<Option<AuthCallback>, String> {
    let parsed =
        Url::parse(url).map_err(|error| format!("認証URLを解析できませんでした: {error}"))?;
    if parsed.scheme() != expected_scheme
        || parsed.host_str() != Some("auth")
        || parsed.path() != "/callback"
    {
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
            form_urlencoded::parse(fragment.as_bytes())
                .into_owned()
                .find_map(|(key, value)| (key == "refresh_token").then_some(value))
        });

    Ok(refresh_token.map(|refresh_token| AuthCallback::SignedIn { refresh_token }))
}

pub fn restore_session(config: &AuthConfig, refresh_token: &str) -> Result<AuthSession, String> {
    let (supabase_url, publishable_key) = config.supabase_credentials()?;
    let supabase_url = supabase_url.trim_end_matches('/');
    let client = Client::builder()
        .build()
        .map_err(|error| format!("認証クライアントを初期化できませんでした: {error}"))?;

    let token_response = client
        .post(format!(
            "{supabase_url}/auth/v1/token?grant_type=refresh_token"
        ))
        .header("apikey", publishable_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "refresh_token": refresh_token,
        }))
        .send()
        .map_err(|error| format!("セッション更新に失敗しました: {error}"))?
        .error_for_status()
        .map_err(|error| format!("セッション更新に失敗しました: {error}"))?
        .json::<TokenRefreshResponse>()
        .map_err(|error| format!("セッション更新レスポンスを解析できませんでした: {error}"))?;

    let user = client
        .get(format!("{supabase_url}/auth/v1/user"))
        .header("apikey", publishable_key)
        .bearer_auth(token_response.access_token.as_str())
        .send()
        .map_err(|error| format!("ユーザー情報の取得に失敗しました: {error}"))?
        .error_for_status()
        .map_err(|error| format!("ユーザー情報の取得に失敗しました: {error}"))?
        .json::<SupabaseUser>()
        .map_err(|error| format!("ユーザー情報レスポンスを解析できませんでした: {error}"))?;
    let plan = fetch_account_plan(
        &client,
        supabase_url,
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
        .map_err(|error| format!("プロフィール情報の取得に失敗しました: {error}"))?
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
        .map_err(|error| format!("サブスクリプション情報の取得に失敗しました: {error}"))?
        .error_for_status()
        .map_err(|error| format!("サブスクリプション情報の取得に失敗しました: {error}"))?
        .json::<Vec<SubscriptionRow>>()
        .map_err(|error| {
            format!("サブスクリプション情報レスポンスを解析できませんでした: {error}")
        })?;

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
