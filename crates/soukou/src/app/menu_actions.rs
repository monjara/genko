use std::path::{Path, PathBuf};

use gpui::{AnyWindowHandle, Context};
use menu::MenuActionHandler;
use workspace::WorkspaceState;

use crate::{
    app::{APP_VERSION, AppModal, CURRENT_DIRECTORY_FALLBACK, FILE_SAVE_ERROR_TITLE, SoukouApp},
    document::DocumentKind,
};

impl MenuActionHandler for SoukouApp {
    fn app_version(&self) -> &'static str {
        APP_VERSION
    }

    fn open_path_from_menu(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_menu_path(path, cx);
    }

    fn export_base_name(&self, _cx: &gpui::App) -> String {
        self.active_document
            .path()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .unwrap_or("untitled")
            .to_string()
    }

    fn export_initial_directory(&self, _cx: &gpui::App) -> PathBuf {
        self.active_document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK))
    }

    fn snapshot_text(&self, cx: &gpui::App) -> String {
        match self.current_document_kind(cx) {
            DocumentKind::PlainText => self.editor_controller.read(cx).snapshot_text(cx),
        }
    }

    fn save_blocking_error(&self, cx: &gpui::App) -> Option<(&'static str, String)> {
        WorkspaceState::global(cx)
            .unsupported_file()
            .is_some()
            .then(|| {
                (
                    FILE_SAVE_ERROR_TITLE,
                    "サポートしていないファイルは保存できません".to_string(),
                )
            })
    }

    fn active_save_path(&self, _cx: &gpui::App) -> Option<PathBuf> {
        self.active_document.path().map(Path::to_path_buf)
    }

    fn suggested_save_directory(&self, cx: &gpui::App) -> PathBuf {
        WorkspaceState::global(cx)
            .suggested_save_directory()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK))
    }

    fn suggested_file_name(&self, cx: &gpui::App) -> String {
        WorkspaceState::global(cx)
            .suggested_file_name()
            .unwrap_or(self.current_document_kind(cx).default_file_name())
            .to_string()
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
