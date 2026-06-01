use std::path::{Path, PathBuf};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::process::Command;

use bottom_bar::BottomBar;
use document::{
    ActiveDocument, DocumentKind,
    document_io::{
        DocumentOpenTarget, SavedDocumentTarget, classify_open_path, classify_saved_document_path,
        current_directory_or_fallback, suggested_file_name, suggested_save_directory,
    },
};
use editor::{
    EditorController, Event as EditorEvent, RubyEditRequest, VimCommandQuit, VimCommandWrite,
};
use gpui::{
    AnyWindowHandle, App, AppContext, BoxShadow, Context, Entity, ExternalPaths, FocusHandle,
    Focusable, FontWeight, Hsla, InteractiveElement, IntoElement, KeyBinding, ParentElement,
    Pixels, Render, RenderOnce, Styled, Subscription, Window, div, point, prelude::FluentBuilder,
    px, svg, transparent_black, white,
};
use menu::{
    MenuActionHandler, OpenFile, OpenSettings, Quit, RichTextBold, RichTextEmphasis,
    RichTextHeading, RichTextPageBreak, RichTextRuby, SaveFile,
};
use rich_text::{RichTextDocumentMeta, RichTextEdit, RichTextKind};
use settings::AppSettings;
use theme::{APP_FONT_FAMILY, Theme};
use title_bar::TitleBar;
use ui::TextInput;
use workspace::{
    Event as WorkspaceEvent, OpenWorkspace, ToggleWorkspacePane, Workspace, WorkspaceState,
    scan_workspace_entries,
};

use crate::{DismissActiveModal, OpenModalPrimary};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const UPDATE_AVAILABLE_TITLE: &str = "新しいバージョンがあります";
const WINDOW_TITLE_SEPARATOR: &str = " - ";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const SUPPORTED_OPEN_ERROR_DETAIL: &str = "現在は .txt ファイルに対応しています";
const UNSUPPORTED_DOCUMENT_SAVE_ERROR_DETAIL: &str = "サポートしていないファイルは保存できません";

pub(super) struct SoukouApp {
    editor_controller: Entity<EditorController>,
    workspace: Entity<Workspace>,
    active_document: ActiveDocument,
    rich_text_meta: RichTextDocumentMeta,
    rich_text_synced_revision: u64,
    rich_text_synced_text: String,
    ruby_editor: Option<RubyEditorState>,
    active_modal: Option<AppModal>,
    _workspace_subscription: Subscription,
    _editor_subscription: Subscription,
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

struct RubyEditorState {
    request: RubyEditRequest,
    input: Entity<TextInput>,
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
        self.active_modal = Some(AppModal::Error {
            title: title.to_string(),
            detail,
        });
        cx.notify();
    }

    fn show_info_modal(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.active_modal = Some(AppModal::Info {
            title: title.to_string(),
            detail,
        });
        cx.notify();
    }

    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let quit_mac = AppSettings::global(cx).keymap_keystroke("app.quit.mac");
        let open_file_mac = AppSettings::global(cx).keymap_keystroke("app.open_file.mac");
        let save_file_mac = AppSettings::global(cx).keymap_keystroke("app.save_file.mac");
        let toggle_workspace_mac = AppSettings::global(cx).keymap_keystroke("workspace.toggle.mac");

        let open_settings_ctrl = AppSettings::global(cx).keymap_keystroke("app.open_settings.ctrl");
        let open_file_ctrl = AppSettings::global(cx).keymap_keystroke("app.open_file.ctrl");
        let save_file_ctrl = AppSettings::global(cx).keymap_keystroke("app.save_file.ctrl");
        let toggle_workspace_ctrl =
            AppSettings::global(cx).keymap_keystroke("workspace.toggle.ctrl");

        cx.bind_keys([
            KeyBinding::new(quit_mac.as_ref(), Quit, None),
            KeyBinding::new(open_settings_ctrl.as_ref(), OpenSettings, None),
            KeyBinding::new(open_file_mac.as_ref(), OpenFile, None),
            KeyBinding::new(open_file_ctrl.as_ref(), OpenFile, None),
            KeyBinding::new(save_file_mac.as_ref(), SaveFile, None),
            KeyBinding::new(save_file_ctrl.as_ref(), SaveFile, None),
            KeyBinding::new(toggle_workspace_mac.as_ref(), ToggleWorkspacePane, None),
            KeyBinding::new(toggle_workspace_ctrl.as_ref(), ToggleWorkspacePane, None),
        ]);

        let editor_controller = cx.new(EditorController::new);
        let editor_subscription =
            cx.subscribe(&editor_controller, |this, _editor_controller, event, cx| {
                this.handle_editor_event(event.clone(), cx);
            });
        let workspace = cx.new(Workspace::new);
        let workspace_subscription =
            cx.subscribe(&workspace, |this, _workspace, event, cx| match event {
                WorkspaceEvent::OpenPath(path) => this.open_workspace_path(path.clone(), cx),
            });
        let title_bar = cx.new(|cx| TitleBar::new(menu::APP_NAME, menu::title_bar_menus(), cx));
        let bottom_bar = cx.new(BottomBar::new);

        Self {
            editor_controller,
            workspace,
            active_document: ActiveDocument::default(),
            rich_text_meta: RichTextDocumentMeta::default(),
            rich_text_synced_revision: 0,
            rich_text_synced_text: String::new(),
            ruby_editor: None,
            active_modal: None,
            _workspace_subscription: workspace_subscription,
            _editor_subscription: editor_subscription,
            title_bar,
            bottom_bar,
        }
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

        if let Some(AppModal::UpdateAvailable {
            release_page_url, ..
        }) = modal
        {
            self.open_external_url(release_page_url.as_str(), window, cx)
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

    fn workspace_pane_visible(&self, cx: &App) -> bool {
        WorkspaceState::global(cx).is_pane_visible()
    }

    fn load_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_plain_text(text, cx);
            editor_controller.set_rich_text_meta(rich_text_meta.clone(), cx);
        });
        self.active_document.set_path(path);
        self.rich_text_meta = rich_text_meta;
        self.rich_text_synced_revision = self.editor_controller.read(cx).draft_revision(cx);
        self.rich_text_synced_text = text.to_string();
        self.ruby_editor = None;
    }

    fn notify_workspace(&self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |_, cx| cx.notify());
    }

    fn open_workspace_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file(path.clone());
        self.load_plain_document(path, text, rich_text_meta, cx);
        self.notify_workspace(cx);
    }

    fn open_standalone_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file_without_root(path.clone());
        self.load_plain_document(path, text, rich_text_meta, cx);
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

    fn save_document_to_path(
        &mut self,
        path: PathBuf,
        contents: String,
        _window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let rich_text_meta = self.rich_text_meta.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    write_plain_document_assets(path, contents, rich_text_meta)
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
                                WorkspaceState::global_mut(cx).open_file(path.clone());
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
                        this.show_error_modal(FILE_SAVE_ERROR_TITLE, error.to_string(), cx);
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
        cx.spawn(async move |this, cx| {
            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move { read_plain_document_assets(path_for_read) })
                .await;

            match result {
                Ok((path, text, rich_text_meta)) => {
                    if let Err(error) = this.update(cx, |this, cx| match document_kind {
                        DocumentKind::PlainText => {
                            if preserve_workspace {
                                this.open_workspace_plain_document(path, &text, rich_text_meta, cx);
                            } else {
                                this.open_standalone_plain_document(
                                    path,
                                    &text,
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
                        this.show_error_modal(FILE_OPEN_ERROR_TITLE, error.to_string(), cx);
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

    fn selected_byte_range(&self, cx: &App) -> std::ops::Range<usize> {
        self.editor_controller.read(cx).selected_byte_range(cx)
    }

    fn selected_text(&self, cx: &App) -> String {
        let text = self.snapshot_text(cx);
        let range = self.selected_byte_range(cx);
        if range.end <= text.len()
            && text.is_char_boundary(range.start)
            && text.is_char_boundary(range.end)
        {
            text[range].to_string()
        } else {
            String::new()
        }
    }

    fn apply_rich_text_kind(&mut self, kind: RichTextKind, cx: &mut Context<Self>) {
        let range = self.editor_controller.read(cx).selected_byte_range(cx);
        let range = if matches!(kind, RichTextKind::PageBreak) {
            range.start..range.start
        } else {
            range
        };
        self.rich_text_meta.add_mark(range, kind);
        self.rich_text_synced_revision = self.editor_controller.read(cx).draft_revision(cx);
        self.rich_text_synced_text = self.snapshot_text(cx);
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_rich_text_meta(self.rich_text_meta.clone(), cx);
        });

        self.save_rich_text_meta(cx);

        cx.notify();
    }

    fn rich_text_ruby_action(
        &mut self,
        _: &RichTextRuby,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_range = self.selected_byte_range(cx);
        if selected_range.is_empty() {
            self.show_error_modal(
                "ルビを適用できません",
                "｜本文《ルビ》の形式で入力した範囲を選択してください。".to_string(),
                cx,
            );
            return;
        }

        let selected_text = self.selected_text(cx);
        let Some(parsed_ruby) = rich_text::parse_ruby_markup(selected_text.as_str()) else {
            self.show_error_modal(
                "ルビを適用できません",
                "｜本文《ルビ》の形式で入力した範囲を選択してください。".to_string(),
                cx,
            );
            return;
        };

        let base_start = selected_range.start;
        let base_end = base_start + parsed_ruby.base.len();
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.replace_byte_range(selected_range, parsed_ruby.base.as_str(), cx);
        });
        self.rich_text_meta.add_mark(
            base_start..base_end,
            RichTextKind::Ruby {
                text: parsed_ruby.ruby,
            },
        );
        self.rich_text_synced_revision = self.editor_controller.read(cx).draft_revision(cx);
        self.rich_text_synced_text = self.snapshot_text(cx);
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_rich_text_meta(self.rich_text_meta.clone(), cx);
        });

        self.save_rich_text_meta(cx);

        cx.notify();
    }

    fn export_epub_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        cx: &mut Context<Self>,
    ) {
        let title = self.export_base_name(cx);
        let rich_text_meta = self.rich_text_meta.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rich_text::export_epub(
                        &path,
                        title.as_str(),
                        contents.as_str(),
                        &rich_text_meta,
                    )
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

impl Render for SoukouApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_rich_text_meta_after_editor_edits(cx);
        title_bar::sync_client_window_inset(window);
        self.sync_window_title(window, cx);
        let title_bar_height = title_bar::platform_title_bar_height(window);
        let bottom_bar_height = bottom_bar::height(window);
        let occupied_workspace_width = if self.workspace_pane_visible(cx) {
            WorkspaceState::global(cx).pane_width()
        } else {
            0.0
        };
        let mut editor_viewport_size = window.viewport_size();
        editor_viewport_size.width =
            px((editor_viewport_size.width.as_f32() - occupied_workspace_width).max(0.0));
        editor_viewport_size.height -= title_bar_height + bottom_bar_height;
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
                    .map(|this| title_bar::apply_client_side_window_frame(this, window))
                    .can_drop(|value, _, _| value.is::<ExternalPaths>())
                    .on_drop(cx.listener(Self::drop_external_paths))
                    .on_action(cx.listener(Self::open_file_action))
                    .on_action(cx.listener(Self::open_workspace_action))
                    .on_action(cx.listener(Self::toggle_workspace_pane_action))
                    .on_action(cx.listener(Self::dismiss_active_modal_action))
                    .on_action(cx.listener(Self::open_modal_primary_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::export_txt_action))
                    .on_action(cx.listener(Self::export_epub_action))
                    .on_action(cx.listener(Self::rich_text_bold_action))
                    .on_action(cx.listener(Self::rich_text_emphasis_action))
                    .on_action(cx.listener(Self::rich_text_ruby_action))
                    .on_action(cx.listener(Self::rich_text_heading_action))
                    .on_action(cx.listener(Self::rich_text_page_break_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
                    .child(self.title_bar.clone().into_element())
                    .child(
                        div()
                            .flex_1()
                            .w_full()
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
                    .when_some(self.active_modal.clone(), |this, modal| {
                        this.child(ActiveModal::from_modal(modal))
                    })
                    .when_some(self.rich_text_toolbar_bounds(cx), |this, bounds| {
                        this.child(RichTextToolbar::new(bounds))
                    })
                    .when_some(self.ruby_editor.as_ref(), |this, ruby_editor| {
                        this.child(RubyEditorPopover::new(
                            ruby_editor.request.bounds,
                            ruby_editor.input.clone(),
                            cx.entity(),
                        ))
                    })
                    .child(self.bottom_bar.clone().into_element()),
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

    fn handle_editor_event(&mut self, event: EditorEvent, cx: &mut Context<Self>) {
        match event {
            EditorEvent::RubyEditRequested(request) => self.open_ruby_editor(request, cx),
        }
    }

    fn open_ruby_editor(&mut self, request: RubyEditRequest, cx: &mut Context<Self>) {
        if WorkspaceState::global(cx).unsupported_file().is_some() {
            return;
        }

        let input = cx.new(TextInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder("ルビ", cx);
            input.set_text(request.text.as_str(), cx);
        });
        self.ruby_editor = Some(RubyEditorState { request, input });
        cx.notify();
    }

    fn apply_ruby_editor(&mut self, cx: &mut Context<Self>) {
        let Some(ruby_editor) = self.ruby_editor.take() else {
            return;
        };
        let ruby_text = ruby_editor.input.read(cx).text();
        self.rich_text_meta
            .set_ruby(ruby_editor.request.range, ruby_text);
        self.rich_text_synced_revision = self.editor_controller.read(cx).draft_revision(cx);
        self.rich_text_synced_text = self.snapshot_text(cx);
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_rich_text_meta(self.rich_text_meta.clone(), cx);
        });
        self.save_rich_text_meta(cx);
        cx.notify();
    }

    fn cancel_ruby_editor(&mut self, cx: &mut Context<Self>) {
        self.ruby_editor = None;
        cx.notify();
    }

    fn save_rich_text_meta(&self, cx: &mut Context<Self>) {
        let Some(path) = self.active_document.path().map(Path::to_path_buf) else {
            return;
        };
        let rich_text_meta = self.rich_text_meta.clone();
        cx.background_spawn(async move {
            if let Err(error) = rich_text::save_meta_for_text_path(&path, &rich_text_meta) {
                eprintln!("failed to save rich text metadata: {error}");
            }
        })
        .detach();
    }

    fn sync_rich_text_meta_after_editor_edits(&mut self, cx: &mut Context<Self>) {
        let (current_revision, current_text, edit_batch) = {
            let editor_controller = self.editor_controller.read(cx);
            (
                editor_controller.draft_revision(cx),
                editor_controller.snapshot_text(cx),
                editor_controller.last_applied_edit_batch(cx),
            )
        };
        if current_revision <= self.rich_text_synced_revision {
            return;
        }

        if current_text == self.rich_text_synced_text {
            self.rich_text_synced_revision = current_revision;
            return;
        }

        let rich_text_edits = edit_batch
            .filter(|edit_batch| edit_batch.revision() == current_revision)
            .map(|edit_batch| {
                edit_batch
                    .edits()
                    .iter()
                    .map(|edit| {
                        RichTextEdit::new(
                            edit.start(),
                            edit.removed_text().to_string(),
                            edit.inserted_text().to_string(),
                            edit.affects_rich_text(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        rich_text::sync_meta_after_text_change(
            &mut self.rich_text_meta,
            self.rich_text_synced_text.as_str(),
            current_text.as_str(),
            rich_text_edits.as_slice(),
        );
        self.rich_text_synced_revision = current_revision;
        self.rich_text_synced_text = current_text;
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_rich_text_meta(self.rich_text_meta.clone(), cx);
        });
    }
}

#[derive(IntoElement)]
struct RubyEditorPopover {
    bounds: gpui::Bounds<Pixels>,
    input: Entity<TextInput>,
    app: Entity<SoukouApp>,
}

impl RubyEditorPopover {
    fn new(bounds: gpui::Bounds<Pixels>, input: Entity<TextInput>, app: Entity<SoukouApp>) -> Self {
        Self { bounds, input, app }
    }
}

impl RenderOnce for RubyEditorPopover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let input = self.input;
        let focus_handle = input.focus_handle(cx);
        window.focus(&focus_handle, cx);

        let app_for_apply = self.app.clone();
        let app_for_cancel = self.app;
        let left = (self.bounds.right() + px(6.0)).max(px(8.0));
        let top = (self.bounds.top() - px(4.0)).max(px(8.0));

        div()
            .absolute()
            .left(left)
            .top(top)
            .w(px(190.0))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .bg(Theme::global(cx).white())
            .border_1()
            .border_color(toolbar_border_color(cx))
            .rounded_md()
            .shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.18,
                },
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(20.0),
                spread_radius: px(0.0),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(input)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(ruby_editor_button("取消", cx, move |cx| {
                        app_for_cancel.update(cx, |app, cx| app.cancel_ruby_editor(cx));
                    }))
                    .child(ruby_editor_button("適用", cx, move |cx| {
                        app_for_apply.update(cx, |app, cx| app.apply_ruby_editor(cx));
                    })),
            )
    }
}

fn ruby_editor_button(
    label: &'static str,
    cx: &mut App,
    on_click: impl Fn(&mut App) + Clone + 'static,
) -> impl IntoElement {
    let on_click = on_click.clone();
    div()
        .px_2()
        .py_1()
        .rounded_sm()
        .text_size(px(12.0))
        .text_color(Theme::global(cx).text_primary())
        .bg(Theme::global(cx).bg_senodary())
        .cursor_pointer()
        .hover(|style| style.bg(white()))
        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
            on_click(cx);
        })
        .child(label)
}

#[derive(IntoElement)]
struct RichTextToolbar {
    selection_bounds: gpui::Bounds<Pixels>,
}

impl RichTextToolbar {
    fn new(selection_bounds: gpui::Bounds<Pixels>) -> Self {
        Self { selection_bounds }
    }
}

impl RenderOnce for RichTextToolbar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let x = (self.selection_bounds.right() + px(1.0)).max(px(1.0));
        let y = (self.selection_bounds.top() - px(10.0)).max(px(8.0));
        let border = toolbar_border_color(cx);

        div()
            .absolute()
            .left(x)
            .top(y)
            .flex()
            .flex_col()
            .items_center()
            .bg(Theme::global(cx).white())
            .border_1()
            .border_color(border)
            .rounded_md()
            .shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.2,
                },
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(22.0),
                spread_radius: px(0.0),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(toolbar_button("B", RichTextBold, cx))
            .child(toolbar_button("•", RichTextEmphasis, cx))
            .child(toolbar_button("ル", RichTextRuby, cx))
            .child(toolbar_button("見", RichTextHeading, cx))
            .child(toolbar_button("改", RichTextPageBreak, cx))
    }
}

fn toolbar_button<Action>(label: &'static str, action: Action, cx: &mut App) -> impl IntoElement
where
    Action: gpui::Action + Clone + 'static,
{
    div()
        .w(px(38.0))
        .h(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.0))
        .font_weight(FontWeight::BOLD)
        .text_color(Theme::global(cx).black())
        .border_b_1()
        .border_color(toolbar_border_color(cx))
        .cursor_pointer()
        .hover(|style| style.bg(white()))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(action.clone()), cx);
        })
        .child(label)
}

#[derive(IntoElement)]
struct ActiveModal {
    icon_path: &'static str,
    title: String,
    subtitle: String,
    detail: String,
    secondary_label: Option<String>,
    primary_label: Option<String>,
}

impl ActiveModal {
    fn from_modal(modal: AppModal) -> Self {
        let (icon_path, title, subtitle, detail, secondary_label, primary_label) = match modal {
            AppModal::Error { title, detail } => (
                icons::MODAL_ERROR,
                title,
                "操作を完了できませんでした。".to_string(),
                detail,
                None,
                Some("閉じる".to_string()),
            ),
            AppModal::Info { title, detail } => (
                icons::MODAL_INFO,
                title,
                String::new(),
                detail,
                None,
                Some("閉じる".to_string()),
            ),
            AppModal::UpdateAvailable {
                current_version,
                latest_version,
                ..
            } => (
                icons::MODAL_UPDATE,
                UPDATE_AVAILABLE_TITLE.to_string(),
                "ダウンロードページを開いて更新できます。".to_string(),
                format!(
                    "現在のバージョンは {current_version}、最新バージョンは {latest_version} です。"
                ),
                Some("あとで".to_string()),
                Some("ダウンロード".to_string()),
            ),
        };

        Self {
            icon_path,
            title,
            subtitle,
            detail,
            secondary_label,
            primary_label,
        }
    }
}

impl RenderOnce for ActiveModal {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let accent = mix(Theme::global(cx).primary(), Theme::global(cx).white(), 0.84);

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .bg(Hsla {
                h: 0.61,
                s: 0.32,
                l: 0.08,
                a: 0.58,
            })
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(DismissActiveModal), cx);
            })
            .child(
                div()
                    .w(px(420.0))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .bg(Theme::global(cx).white())
                    .border_1()
                    .border_color(toolbar_border_color(cx))
                    .rounded_lg()
                    .shadow(vec![BoxShadow {
                        color: Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 0.0,
                            a: 0.18,
                        },
                        offset: point(px(0.0), px(18.0)),
                        blur_radius: px(42.0),
                        spread_radius: px(0.0),
                    }])
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(46.0))
                                    .h(px(46.0))
                                    .rounded_full()
                                    .bg(accent)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .external_path(self.icon_path)
                                            .size_6()
                                            .text_color(Theme::global(cx).primary()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .child(self.title),
                                    )
                                    .when(!self.subtitle.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_color(Theme::global(cx).text_senodary())
                                                .child(self.subtitle),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(Theme::global(cx).text_senodary())
                                            .child(self.detail),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .when_some(self.secondary_label, |this, label| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(toolbar_border_color(cx))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(gpui::rgb(0xf4f5f6)))
                                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                                            window
                                                .dispatch_action(Box::new(DismissActiveModal), cx);
                                        })
                                        .child(label),
                                )
                            })
                            .when_some(self.primary_label, |this, label| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .rounded_sm()
                                        .bg(Theme::global(cx).primary())
                                        .text_color(Theme::global(cx).white())
                                        .cursor_pointer()
                                        .hover(|style| style.opacity(0.92))
                                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                                            window.dispatch_action(Box::new(OpenModalPrimary), cx);
                                        })
                                        .child(label),
                                )
                            }),
                    ),
            )
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

fn read_document_to_string(path: PathBuf) -> std::io::Result<(PathBuf, String)> {
    std::fs::read_to_string(&path).map(|text| (path, text))
}

fn read_plain_document_assets(
    path: PathBuf,
) -> std::io::Result<(PathBuf, String, RichTextDocumentMeta)> {
    let (path, text) = read_document_to_string(path)?;
    let rich_text_meta = rich_text::load_meta_for_text_path(path.as_path())?;
    Ok((path, text, rich_text_meta))
}

fn write_plain_document_assets(
    path: PathBuf,
    contents: String,
    rich_text_meta: RichTextDocumentMeta,
) -> std::io::Result<PathBuf> {
    std::fs::write(&path, contents.as_bytes())?;
    if !rich_text_meta.is_empty() {
        rich_text::save_meta_for_text_path(path.as_path(), &rich_text_meta)?;
    }
    Ok(path)
}
