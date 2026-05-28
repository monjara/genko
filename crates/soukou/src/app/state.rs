#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::process::Command;

use bottom_bar::BottomBar;
use editor::{
    EditorController, RequestProForRichText, ToggleBold, ToggleStrikethrough, VimCommandQuit,
    VimCommandWrite,
};
use gpui::{
    App, AppContext, Context, ExternalPaths, FocusHandle, Focusable, KeyBinding, MouseDownEvent,
    Window,
};
use settings::AppSettings;
use title_bar::{TitleBar, TitleBarMenu};
use ui::{MenuBarItem, MenuBarMenu};

use crate::{
    CheckForUpdates, ExportEpub, ExportTxt, ExportWord, OpenFile, OpenSettings, Quit, SaveFile,
    app::{
        APP_NAME, AppModal, CHECK_FOR_UPDATES_MENU_LABEL, EXPORT_EPUB_MENU_LABEL,
        EXPORT_TXT_MENU_LABEL, EXPORT_WORD_MENU_LABEL, FILE_MENU_LABEL, OPEN_PROMPT_LABEL,
        QUIT_MENU_LABEL, SAVE_MENU_LABEL, SETTINGS_MENU_LABEL, SoukouApp, WINDOW_TITLE_SEPARATOR,
    },
    document::{ActiveDocument, DocumentKind, ExportFormat},
};

impl SoukouApp {
    pub(super) fn open_external_url(
        &mut self,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn show_error_modal(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.active_modal = Some(AppModal::Error {
            title: title.to_string(),
            detail,
        });
        cx.notify();
    }

    pub(super) fn show_info_modal(&mut self, title: &str, detail: String, cx: &mut Context<Self>) {
        self.active_modal = Some(AppModal::Info {
            title: title.to_string(),
            detail,
        });
        cx.notify();
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let quit_mac = AppSettings::global(cx).keymap_keystroke("app.quit.mac");
        let open_file_mac = AppSettings::global(cx).keymap_keystroke("app.open_file.mac");
        let save_file_mac = AppSettings::global(cx).keymap_keystroke("app.save_file.mac");
        let toggle_bold_mac = AppSettings::global(cx).keymap_keystroke("app.toggle_bold.mac");
        let toggle_strikethrough_mac =
            AppSettings::global(cx).keymap_keystroke("app.toggle_strikethrough.mac");

        let open_settings_ctrl = AppSettings::global(cx).keymap_keystroke("app.open_settings.ctrl");
        let open_file_ctrl = AppSettings::global(cx).keymap_keystroke("app.open_file.ctrl");
        let save_file_ctrl = AppSettings::global(cx).keymap_keystroke("app.save_file.ctrl");
        let toggle_bold_ctrl = AppSettings::global(cx).keymap_keystroke("app.toggle_bold.ctrl");
        let toggle_strikethrough_ctrl =
            AppSettings::global(cx).keymap_keystroke("app.toggle_strikethrough.ctrl");

        cx.bind_keys([
            KeyBinding::new(quit_mac.as_ref(), Quit, None),
            KeyBinding::new(open_settings_ctrl.as_ref(), OpenSettings, None),
            KeyBinding::new(open_file_mac.as_ref(), OpenFile, None),
            KeyBinding::new(open_file_ctrl.as_ref(), OpenFile, None),
            KeyBinding::new(save_file_mac.as_ref(), SaveFile, None),
            KeyBinding::new(save_file_ctrl.as_ref(), SaveFile, None),
            KeyBinding::new(toggle_bold_mac.as_ref(), ToggleBold, None),
            KeyBinding::new(toggle_bold_ctrl.as_ref(), ToggleBold, None),
            KeyBinding::new(toggle_strikethrough_mac.as_ref(), ToggleStrikethrough, None),
            KeyBinding::new(
                toggle_strikethrough_ctrl.as_ref(),
                ToggleStrikethrough,
                None,
            ),
        ]);

        let editor_controller = cx.new(EditorController::new);
        let title_bar = cx.new(|cx| TitleBar::new(APP_NAME, Self::title_bar_menus(), cx));
        let bottom_bar = cx.new(BottomBar::new);

        Self {
            editor_controller,
            active_document: ActiveDocument::default(),
            active_modal: None,
            epub_metadata_form: None,
            title_bar,
            bottom_bar,
        }
    }

    fn title_bar_menus() -> Vec<TitleBarMenu> {
        vec![
            MenuBarMenu::new(
                APP_NAME,
                vec![
                    MenuBarItem::new(SETTINGS_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(OpenSettings), cx);
                    }),
                    MenuBarItem::new(CHECK_FOR_UPDATES_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(CheckForUpdates), cx);
                    }),
                    MenuBarItem::new(QUIT_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(Quit), cx);
                    }),
                ],
            ),
            MenuBarMenu::new(
                FILE_MENU_LABEL,
                vec![
                    MenuBarItem::new(OPEN_PROMPT_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(OpenFile), cx);
                    }),
                    MenuBarItem::new(SAVE_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(SaveFile), cx);
                    }),
                    MenuBarItem::new(EXPORT_TXT_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(ExportTxt), cx);
                    }),
                    MenuBarItem::new(EXPORT_WORD_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(ExportWord), cx);
                    }),
                    MenuBarItem::new(EXPORT_EPUB_MENU_LABEL, |window, cx| {
                        window.dispatch_action(Box::new(ExportEpub), cx);
                    }),
                ],
            ),
        ]
    }

    fn window_title(&self, _cx: &App) -> String {
        match self.active_document.path() {
            Some(path) => format!("{APP_NAME}{WINDOW_TITLE_SEPARATOR}{}", path.display()),
            _ => APP_NAME.to_string(),
        }
    }

    pub(super) fn sync_window_title(&self, window: &mut Window, cx: &App) {
        window.set_window_title(&self.window_title(cx));
    }

    pub(super) fn dismiss_active_modal(
        &mut self,
        _: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_modal = None;
        cx.notify();
    }

    pub(super) fn open_modal_primary_action(
        &mut self,
        _: &MouseDownEvent,
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
            _ => {}
        }
    }

    pub(super) fn open_file_action(
        &mut self,
        _: &OpenFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file(window, cx);
    }

    pub(super) fn save_file_action(
        &mut self,
        _: &SaveFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_file(window, cx);
    }

    pub(super) fn export_txt_action(
        &mut self,
        _: &ExportTxt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_txt_document(window, cx);
    }

    pub(super) fn export_word_action(
        &mut self,
        _: &ExportWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document(ExportFormat::Word, window, cx);
    }

    pub(super) fn export_epub_action(
        &mut self,
        _: &ExportEpub,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document(ExportFormat::Epub, window, cx);
    }

    pub(super) fn check_for_updates_action(
        &mut self,
        _: &CheckForUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.check_for_updates(window, cx);
    }

    pub(super) fn vim_command_write_action(
        &mut self,
        _: &VimCommandWrite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_file(window, cx);
    }

    pub(super) fn vim_command_quit_action(
        &mut self,
        _: &VimCommandQuit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.quit();
    }

    pub(super) fn drop_external_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_dropped_paths(paths, window, cx);
    }

    pub(super) fn request_pro_for_richtext_action(
        &mut self,
        _: &RequestProForRichText,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(super) fn current_document_is_richtext(&self, cx: &App) -> bool {
        self.editor_controller.read(cx).has_richtext_document(cx)
    }

    pub(super) fn current_document_kind(&self, cx: &App) -> DocumentKind {
        if self.current_document_is_richtext(cx) {
            DocumentKind::RichText
        } else {
            DocumentKind::PlainText
        }
    }
}

impl Focusable for SoukouApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor_controller.focus_handle(cx)
    }
}
