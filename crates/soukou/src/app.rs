mod modal;
use modal::{ActiveModal, AppModal, EpubMetaFormOverlay};

use std::path::{Path, PathBuf};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::process::Command;

use crate::{
    CommandPaletteSelectNext, CommandPaletteSelectPrevious, ConfirmCommandPalette, ConfirmEpubMeta,
    ConfirmFilePicker, DismissActiveModal, DismissCommandPalette, DismissEpubMetaForm,
    DismissFilePicker, ExecuteCommandPaletteCommand, FilePickerSelectNext,
    FilePickerSelectPrevious, OpenCommandPalette, OpenFilePicker, OpenFilePickerPath,
    OpenModalPrimary,
    notification::{
        DismissErrorNotification, ErrorNotification, ErrorNotificationStack, ErrorPresentation,
    },
};
use bottom_bar::BottomBar;
use document::{
    ActiveDocument, DocumentError, DocumentKind,
    document_io::{
        DocumentOpenTarget, SavedDocumentTarget, classify_open_path, classify_saved_document_path,
        current_directory_or_fallback, suggested_file_name, suggested_save_directory,
    },
};
use editor::{
    ApplyRichTextBold, ApplyRichTextEmphasis, ApplyRichTextHeading, ApplyRichTextRotated,
    EditorController, OpenSearch, PreparedPlainText, RemovePageBreakCurrentColumn, RichTextToolbar,
    SetPageBreakLeftOfCurrentColumn, SetPageBreakRightOfCurrentColumn, VimCommandQuit,
    VimCommandWrite,
};
use gpui::{
    App, AppContext, BoxShadow, Context, Entity, ExternalPaths, FocusHandle, Focusable, FontWeight,
    Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, Render, RenderOnce, Styled,
    Subscription, Window, div, point, prelude::FluentBuilder, px, transparent_black,
};
use menu::{MenuActionHandler, RegisterAccount, SignOut};
use rich_text::{EpubBookMeta, RichTextDocumentMeta};
use settings::AppSettings;
use theme::{APP_FONT_FAMILY, Theme, ThemeMode};
use title_bar::TitleBar;
use workspace::{
    Event as WorkspaceEvent, OpenWorkspace, ToggleWorkspacePane, Workspace, WorkspaceFileEntry,
    WorkspaceState, scan_workspace_entries, scan_workspace_file_entries,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const WINDOW_TITLE_SEPARATOR: &str = " - ";
const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
const SUPPORTED_OPEN_ERROR_DETAIL: &str = "現在は .txt ファイルに対応しています";
const UNSUPPORTED_DOCUMENT_SAVE_ERROR_DETAIL: &str = "サポートしていないファイルは保存できません";
const BOTTOM_BAR_TOP_GAP: f32 = 4.0;
const FILE_PICKER_LIMIT: usize = 12;
const COMMAND_PALETTE_LIMIT: usize = 12;

pub(super) struct SoukouApp {
    editor_controller: Entity<EditorController>,
    workspace: Entity<Workspace>,
    active_document: ActiveDocument,
    epub_meta_form: Option<EpubMetaFormState>,
    file_picker: Option<FilePickerState>,
    command_palette: Option<CommandPaletteState>,
    active_modal: Option<AppModal>,
    error_notifications: Vec<ErrorNotification>,
    next_error_notification_id: u64,
    _workspace_subscription: Subscription,
    appearance_subscription: Option<Subscription>,
    title_bar: Entity<TitleBar>,
    bottom_bar: Entity<BottomBar>,
}

struct EpubMetaFormState {
    title_input: Entity<ui::TextInput>,
    author_input: Entity<ui::TextInput>,
}

struct FilePickerState {
    input: Entity<ui::TextInput>,
    entries: Vec<WorkspaceFileEntry>,
    matches: Vec<WorkspaceFileEntry>,
    selected_index: usize,
    _input_subscription: Subscription,
}

struct CommandPaletteState {
    input: Entity<ui::TextInput>,
    entries: Vec<CommandPaletteEntry>,
    matches: Vec<CommandPaletteEntry>,
    selected_index: usize,
    _input_subscription: Subscription,
}

#[derive(Clone)]
struct CommandPaletteEntry {
    id: usize,
    title: &'static str,
    detail: &'static str,
    dispatch: fn(&mut Window, &mut Context<SoukouApp>),
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
        self.push_error_notification(title.to_string(), detail, cx);
    }

    fn push_error_notification(&mut self, title: String, detail: String, cx: &mut Context<Self>) {
        let id = self.next_error_notification_id;
        self.next_error_notification_id = self.next_error_notification_id.saturating_add(1);
        self.error_notifications
            .push(ErrorNotification { id, title, detail });
        cx.notify();
    }

    fn show_error_from(&mut self, error: impl Into<ErrorPresentation>, cx: &mut Context<Self>) {
        let presentation = error.into();
        self.push_error_notification(presentation.title, presentation.detail, cx);
    }

    fn show_info_modal(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.active_modal = Some(AppModal::Info {
            title: title.to_string(),
            detail,
        });
        cx.notify();
    }

    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let loaded_key_bindings = keymap::load_key_bindings(cx);
        cx.bind_keys(loaded_key_bindings.key_bindings);
        let error_notifications = loaded_key_bindings
            .error
            .map(|error| {
                let presentation = ErrorPresentation::from(error);
                vec![ErrorNotification {
                    id: 0,
                    title: presentation.title,
                    detail: presentation.detail,
                }]
            })
            .unwrap_or_default();
        let next_error_notification_id = error_notifications
            .last()
            .map(|notification| notification.id.saturating_add(1))
            .unwrap_or(0);

        let editor_controller = cx.new(EditorController::new);
        let workspace = cx.new(Workspace::new);
        let workspace_subscription =
            cx.subscribe(&workspace, |this, _workspace, event, cx| match event {
                WorkspaceEvent::OpenPath(path) => this.open_workspace_path(path.clone(), cx),
            });
        let title_bar = cx.new(|cx| TitleBar::new(menu::title_bar_menus(), cx));
        let bottom_bar = cx.new(BottomBar::new);

        Self {
            editor_controller,
            workspace,
            active_document: ActiveDocument::default(),
            epub_meta_form: None,
            file_picker: None,
            command_palette: None,
            active_modal: None,
            error_notifications,
            next_error_notification_id,
            _workspace_subscription: workspace_subscription,
            appearance_subscription: None,
            title_bar,
            bottom_bar,
        }
    }

    fn dismiss_error_notification(&mut self, id: u64, cx: &mut Context<Self>) {
        self.error_notifications
            .retain(|notification| notification.id != id);
        cx.notify();
    }

    fn dismiss_error_notification_action(
        &mut self,
        action: &DismissErrorNotification,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_error_notification(action.id, cx);
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

        match modal {
            Some(AppModal::UpdateAvailable {
                release_page_url, ..
            }) => self.open_external_url(release_page_url.as_str(), window, cx),
            Some(AppModal::Info { .. }) | None => {}
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

    fn open_file_picker_action(
        &mut self,
        _: &OpenFilePicker,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file_picker(cx);
    }

    fn open_file_picker(&mut self, cx: &mut Context<Self>) {
        let Some(root_dir) = WorkspaceState::global(cx).root_dir().map(Path::to_path_buf) else {
            self.show_error_modal(
                "ワークスペースが開かれていません",
                "フォルダを開いてからファイル検索を使ってください。".to_string(),
                cx,
            );
            return;
        };

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { scan_workspace_file_entries(root_dir.as_path()) })
                .await;

            match result {
                Ok(entries) => {
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.show_file_picker(entries, cx);
                    }) {
                        eprintln!("failed to show file picker: {error}");
                    }
                }
                Err(error) => {
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.show_error_modal(FILE_OPEN_ERROR_TITLE, error.to_string(), cx);
                    }) {
                        eprintln!("failed to show file picker error: {update_error}");
                    }
                }
            }
        })
        .detach();
    }

    fn show_file_picker(&mut self, entries: Vec<WorkspaceFileEntry>, cx: &mut Context<Self>) {
        let input = cx.new(|cx| {
            let mut input = ui::TextInput::new(cx);
            input.set_key_context("SoukouTextInput file_picker", cx);
            input.set_placeholder("ファイルを検索", cx);
            input
        });
        let input_subscription = cx.observe(&input, |this, input, cx| {
            let query = input.read(cx).text();
            this.update_file_picker_matches(query.as_str(), cx);
        });

        let matches = filter_file_picker_entries(entries.as_slice(), "");
        self.file_picker = Some(FilePickerState {
            input,
            entries,
            matches,
            selected_index: 0,
            _input_subscription: input_subscription,
        });
        cx.notify();
    }

    fn update_file_picker_matches(&mut self, query: &str, cx: &mut Context<Self>) {
        let Some(file_picker) = self.file_picker.as_mut() else {
            return;
        };
        file_picker.matches = filter_file_picker_entries(file_picker.entries.as_slice(), query);
        file_picker.selected_index = 0;
        cx.notify();
    }

    fn dismiss_file_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.file_picker = None;
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
    }

    fn dismiss_file_picker_action(
        &mut self,
        _: &DismissFilePicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_file_picker(window, cx);
    }

    fn confirm_file_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.file_picker.as_ref().and_then(|file_picker| {
            file_picker
                .matches
                .get(file_picker.selected_index)
                .map(|entry| entry.path().to_path_buf())
        });
        self.dismiss_file_picker(window, cx);

        if let Some(path) = path {
            self.open_workspace_path(path, cx);
        }
    }

    fn confirm_file_picker_action(
        &mut self,
        _: &ConfirmFilePicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_file_picker(window, cx);
    }

    fn open_file_picker_path_action(
        &mut self,
        action: &OpenFilePickerPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = action.path.clone();
        self.dismiss_file_picker(window, cx);
        self.open_workspace_path(path, cx);
    }

    fn select_next_file_picker_entry(
        &mut self,
        _: &FilePickerSelectNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(file_picker) = self.file_picker.as_mut() else {
            return;
        };
        let selectable_len = file_picker_selectable_len(file_picker.matches.len());
        if selectable_len == 0 {
            return;
        }
        file_picker.selected_index = (file_picker.selected_index + 1) % selectable_len;
        cx.notify();
    }

    fn select_previous_file_picker_entry(
        &mut self,
        _: &FilePickerSelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(file_picker) = self.file_picker.as_mut() else {
            return;
        };
        let selectable_len = file_picker_selectable_len(file_picker.matches.len());
        if selectable_len == 0 {
            return;
        }
        file_picker.selected_index = file_picker
            .selected_index
            .checked_sub(1)
            .unwrap_or(selectable_len - 1);
        cx.notify();
    }

    fn open_command_palette_action(
        &mut self,
        _: &OpenCommandPalette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_command_palette(cx);
    }

    fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        let input = cx.new(|cx| {
            let mut input = ui::TextInput::new(cx);
            input.set_key_context("SoukouTextInput command_palette", cx);
            input.set_placeholder("コマンドを検索", cx);
            input
        });
        let input_subscription = cx.observe(&input, |this, input, cx| {
            let query = input.read(cx).text();
            this.update_command_palette_matches(query.as_str(), cx);
        });

        let entries = command_palette_entries();
        let matches = filter_command_palette_entries(entries.as_slice(), "");
        self.command_palette = Some(CommandPaletteState {
            input,
            entries,
            matches,
            selected_index: 0,
            _input_subscription: input_subscription,
        });
        cx.notify();
    }

    fn update_command_palette_matches(&mut self, query: &str, cx: &mut Context<Self>) {
        let Some(command_palette) = self.command_palette.as_mut() else {
            return;
        };
        command_palette.matches =
            filter_command_palette_entries(command_palette.entries.as_slice(), query);
        command_palette.selected_index = 0;
        cx.notify();
    }

    fn dismiss_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette = None;
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
    }

    fn dismiss_command_palette_action(
        &mut self,
        _: &DismissCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_command_palette(window, cx);
    }

    fn confirm_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = self.command_palette.as_ref().and_then(|command_palette| {
            command_palette
                .matches
                .get(command_palette.selected_index)
                .cloned()
        });
        self.dismiss_command_palette(window, cx);

        if let Some(command) = command {
            (command.dispatch)(window, cx);
        }
    }

    fn confirm_command_palette_action(
        &mut self,
        _: &ConfirmCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_command_palette(window, cx);
    }

    fn execute_command_palette_command_action(
        &mut self,
        action: &ExecuteCommandPaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self.command_palette.as_ref().and_then(|command_palette| {
            command_palette
                .entries
                .iter()
                .find(|entry| entry.id == action.command_id)
                .cloned()
        });
        self.dismiss_command_palette(window, cx);

        if let Some(command) = command {
            (command.dispatch)(window, cx);
        }
    }

    fn select_next_command_palette_entry(
        &mut self,
        _: &CommandPaletteSelectNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command_palette) = self.command_palette.as_mut() else {
            return;
        };
        let selectable_len = command_palette_selectable_len(command_palette.matches.len());
        if selectable_len == 0 {
            return;
        }
        command_palette.selected_index = (command_palette.selected_index + 1) % selectable_len;
        cx.notify();
    }

    fn select_previous_command_palette_entry(
        &mut self,
        _: &CommandPaletteSelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command_palette) = self.command_palette.as_mut() else {
            return;
        };
        let selectable_len = command_palette_selectable_len(command_palette.matches.len());
        if selectable_len == 0 {
            return;
        }
        command_palette.selected_index = command_palette
            .selected_index
            .checked_sub(1)
            .unwrap_or(selectable_len - 1);
        cx.notify();
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

    fn selected_byte_range(&self, cx: &App) -> std::ops::Range<usize> {
        self.editor_controller.read(cx).selected_byte_range(cx)
    }

    fn workspace_pane_visible(&self, cx: &App) -> bool {
        WorkspaceState::global(cx).is_pane_visible()
    }

    fn key_context(&self) -> String {
        let mut context = String::from("SoukouApp");
        if self.active_modal.is_some() {
            context.push_str(" active_modal");
        }
        if self.epub_meta_form.is_some() {
            context.push_str(" epub_meta_form");
        }
        if self.file_picker.is_some() {
            context.push_str(" file_picker");
        }
        if self.command_palette.is_some() {
            context.push_str(" command_palette");
        }
        context
    }

    fn load_plain_document(
        &mut self,
        path: PathBuf,
        prepared_text: PreparedPlainText,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_prepared_plain_text(prepared_text, cx);
            editor_controller.set_rich_text_meta(rich_text_meta, cx);
        });
        self.active_document.set_path(path);
    }

    fn notify_workspace(&self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |_, cx| cx.notify());
    }

    fn open_workspace_plain_document(
        &mut self,
        path: PathBuf,
        prepared_text: PreparedPlainText,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file(path.clone());
        self.load_plain_document(path, prepared_text, rich_text_meta, cx);
        self.notify_workspace(cx);
    }

    fn open_standalone_plain_document(
        &mut self,
        path: PathBuf,
        prepared_text: PreparedPlainText,
        rich_text_meta: RichTextDocumentMeta,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file_without_root(path.clone());
        self.load_plain_document(path, prepared_text, rich_text_meta, cx);
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

    fn save_document_to_path(&mut self, path: PathBuf, contents: String, cx: &mut Context<Self>) {
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    write_plain_document_assets(path, contents, rich_text_meta, true)
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
                                WorkspaceState::global_mut(cx).open_saved_file(path.clone());
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
                        this.show_error_from(error, cx);
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
        let plain_text_load_settings = self.editor_controller.read(cx).plain_text_load_settings(cx);
        cx.spawn(async move |this, cx| {
            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move {
                    read_plain_document_assets(path_for_read).map(|(path, text, rich_text_meta)| {
                        (
                            path,
                            PreparedPlainText::new(text, plain_text_load_settings),
                            rich_text_meta,
                        )
                    })
                })
                .await;

            match result {
                Ok((path, prepared_text, rich_text_meta)) => {
                    if let Err(error) = this.update(cx, |this, cx| match document_kind {
                        DocumentKind::PlainText => {
                            if preserve_workspace {
                                this.open_workspace_plain_document(
                                    path,
                                    prepared_text,
                                    rich_text_meta,
                                    cx,
                                );
                            } else {
                                this.open_standalone_plain_document(
                                    path,
                                    prepared_text,
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
                        this.show_error_from(error, cx);
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

    fn export_epub_action(
        &mut self,
        _: &menu::ExportEpub,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_epub_meta_form(window, cx);
    }

    fn export_word_action(
        &mut self,
        _: &menu::ExportWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_word(window, cx);
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

    fn export_epub_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        cx: &mut Context<Self>,
    ) {
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rich_text::export_epub(&path, contents.as_str(), &rich_text_meta)
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

    fn export_word_path_from_menu(
        &mut self,
        path: PathBuf,
        contents: String,
        cx: &mut Context<Self>,
    ) {
        let rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rich_text::export_word(&path, contents.as_str(), &rich_text_meta)
                })
                .await;

            if let Err(error) = result
                && let Err(update_error) = this.update(cx, |this, cx| {
                    this.show_error_modal("Wordを書き出せませんでした", error.to_string(), cx);
                })
            {
                eprintln!("failed to show word export error: {update_error}");
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

    fn save_path_from_menu(&mut self, path: PathBuf, contents: String, cx: &mut Context<Self>) {
        self.save_document_to_path(path, contents, cx);
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
        title_bar::sync_client_window_inset(window);
        if self.appearance_subscription.is_none() {
            self.appearance_subscription =
                Some(cx.observe_window_appearance(window, |_, window, cx| {
                    if AppSettings::global(cx).theme_mode == ThemeMode::System {
                        theme::apply_mode_for_window_appearance(
                            ThemeMode::System,
                            window.appearance(),
                            cx,
                        );
                        cx.refresh_windows();
                    }
                }));
        }
        self.sync_window_title(window, cx);
        let title_bar_height = title_bar::platform_title_bar_height(window);
        let bottom_bar_height = bottom_bar::height(window);
        let key_context = self.key_context();
        let client_shadow_padding = title_bar::client_side_shadow_padding_size(window);
        let occupied_workspace_width = if self.workspace_pane_visible(cx) {
            WorkspaceState::global(cx).pane_width()
        } else {
            0.0
        };
        let mut editor_viewport_size = window.viewport_size();
        editor_viewport_size.width = (editor_viewport_size.width
            - client_shadow_padding.width
            - px(occupied_workspace_width))
        .max(Pixels::ZERO);
        editor_viewport_size.height = (editor_viewport_size.height
            - client_shadow_padding.height
            - title_bar_height
            - bottom_bar_height
            - px(BOTTOM_BAR_TOP_GAP))
        .max(Pixels::ZERO);
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
                    .bg(Theme::global(cx).surface())
                    .font_family(APP_FONT_FAMILY)
                    .flex()
                    .flex_col()
                    .items_center()
                    .overflow_hidden()
                    .key_context(key_context.as_str())
                    .map(|this| title_bar::apply_client_side_window_frame(this, window))
                    .can_drop(|value, _, _| value.is::<ExternalPaths>())
                    .on_drop(cx.listener(Self::drop_external_paths))
                    .on_action(cx.listener(Self::open_file_action))
                    .on_action(cx.listener(Self::open_workspace_action))
                    .on_action(cx.listener(Self::open_file_picker_action))
                    .on_action(cx.listener(Self::dismiss_file_picker_action))
                    .on_action(cx.listener(Self::confirm_file_picker_action))
                    .on_action(cx.listener(Self::open_file_picker_path_action))
                    .on_action(cx.listener(Self::select_next_file_picker_entry))
                    .on_action(cx.listener(Self::select_previous_file_picker_entry))
                    .on_action(cx.listener(Self::open_command_palette_action))
                    .on_action(cx.listener(Self::dismiss_command_palette_action))
                    .on_action(cx.listener(Self::confirm_command_palette_action))
                    .on_action(cx.listener(Self::execute_command_palette_command_action))
                    .on_action(cx.listener(Self::select_next_command_palette_entry))
                    .on_action(cx.listener(Self::select_previous_command_palette_entry))
                    .on_action(cx.listener(Self::toggle_workspace_pane_action))
                    .on_action(cx.listener(Self::dismiss_active_modal_action))
                    .on_action(cx.listener(Self::dismiss_error_notification_action))
                    .on_action(cx.listener(Self::open_modal_primary_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::export_txt_action))
                    .on_action(cx.listener(Self::export_word_action))
                    .on_action(cx.listener(Self::export_epub_action))
                    .on_action(cx.listener(Self::confirm_epub_meta_action))
                    .on_action(cx.listener(Self::dismiss_epub_meta_form_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
                    .child(self.title_bar.clone().into_element())
                    .child(
                        div()
                            .flex_1()
                            .h(px(0.0))
                            .w_full()
                            .mb(px(BOTTOM_BAR_TOP_GAP))
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
                                    .bg(Theme::global(cx).bg_primary())
                                    .items_center()
                                    .justify_center()
                                    .child(content),
                            ),
                    )
                    .when_some(self.epub_meta_form.as_ref(), |this, form| {
                        this.child(EpubMetaFormOverlay::new(
                            form.title_input.clone(),
                            form.author_input.clone(),
                        ))
                    })
                    .when_some(self.file_picker.as_ref(), |this, file_picker| {
                        this.child(FilePickerOverlay::new(
                            file_picker.input.clone(),
                            file_picker.matches.clone(),
                            file_picker.selected_index,
                        ))
                    })
                    .when_some(self.command_palette.as_ref(), |this, command_palette| {
                        this.child(CommandPaletteOverlay::new(
                            command_palette.input.clone(),
                            command_palette.matches.clone(),
                            command_palette.selected_index,
                        ))
                    })
                    .when_some(self.active_modal.clone(), |this, modal| {
                        this.child(ActiveModal::from_modal(modal))
                    })
                    .when(!self.error_notifications.is_empty(), |this| {
                        this.child(ErrorNotificationStack::new(
                            self.error_notifications.clone(),
                            bottom_bar_height,
                        ))
                    })
                    .when_some(self.rich_text_toolbar_bounds(cx), |this, bounds| {
                        this.child(RichTextToolbar::new(bounds))
                    })
                    .child(
                        div()
                            .flex_none()
                            .w_full()
                            .h(bottom_bar_height)
                            .child(self.bottom_bar.clone().into_element()),
                    ),
            )
    }
}

#[derive(IntoElement)]
struct FilePickerOverlay {
    input: Entity<ui::TextInput>,
    matches: Vec<WorkspaceFileEntry>,
    selected_index: usize,
}

impl FilePickerOverlay {
    fn new(
        input: Entity<ui::TextInput>,
        matches: Vec<WorkspaceFileEntry>,
        selected_index: usize,
    ) -> Self {
        Self {
            input,
            matches,
            selected_index,
        }
    }
}

impl RenderOnce for FilePickerOverlay {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self.input.focus_handle(cx);
        window.focus(&focus_handle, cx);

        let mut shown_matches = Vec::new();
        for (index, entry) in self
            .matches
            .iter()
            .take(FILE_PICKER_LIMIT)
            .cloned()
            .enumerate()
        {
            shown_matches.push(
                file_picker_row(index, entry, index == self.selected_index, cx).into_any_element(),
            );
        }
        let result_label = if self.matches.is_empty() {
            "一致するファイルはありません".to_string()
        } else {
            format!("{}件", self.matches.len())
        };

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
                a: 0.42,
            })
            .flex()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(DismissFilePicker), cx);
            })
            .child(
                div()
                    .mt(px(92.0))
                    .w(px(640.0))
                    .max_w(px(640.0))
                    .h_auto()
                    .flex()
                    .flex_col()
                    .bg(Theme::global(cx).surface())
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
                        inset: false,
                    }])
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(toolbar_border_color(cx))
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(15.0))
                                    .child(self.input),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(Theme::global(cx).text_senodary())
                                    .child(result_label),
                            ),
                    )
                    .child(
                        div()
                            .py_2()
                            .flex()
                            .flex_col()
                            .when(self.matches.is_empty(), |this| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_3()
                                        .text_sm()
                                        .text_color(Theme::global(cx).text_senodary())
                                        .child("一致するファイルはありません"),
                                )
                            })
                            .children(shown_matches),
                    ),
            )
    }
}

fn file_picker_row(
    index: usize,
    entry: WorkspaceFileEntry,
    selected: bool,
    cx: &mut App,
) -> impl IntoElement {
    let path = entry.path().to_path_buf();
    let background = if selected {
        mix(
            Theme::global(cx).primary(),
            Theme::global(cx).surface(),
            0.88,
        )
    } else {
        Theme::global(cx).surface()
    };
    let text_color = if selected {
        Theme::global(cx).primary()
    } else {
        Theme::global(cx).text_primary()
    };

    div()
        .id(("file-picker-row", index))
        .mx_2()
        .px_3()
        .py_2()
        .rounded_sm()
        .bg(background)
        .text_color(text_color)
        .cursor_pointer()
        .hover(|style| style.bg(Theme::global(cx).bg_senodary()))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(OpenFilePickerPath { path: path.clone() }), cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .text_sm()
                .overflow_hidden()
                .child(entry.display_name().to_string()),
        )
}

fn filter_file_picker_entries(
    entries: &[WorkspaceFileEntry],
    query: &str,
) -> Vec<WorkspaceFileEntry> {
    let query = query.trim().to_lowercase();
    let mut matches = entries
        .iter()
        .filter(|entry| file_picker_entry_matches(entry.display_name(), query.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        file_picker_score(left.display_name(), query.as_str())
            .cmp(&file_picker_score(right.display_name(), query.as_str()))
            .then_with(|| left.display_name().cmp(right.display_name()))
    });
    matches
}

fn file_picker_selectable_len(match_len: usize) -> usize {
    match_len.min(FILE_PICKER_LIMIT)
}

fn file_picker_entry_matches(display_name: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let display_name = display_name.to_lowercase();
    if display_name.contains(query) {
        return true;
    }

    let mut query_chars = query.chars();
    let Some(mut current_query_char) = query_chars.next() else {
        return true;
    };
    for display_char in display_name.chars() {
        if display_char == current_query_char {
            match query_chars.next() {
                Some(next_query_char) => current_query_char = next_query_char,
                None => return true,
            }
        }
    }
    false
}

fn file_picker_score(display_name: &str, query: &str) -> usize {
    if query.is_empty() {
        return display_name.len();
    }

    let display_name = display_name.to_lowercase();
    if display_name == query {
        0
    } else if display_name
        .rsplit('/')
        .next()
        .is_some_and(|file_name| file_name == query)
    {
        1
    } else if display_name.starts_with(query) {
        2
    } else if display_name.contains(query) {
        3
    } else {
        4
    }
}

#[derive(IntoElement)]
struct CommandPaletteOverlay {
    input: Entity<ui::TextInput>,
    matches: Vec<CommandPaletteEntry>,
    selected_index: usize,
}

impl CommandPaletteOverlay {
    fn new(
        input: Entity<ui::TextInput>,
        matches: Vec<CommandPaletteEntry>,
        selected_index: usize,
    ) -> Self {
        Self {
            input,
            matches,
            selected_index,
        }
    }
}

impl RenderOnce for CommandPaletteOverlay {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self.input.focus_handle(cx);
        window.focus(&focus_handle, cx);

        let mut shown_matches = Vec::new();
        for (index, entry) in self
            .matches
            .iter()
            .take(COMMAND_PALETTE_LIMIT)
            .cloned()
            .enumerate()
        {
            shown_matches.push(
                command_palette_row(index, entry, index == self.selected_index, cx)
                    .into_any_element(),
            );
        }
        let result_label = if self.matches.is_empty() {
            "一致するコマンドはありません".to_string()
        } else {
            format!("{}件", self.matches.len())
        };

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
                a: 0.42,
            })
            .flex()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(DismissCommandPalette), cx);
            })
            .child(
                div()
                    .mt(px(92.0))
                    .w(px(640.0))
                    .max_w(px(640.0))
                    .h_auto()
                    .flex()
                    .flex_col()
                    .bg(Theme::global(cx).surface())
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
                        inset: false,
                    }])
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(toolbar_border_color(cx))
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(15.0))
                                    .child(self.input),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(Theme::global(cx).text_senodary())
                                    .child(result_label),
                            ),
                    )
                    .child(
                        div()
                            .py_2()
                            .flex()
                            .flex_col()
                            .when(self.matches.is_empty(), |this| {
                                this.child(
                                    div()
                                        .px_4()
                                        .py_3()
                                        .text_sm()
                                        .text_color(Theme::global(cx).text_senodary())
                                        .child("一致するコマンドはありません"),
                                )
                            })
                            .children(shown_matches),
                    ),
            )
    }
}

fn command_palette_row(
    index: usize,
    entry: CommandPaletteEntry,
    selected: bool,
    cx: &mut App,
) -> impl IntoElement {
    let command_id = entry.id;
    let background = if selected {
        mix(
            Theme::global(cx).primary(),
            Theme::global(cx).surface(),
            0.88,
        )
    } else {
        Theme::global(cx).surface()
    };
    let title_color = if selected {
        Theme::global(cx).primary()
    } else {
        Theme::global(cx).text_primary()
    };

    div()
        .id(("command-palette-row", index))
        .mx_2()
        .px_3()
        .py_2()
        .rounded_sm()
        .bg(background)
        .cursor_pointer()
        .hover(|style| style.bg(Theme::global(cx).bg_senodary()))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(ExecuteCommandPaletteCommand { command_id }), cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .text_sm()
                        .text_color(title_color)
                        .overflow_hidden()
                        .child(entry.title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(Theme::global(cx).text_senodary())
                        .child(entry.detail),
                ),
        )
}

fn command_palette_entries() -> Vec<CommandPaletteEntry> {
    vec![
        command_palette_entry(0, "ファイルを開く", "menu::OpenFile", dispatch_open_file),
        command_palette_entry(
            1,
            "ワークスペースを開く",
            "workspace::OpenWorkspace",
            dispatch_open_workspace,
        ),
        command_palette_entry(
            2,
            "ワークスペースからファイルを検索",
            "soukou::OpenFilePicker",
            dispatch_open_file_picker,
        ),
        command_palette_entry(3, "保存", "menu::SaveFile", dispatch_save_file),
        command_palette_entry(4, "txtを書き出し", "menu::ExportTxt", dispatch_export_txt),
        command_palette_entry(
            5,
            "Wordを書き出し",
            "menu::ExportWord",
            dispatch_export_word,
        ),
        command_palette_entry(
            6,
            "epubを書き出し",
            "menu::ExportEpub",
            dispatch_export_epub,
        ),
        command_palette_entry(7, "検索", "editor::OpenSearch", dispatch_open_search),
        command_palette_entry(
            8,
            "ワークスペースペインを切り替え",
            "workspace::ToggleWorkspacePane",
            dispatch_toggle_workspace_pane,
        ),
        command_palette_entry(
            9,
            "太字を適用",
            "editor::ApplyRichTextBold",
            dispatch_apply_rich_text_bold,
        ),
        command_palette_entry(
            10,
            "強調を適用",
            "editor::ApplyRichTextEmphasis",
            dispatch_apply_rich_text_emphasis,
        ),
        command_palette_entry(
            11,
            "見出しを適用",
            "editor::ApplyRichTextHeading",
            dispatch_apply_rich_text_heading,
        ),
        command_palette_entry(
            12,
            "縦中横を適用",
            "editor::ApplyRichTextRotated",
            dispatch_apply_rich_text_rotated,
        ),
        command_palette_entry(
            13,
            "現在の列の左に改ページ",
            "editor::SetPageBreakLeftOfCurrentColumn",
            dispatch_set_page_break_left,
        ),
        command_palette_entry(
            14,
            "現在の列の右に改ページ",
            "editor::SetPageBreakRightOfCurrentColumn",
            dispatch_set_page_break_right,
        ),
        command_palette_entry(
            15,
            "現在の列の改ページを削除",
            "editor::RemovePageBreakCurrentColumn",
            dispatch_remove_page_break,
        ),
        command_palette_entry(
            16,
            "設定を開く",
            "menu::OpenSettings",
            dispatch_open_settings,
        ),
        command_palette_entry(
            17,
            "更新を確認",
            "menu::CheckForUpdates",
            dispatch_check_for_updates,
        ),
        command_palette_entry(
            18,
            "会員登録 / アカウント",
            "menu::RegisterAccount",
            dispatch_register_account,
        ),
        command_palette_entry(19, "サインアウト", "menu::SignOut", dispatch_sign_out),
        command_palette_entry(20, "終了", "menu::Quit", dispatch_quit),
    ]
}

fn command_palette_entry(
    id: usize,
    title: &'static str,
    detail: &'static str,
    dispatch: fn(&mut Window, &mut Context<SoukouApp>),
) -> CommandPaletteEntry {
    CommandPaletteEntry {
        id,
        title,
        detail,
        dispatch,
    }
}

fn dispatch_open_file(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::OpenFile), cx);
}

fn dispatch_open_workspace(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(OpenWorkspace), cx);
}

fn dispatch_open_file_picker(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(OpenFilePicker), cx);
}

fn dispatch_save_file(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::SaveFile), cx);
}

fn dispatch_export_txt(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::ExportTxt), cx);
}

fn dispatch_export_word(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::ExportWord), cx);
}

fn dispatch_export_epub(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::ExportEpub), cx);
}

fn dispatch_open_search(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(OpenSearch), cx);
}

fn dispatch_toggle_workspace_pane(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(ToggleWorkspacePane), cx);
}

fn dispatch_apply_rich_text_bold(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(ApplyRichTextBold), cx);
}

fn dispatch_apply_rich_text_emphasis(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(ApplyRichTextEmphasis), cx);
}

fn dispatch_apply_rich_text_heading(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(ApplyRichTextHeading), cx);
}

fn dispatch_apply_rich_text_rotated(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(ApplyRichTextRotated), cx);
}

fn dispatch_set_page_break_left(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(SetPageBreakLeftOfCurrentColumn), cx);
}

fn dispatch_set_page_break_right(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(SetPageBreakRightOfCurrentColumn), cx);
}

fn dispatch_remove_page_break(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(RemovePageBreakCurrentColumn), cx);
}

fn dispatch_open_settings(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::OpenSettings), cx);
}

fn dispatch_check_for_updates(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::CheckForUpdates), cx);
}

fn dispatch_register_account(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(RegisterAccount), cx);
}

fn dispatch_sign_out(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(SignOut), cx);
}

fn dispatch_quit(window: &mut Window, cx: &mut Context<SoukouApp>) {
    window.dispatch_action(Box::new(menu::Quit), cx);
}

fn filter_command_palette_entries(
    entries: &[CommandPaletteEntry],
    query: &str,
) -> Vec<CommandPaletteEntry> {
    let query = query.trim().to_lowercase();
    let mut matches = entries
        .iter()
        .filter(|entry| {
            file_picker_entry_matches(entry.title, query.as_str())
                || file_picker_entry_matches(entry.detail, query.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        command_palette_score(left, query.as_str())
            .cmp(&command_palette_score(right, query.as_str()))
            .then_with(|| left.title.cmp(right.title))
    });
    matches
}

fn command_palette_selectable_len(match_len: usize) -> usize {
    match_len.min(COMMAND_PALETTE_LIMIT)
}

fn command_palette_score(entry: &CommandPaletteEntry, query: &str) -> usize {
    if query.is_empty() {
        return entry.id;
    }

    let title = entry.title.to_lowercase();
    let detail = entry.detail.to_lowercase();
    if title == query || detail == query {
        0
    } else if title.starts_with(query) || detail.starts_with(query) {
        1
    } else if title.contains(query) || detail.contains(query) {
        2
    } else {
        3
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

    fn show_epub_meta_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let existing_epub_meta = self
            .active_document
            .path()
            .and_then(|path| {
                rich_text::load_meta_for_text_path(path)
                    .ok()
                    .map(|meta| meta.epub)
            })
            .unwrap_or_default();

        let default_title = if existing_epub_meta.title.is_empty() {
            self.export_base_name(cx)
        } else {
            existing_epub_meta.title.clone()
        };

        let title_input = cx.new(ui::TextInput::new);
        title_input.update(cx, |input, cx| {
            input.set_placeholder("タイトル", cx);
            input.set_text(default_title.as_str(), cx);
        });

        let author_input = cx.new(ui::TextInput::new);
        author_input.update(cx, |input, cx| {
            input.set_placeholder("著者名", cx);
            input.set_text(existing_epub_meta.author.as_str(), cx);
        });

        let title_focus = title_input.focus_handle(cx);
        window.focus(&title_focus, cx);

        self.epub_meta_form = Some(EpubMetaFormState {
            title_input,
            author_input,
        });
        cx.notify();
    }

    fn confirm_epub_meta(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.epub_meta_form.take() else {
            return;
        };
        let title = form.title_input.read(cx).text();
        let author = form.author_input.read(cx).text();

        let mut rich_text_meta = self.editor_controller.read(cx).rich_text_meta(cx);
        rich_text_meta.epub = EpubBookMeta { title, author };
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_rich_text_meta(rich_text_meta, cx);
        });

        cx.notify();

        MenuActionHandler::export_epub(self, window, cx);
    }

    fn confirm_epub_meta_action(
        &mut self,
        _: &ConfirmEpubMeta,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_epub_meta(window, cx);
    }

    fn dismiss_epub_meta_form(&mut self, cx: &mut Context<Self>) {
        self.epub_meta_form = None;
        cx.notify();
    }

    fn dismiss_epub_meta_form_action(
        &mut self,
        _: &DismissEpubMetaForm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_epub_meta_form(cx);
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

pub(crate) fn toolbar_border_color(cx: &App) -> gpui::Hsla {
    mix(
        Theme::global(cx).text_primary(),
        Theme::global(cx).surface(),
        0.72,
    )
    .into()
}

pub(super) fn mix(left: gpui::Rgba, right: gpui::Rgba, ratio: f32) -> gpui::Rgba {
    let ratio = ratio.clamp(0.0, 1.0);
    let inv = 1.0 - ratio;
    gpui::Rgba {
        r: left.r * inv + right.r * ratio,
        g: left.g * inv + right.g * ratio,
        b: left.b * inv + right.b * ratio,
        a: left.a * inv + right.a * ratio,
    }
}

fn read_document_to_string(path: PathBuf) -> Result<(PathBuf, String), DocumentError> {
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok((path, text)),
        Err(source) => Err(DocumentError::OpenFailed { path, source }),
    }
}

fn read_plain_document_assets(
    path: PathBuf,
) -> Result<(PathBuf, String, RichTextDocumentMeta), DocumentError> {
    let (path, text) = read_document_to_string(path)?;
    let rich_text_meta = rich_text::load_meta_for_text_path(path.as_path()).map_err(|source| {
        DocumentError::MetadataOpenFailed {
            path: path.clone(),
            source,
        }
    })?;
    Ok((path, text, rich_text_meta))
}

// TODO 設定の値で rich_text_meta を保存するかどうかを決めるようにする
fn write_plain_document_assets(
    path: PathBuf,
    contents: String,
    rich_text_meta: RichTextDocumentMeta,
    save_rich_text_meta: bool,
) -> Result<PathBuf, DocumentError> {
    std::fs::write(&path, contents.as_bytes()).map_err(|source| DocumentError::SaveFailed {
        path: path.clone(),
        source,
    })?;
    if save_rich_text_meta {
        rich_text::save_meta_for_text_path(path.as_path(), &rich_text_meta).map_err(|source| {
            DocumentError::MetadataSaveFailed {
                path: path.clone(),
                source,
            }
        })?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use rich_text::{RichTextDocumentMeta, RichTextKind};

    use super::write_plain_document_assets;

    fn unique_temp_dir(test_name: &str) -> Result<std::path::PathBuf, Box<dyn Error>> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("soukou-{test_name}-{timestamp}"));
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn cleanup_temp_dir(path: &std::path::Path) -> Result<(), Box<dyn Error>> {
        match fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Box::new(error)),
        }
    }

    #[test]
    fn saving_plain_document_writes_rich_text_meta_when_enabled() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("writes-meta")?;
        let text_path = dir.join("draft.txt");
        let meta_path = rich_text::meta_path_for_text_path(&text_path);
        let mut rich_text_meta = RichTextDocumentMeta::default();
        rich_text_meta.add_mark(0..2, RichTextKind::Bold);

        write_plain_document_assets(text_path.clone(), "本文".to_string(), rich_text_meta, true)?;

        assert_eq!(fs::read_to_string(&text_path)?, "本文");
        assert!(meta_path.exists());

        cleanup_temp_dir(&dir)?;
        Ok(())
    }

    #[test]
    fn saving_plain_document_does_not_write_meta_when_disabled() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("skips-meta")?;
        let text_path = dir.join("draft.txt");
        let meta_path = rich_text::meta_path_for_text_path(&text_path);

        write_plain_document_assets(
            text_path.clone(),
            "本文".to_string(),
            RichTextDocumentMeta::default(),
            false,
        )?;

        assert_eq!(fs::read_to_string(&text_path)?, "本文");
        assert!(!meta_path.exists());

        cleanup_temp_dir(&dir)?;
        Ok(())
    }

    #[test]
    fn saving_plain_document_writes_empty_meta_when_enabled() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("writes-empty-meta")?;
        let text_path = dir.join("draft.txt");
        let meta_path = rich_text::meta_path_for_text_path(&text_path);

        write_plain_document_assets(
            text_path.clone(),
            "本文".to_string(),
            RichTextDocumentMeta::default(),
            true,
        )?;

        assert_eq!(fs::read_to_string(&text_path)?, "本文");
        assert!(meta_path.exists());
        assert_eq!(
            rich_text::load_meta_for_text_path(&text_path)?,
            RichTextDocumentMeta::default()
        );

        cleanup_temp_dir(&dir)?;
        Ok(())
    }
}
