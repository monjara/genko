#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::process::Command;

use bottom_bar::BottomBar;
use editor::{EditorController, VimCommandQuit, VimCommandWrite};
use gpui::{App, AppContext, Context, ExternalPaths, FocusHandle, Focusable, KeyBinding, Window};
use menu::{MenuActionHandler, OpenFile, OpenSettings, Quit, SaveFile};
use settings::AppSettings;
use title_bar::TitleBar;
use workspace::{
    Event as WorkspaceEvent, OpenWorkspace, ToggleWorkspacePane, Workspace, WorkspaceState,
};

use crate::{
    DismissActiveModal, OpenModalPrimary,
    app::{AppModal, SoukouApp, WINDOW_TITLE_SEPARATOR},
    document::{ActiveDocument, DocumentKind},
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
            active_modal: None,
            _workspace_subscription: workspace_subscription,
            title_bar,
            bottom_bar,
        }
    }

    fn window_title(&self, _cx: &App) -> String {
        match WorkspaceState::global(_cx)
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

    pub(super) fn sync_window_title(&self, window: &mut Window, cx: &App) {
        window.set_window_title(&self.window_title(cx));
    }

    pub(super) fn dismiss_active_modal_action(
        &mut self,
        _: &DismissActiveModal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_modal = None;
        cx.notify();
    }

    pub(super) fn open_modal_primary_action(
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
            _ => {}
        }
    }

    pub(super) fn open_workspace_action(
        &mut self,
        _: &OpenWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        MenuActionHandler::open_file(self, window, cx);
    }

    pub(super) fn toggle_workspace_pane_action(
        &mut self,
        _: &ToggleWorkspacePane,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).toggle_pane();
        self.workspace.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    pub(super) fn vim_command_write_action(
        &mut self,
        _: &VimCommandWrite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        MenuActionHandler::save_file(self, window, cx);
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

    pub(super) fn current_document_kind(&self, _cx: &App) -> DocumentKind {
        DocumentKind::PlainText
    }

    pub(super) fn workspace_pane_visible(&self, cx: &App) -> bool {
        WorkspaceState::global(cx).is_pane_visible()
    }
}

impl Focusable for SoukouApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor_controller.focus_handle(cx)
    }
}
