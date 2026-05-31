use crate::document::ActiveDocument;
use bottom_bar::BottomBar;
use editor::EditorController;
use gpui::{App, Entity, Subscription};
use theme::Theme;
use title_bar::TitleBar;
use workspace::Workspace;

mod active_modal;
mod document_io;
mod menu_actions;
mod render;
mod state;
mod unsupported_document;

pub(crate) const APP_ID: &str = "dev.monj.soukou";
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const MAIN_WINDOW_WIDTH: f32 = 1200.0;
pub(crate) const MAIN_WINDOW_HEIGHT: f32 = 800.0;

const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const UPDATE_AVAILABLE_TITLE: &str = "新しいバージョンがあります";
const CURRENT_DIRECTORY_FALLBACK: &str = ".";
const WINDOW_TITLE_SEPARATOR: &str = " - ";

pub(crate) struct SoukouApp {
    editor_controller: Entity<EditorController>,
    workspace: Entity<Workspace>,
    active_document: ActiveDocument,
    active_modal: Option<AppModal>,
    _workspace_subscription: Subscription,
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
