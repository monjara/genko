use crate::document::ActiveDocument;
use bottom_bar::BottomBar;
use editor::EditorController;
use gpui::{App, Entity};
use semver::Version;
use serde::Deserialize;
use theme::Theme;
use title_bar::TitleBar;

mod document_io;
mod export_flow;
mod render;
mod state;
mod updates;

pub(crate) const APP_NAME: &str = "草稿";
pub(crate) const APP_ID: &str = "dev.monj.soukou";
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const MAIN_WINDOW_WIDTH: f32 = 1200.0;
pub(crate) const MAIN_WINDOW_HEIGHT: f32 = 800.0;

const OPEN_PROMPT_LABEL: &str = "開く";
const SETTINGS_MENU_LABEL: &str = "設定";
const CHECK_FOR_UPDATES_MENU_LABEL: &str = "更新を確認";
const QUIT_MENU_LABEL: &str = "終了";
const FILE_MENU_LABEL: &str = "ファイル";
const SAVE_MENU_LABEL: &str = "保存";
const EXPORT_TXT_MENU_LABEL: &str = "txtエクスポート";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const FILE_PICKER_ERROR_TITLE: &str = "ファイル選択を開けませんでした";
const SAVE_PATH_PICKER_ERROR_TITLE: &str = "保存先を選択できませんでした";
const EXPORT_ERROR_TITLE: &str = "書き出しを開始できませんでした";
const UPDATE_CHECK_ERROR_TITLE: &str = "更新を確認できませんでした";
const UPDATE_AVAILABLE_TITLE: &str = "新しいバージョンがあります";
const UPDATE_NOT_AVAILABLE_TITLE: &str = "最新版を使用しています";
const CURRENT_DIRECTORY_FALLBACK: &str = ".";
const WINDOW_TITLE_SEPARATOR: &str = " - ";
const RELEASES_LATEST_API_URL: &str = "https://api.github.com/repos/monjara/genko/releases/latest";
const MODAL_ERROR_ICON_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/modal_error.svg"
);
const MODAL_INFO_ICON_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/modal_info.svg"
);
const MODAL_UPDATE_ICON_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/modal_update.svg"
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

pub(crate) struct SoukouApp {
    editor_controller: Entity<EditorController>,
    active_document: ActiveDocument,
    active_modal: Option<AppModal>,
    title_bar: Entity<TitleBar>,
    bottom_bar: Entity<BottomBar>,
}

#[derive(Clone, Debug)]
enum AppModal {
    Error {
        title: String,
        detail: String,
    },
    Info {
        title: String,
        detail: String,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
        release_page_url: String,
    },
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
