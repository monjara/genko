mod font;

use std::path::{Path, PathBuf};

use bottom_bar::BottomBar;
use editor::Vim;
use gpui::{
    App, AppContext, Bounds, Context, Decorations, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, Menu, MenuItem, ParentElement, PathPromptOptions,
    PromptLevel, Render, Styled, Subscription, Window, WindowBounds, WindowDecorations,
    WindowOptions, actions, div, prelude::FluentBuilder, px, size, transparent_black,
};
use settings::open_settings_window;
use theme::{APP_FONT_FAMILY, Theme};
use title_bar::TitleBar;
use workspace::{
    Event as WorkspaceEvent, OpenWorkspaceFile, OpenWorkspaceFolder, ToggleWorkspacePane,
    WORKSPACE_PANE_WIDTH, Workspace, scan_workspace_entries,
};

actions!(genko, [OpenSettings, OpenFile, OpenFolder, SaveFile, Quit]);

struct GenkoApp {
    vim: Entity<Vim>,
    workspace: Entity<Workspace>,
    title_bar: Entity<TitleBar>,
    bottom_bar: Entity<BottomBar>,
    window_handle: Option<gpui::AnyWindowHandle>,
    _subscriptions: Vec<Subscription>,
}

impl GenkoApp {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("ctrl-b", ToggleWorkspacePane, None),
            KeyBinding::new("ctrl-,", OpenSettings, None),
            KeyBinding::new("cmd-o", OpenFile, None),
            KeyBinding::new("ctrl-o", OpenFile, None),
            KeyBinding::new("cmd-shift-o", OpenFolder, None),
            KeyBinding::new("ctrl-shift-o", OpenFolder, None),
            KeyBinding::new("cmd-s", SaveFile, None),
            KeyBinding::new("ctrl-s", SaveFile, None),
        ]);

        let vim = cx.new(Vim::new);
        let workspace = cx.new(Workspace::new);
        let subscriptions = vec![cx.subscribe(&workspace, |this, _, event, cx| {
            let WorkspaceEvent::OpenPath(path) = event;
            if let Some(window_handle) = this.window_handle {
                this.open_document_path(path.clone(), true, window_handle, cx);
            }
        })];
        let title_bar = cx.new(|cx| TitleBar::new("Genko", cx));
        let bottom_bar = cx.new(BottomBar::new);

        Self {
            vim,
            workspace,
            title_bar,
            bottom_bar,
            _subscriptions: subscriptions,
            window_handle: None,
        }
    }

    fn window_title(&self, cx: &App) -> String {
        match self.workspace.read(cx).active_file() {
            Some(path) => format!("Genko - {}", path.display()),
            None => "Genko".to_string(),
        }
    }

    fn sync_window_title(&self, window: &mut Window, cx: &App) {
        window.set_window_title(&self.window_title(cx));
    }

    // TODO Future
    fn show_error(window: &mut Window, message: &str, detail: String, cx: &mut App) {
        let _ = window.prompt(
            PromptLevel::Warning,
            message,
            Some(detail.as_str()),
            &["OK"],
            cx,
        );
    }

    fn load_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.vim.update(cx, |vim, cx| vim.load_text(text, cx));
        self.workspace
            .update(cx, |workspace, cx| workspace.open_file(path, cx));
    }

    fn open_standalone_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.vim.update(cx, |vim, cx| vim.load_text(text, cx));
        self.workspace.update(cx, |workspace, cx| {
            workspace.open_file_without_root(path, cx)
        });
    }

    fn save_document_to_path(
        &mut self,
        path: PathBuf,
        contents: String,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    std::fs::write(&path, contents.as_bytes()).map(|_| path)
                })
                .await;

            match result {
                Ok(path) => {
                    let _ = this.update(cx, |this, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.open_file(path, cx));
                    });
                }
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, "ファイルを保存できませんでした", detail, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn open_document_path(
        &mut self,
        path: PathBuf,
        preserve_workspace: bool,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move {
                    std::fs::read_to_string(&path_for_read).map(|text| (path_for_read, text))
                })
                .await;

            match result {
                Ok((path, text)) => {
                    let _ = this.update(cx, |this, cx| {
                        if preserve_workspace {
                            this.load_document(path, &text, cx);
                        } else {
                            this.open_standalone_document(path, &text, cx);
                        }
                    });
                }
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, "ファイルを開けませんでした", detail, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn open_workspace_root(
        &mut self,
        root_dir: PathBuf,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let root_for_scan = root_dir.clone();
            let result = cx
                .background_spawn(async move {
                    scan_workspace_entries(root_for_scan.as_path())
                        .map(|entries| (root_for_scan, entries))
                })
                .await;

            match result {
                Ok((root_dir, entries)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.open_root(root_dir, entries, cx)
                        });
                    });
                }
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, "フォルダを開けませんでした", detail, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("開く".into()),
        });
        let window_handle = window.window_handle();

        cx.spawn(async move |this, cx| {
            let Ok(result) = picker.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(Some(mut paths)) => paths.pop(),
                Ok(None) => None,
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, "ファイル選択を開けませんでした", detail, cx);
                    });
                    None
                }
            }) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.open_document_path(path, false, window_handle, cx);
            });
        })
        .detach();
    }

    fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("フォルダを開く".into()),
        });
        let window_handle = window.window_handle();

        cx.spawn(async move |this, cx| {
            let Ok(result) = picker.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(Some(mut paths)) => paths.pop(),
                Ok(None) => None,
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, "フォルダ選択を開けませんでした", detail, cx);
                    });
                    None
                }
            }) else {
                return;
            };

            let _ = this.update(cx, |this, cx| {
                this.open_workspace_root(path, window_handle, cx);
            });
        })
        .detach();
    }

    fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let contents = self.vim.read(cx).snapshot_text(cx);
        let window_handle = window.window_handle();

        if let Some(path) = self.workspace.read(cx).active_file().map(Path::to_path_buf) {
            self.save_document_to_path(path, contents, window_handle, cx);
            return;
        }

        let initial_directory = self
            .workspace
            .read(cx)
            .suggested_save_directory()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested_name = self
            .workspace
            .read(cx)
            .suggested_file_name()
            .unwrap_or("untitled.txt")
            .to_string();
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));

        cx.spawn(async move |this, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    let detail = error.to_string();
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        Self::show_error(window, "保存先を選択できませんでした", detail, cx);
                    });
                    None
                }
            }) else {
                return;
            };

            let _ = this.update(cx, |this, cx| {
                this.save_document_to_path(path, contents, window_handle, cx);
            });
        })
        .detach();
    }

    fn open_file_action(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        self.open_file(window, cx);
    }

    fn open_folder_action(&mut self, _: &OpenFolder, window: &mut Window, cx: &mut Context<Self>) {
        self.open_folder(window, cx);
    }

    fn open_workspace_file_action(
        &mut self,
        _: &OpenWorkspaceFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file(window, cx);
    }

    fn open_workspace_folder_action(
        &mut self,
        _: &OpenWorkspaceFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_folder(window, cx);
    }

    fn save_file_action(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        self.save_file(window, cx);
    }

    fn toggle_workspace_pane_action(
        &mut self,
        _: &ToggleWorkspacePane,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace
            .update(cx, |workspace, cx| workspace.toggle_pane(cx));
    }
}

impl Render for GenkoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        title_bar::sync_client_window_inset(window);
        self.window_handle = Some(window.window_handle());
        self.sync_window_title(window, cx);
        let bar_height = title_bar::platform_title_bar_height(window);
        let mut editor_viewport_size = window.viewport_size();
        if self.workspace.read(cx).is_pane_visible() {
            editor_viewport_size.width -= px(WORKSPACE_PANE_WIDTH);
        }
        editor_viewport_size.height -= bar_height * 2.0;
        self.vim.update(cx, |vim, cx| {
            vim.update_viewport_size(editor_viewport_size, cx);
        });

        let content = self.vim.clone().into_element();

        div()
            .size_full()
            .bg(transparent_black())
            .map(|this| match window.window_decorations() {
                Decorations::Server => this,
                Decorations::Client { tiling } => this
                    .when(!tiling.top, |this| {
                        this.pt(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.bottom, |this| {
                        this.pb(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.left, |this| {
                        this.pl(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.right, |this| {
                        this.pr(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    }),
            })
            .child(
                div()
                    .size_full()
                    .bg(Theme::global(cx).white())
                    .font_family(APP_FONT_FAMILY)
                    .flex()
                    .flex_col()
                    .items_center()
                    .overflow_hidden()
                    .map(|this| match window.window_decorations() {
                        Decorations::Server => this,
                        Decorations::Client { tiling } => this
                            .when(!(tiling.top || tiling.right), |this| {
                                this.rounded_tr(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.top || tiling.left), |this| {
                                this.rounded_tl(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.right), |this| {
                                this.rounded_br(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.left), |this| {
                                this.rounded_bl(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!tiling.is_tiled(), |this| {
                                this.shadow(title_bar::client_window_shadow())
                            }),
                    })
                    .on_action(cx.listener(Self::open_file_action))
                    .on_action(cx.listener(Self::open_folder_action))
                    .on_action(cx.listener(Self::open_workspace_file_action))
                    .on_action(cx.listener(Self::open_workspace_folder_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::toggle_workspace_pane_action))
                    .child(self.title_bar.clone().into_element())
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .flex()
                            .when(self.workspace.read(cx).is_pane_visible(), |this| {
                                this.child(self.workspace.clone().into_element())
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(content),
                            ),
                    )
                    .child(self.bottom_bar.clone().into_element()),
            )
    }
}

impl Focusable for GenkoApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.vim.focus_handle(cx)
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        font::init(cx);
        theme::init(cx);
        settings::init(cx);
        editor::init(cx);

        cx.on_action(|_: &Quit, cx| cx.quit())
            .on_action(|_: &OpenSettings, cx| open_settings_window(cx))
            .set_menus(vec![
                Menu {
                    disabled: false,
                    name: "Genko".into(),
                    items: vec![
                        MenuItem::action("設定", OpenSettings),
                        MenuItem::separator(),
                        MenuItem::action("終了", Quit),
                    ],
                },
                Menu {
                    disabled: false,
                    name: "ファイル".into(),
                    items: vec![
                        MenuItem::action("開く", OpenFile),
                        MenuItem::action("フォルダを開く", OpenFolder),
                        MenuItem::action("保存", SaveFile),
                    ],
                },
            ]);

        cx.open_window(
            title_bar::configure_window_options(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.0), px(800.0)),
                    cx,
                ))),
                app_id: Some("dev.genko".into()),
                is_movable: true,
                is_resizable: true,
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            }),
            |_, cx| cx.new(GenkoApp::new),
        )
        .and_then(|window| {
            window.update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.activate(true);
            })
        })
        .expect("Failed to open main window")
    })
}
