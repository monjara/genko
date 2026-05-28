use gpui::{AppContext, Context, Window};
use semver::Version;

use crate::app::{
    APP_VERSION, AppModal, AvailableUpdate, GitHubRelease, RELEASES_LATEST_API_URL, SoukouApp,
    UPDATE_CHECK_ERROR_TITLE, UPDATE_NOT_AVAILABLE_TITLE,
};

impl SoukouApp {
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

    pub(super) fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let _ = window;
        cx.spawn(async move |_, cx| {
            let update_result = cx
                .background_spawn(async { Self::fetch_available_update() })
                .await;
            let Some(this_entity) = this.upgrade() else {
                return;
            };
            let _ = this_entity.update(cx, |this, cx| match update_result {
                Ok(Some(available_update)) => {
                    this.active_modal = Some(AppModal::UpdateAvailable {
                        current_version: format!("v{}", available_update.current_version),
                        latest_version: format!("v{}", available_update.latest_version),
                        release_page_url: available_update.release_page_url,
                    });
                    cx.notify();
                }
                Ok(None) => {
                    this.show_info_modal(
                        UPDATE_NOT_AVAILABLE_TITLE,
                        format!("現在のバージョン v{APP_VERSION} は最新版です。"),
                        cx,
                    );
                }
                Err(detail) => {
                    this.show_error_modal(UPDATE_CHECK_ERROR_TITLE, detail, cx);
                }
            });
        })
        .detach();
    }
}
