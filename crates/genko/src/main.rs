mod settings_window;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use editor::Editor;
use settings::AppSettings;
use settings_window::SettingsWindow;
use vim::Vim;

use theme::{APP_BACKGROUND, APP_FONT_FAMILY};

use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, Menu, MenuItem, ParentElement, PathPromptOptions, PromptLevel, Render,
    Styled, Window, WindowBounds, WindowOptions, actions, div, px, rgb, size,
};

actions!(genko, [OpenSettings, OpenFile, SaveFile, Quit]);

pub(crate) struct GenkoApp {
    editor: Entity<Editor>,
    vim: Entity<Vim>,
    vim_mode_status: Entity<VimModeStatus>,
    current_path: Option<PathBuf>,
    last_viewport_size: Option<gpui::Size<gpui::Pixels>>,
    last_vim_mode_enabled: Option<bool>,
}

struct VimModeStatus {
    vim: Entity<Vim>,
}

impl VimModeStatus {
    fn new(vim: Entity<Vim>, cx: &mut Context<Self>) -> Self {
        cx.observe(&vim, |_, _, cx| cx.notify()).detach();
        Self { vim }
    }
}

impl Render for VimModeStatus {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .right_auto()
            .py_1()
            .text_color(rgb(0x2D2416))
            .border_1()
            .rounded_sm()
            .child(self.vim.read(cx).mode_label())
    }
}

impl GenkoApp {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.bind_keys([KeyBinding::new("ctrl-,", OpenSettings, None)]);
        cx.bind_keys([
            KeyBinding::new("cmd-o", OpenFile, None),
            KeyBinding::new("ctrl-o", OpenFile, None),
            KeyBinding::new("cmd-s", SaveFile, None),
            KeyBinding::new("ctrl-s", SaveFile, None),
        ]);

        let editor = cx.new(Editor::new);
        let vim = cx.new(|_| Vim::new(editor.clone()));
        let vim_mode_status = cx.new(|cx| VimModeStatus::new(vim.clone(), cx));

        Self {
            editor,
            vim,
            vim_mode_status,
            current_path: None,
            last_viewport_size: None,
            last_vim_mode_enabled: None,
        }
    }

    fn window_title(&self) -> String {
        match &self.current_path {
            Some(path) => format!("Genko - {}", path.display()),
            None => "Genko".to_string(),
        }
    }

    fn sync_window_title(&self, window: &mut Window) {
        window.set_window_title(&self.window_title());
    }

    fn show_error(window: &mut Window, message: &str, detail: String, cx: &mut App) {
        let _ = window.prompt(
            PromptLevel::Warning,
            message,
            Some(detail.as_str()),
            &["OK"],
            cx,
        );
    }

    fn load_document(&mut self, path: PathBuf, text: String, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.load_text(&text, cx));
        self.current_path = Some(path);
        cx.notify();
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
                        this.current_path = Some(path);
                        cx.notify();
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

            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move {
                    std::fs::read_to_string(&path_for_read).map(|text| (path_for_read, text))
                })
                .await;

            match result {
                Ok((path, text)) => {
                    let _ = this.update(cx, |this, cx| this.load_document(path, text, cx));
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

    fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let contents = self.editor.read(cx).snapshot_text();
        let window_handle = window.window_handle();

        if let Some(path) = self.current_path.clone() {
            self.save_document_to_path(path, contents, window_handle, cx);
            return;
        }

        let initial_directory = self
            .current_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested_name = self
            .current_path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
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

    fn save_file_action(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        self.save_file(window, cx);
    }
}

impl Render for GenkoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_window_title(window);
        let viewport_size = window.viewport_size();
        let vim_mode_enabled = AppSettings::global(cx).vim_mode;
        let needs_viewport_sync = self.last_viewport_size != Some(viewport_size)
            || self.last_vim_mode_enabled != Some(vim_mode_enabled);
        if needs_viewport_sync && vim_mode_enabled {
            self.vim.update(cx, |vim, cx| {
                vim.update_viewport_size(viewport_size, cx);
            });
        } else if needs_viewport_sync {
            self.editor.update(cx, |editor, cx| {
                editor.update_viewport_size(viewport_size, cx);
                editor.set_text_input_enabled(true, cx);
            });
        }
        self.last_viewport_size = Some(viewport_size);
        self.last_vim_mode_enabled = Some(vim_mode_enabled);

        let content = div()
            .flex()
            .flex_col()
            .gap_2()
            .items_start()
            .child(if vim_mode_enabled {
                self.vim.clone().into_element()
            } else {
                self.editor.clone().into_element()
            });
        let content = if vim_mode_enabled {
            content.child(self.vim_mode_status.clone())
        } else {
            content
        };

        div()
            .size_full()
            .bg(rgb(APP_BACKGROUND))
            .font_family(APP_FONT_FAMILY)
            .flex()
            .items_center()
            .justify_center()
            .on_action(cx.listener(Self::open_file_action))
            .on_action(cx.listener(Self::save_file_action))
            .child(content)
    }
}

impl Focusable for GenkoApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if AppSettings::global(cx).vim_mode {
            self.vim.focus_handle(cx)
        } else {
            self.editor.focus_handle(cx)
        }
    }
}

fn open_settings_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);

    let settings_window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("dev.genko.settings".into()),
                ..Default::default()
            },
            move |_, cx| cx.new(move |_| SettingsWindow::new()),
        )
        .unwrap();

    settings_window
        .update(cx, |_, window, cx| {
            window.activate_window();
            cx.activate(true);
        })
        .unwrap();
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let fonts = [include_bytes!(
            "../../../assets/fonts/ZenOldMincho-Regular.ttf"
        )]
        .iter()
        .map(|bytes| Cow::Borrowed(&bytes[..]))
        .collect();
        let _ = cx.text_system().add_fonts(fonts);

        settings::init(cx);
        editor::init(cx);
        vim::init(cx);

        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    app_id: Some("dev.genko".into()),
                    ..Default::default()
                },
                |_, cx| cx.new(GenkoApp::new),
            )
            .unwrap();

        cx.on_action(|_: &Quit, cx| cx.quit())
            .on_action(|_: &OpenSettings, cx| open_settings_window(cx));

        cx.set_menus(vec![
            Menu {
                disabled: false,
                name: "ファイル".into(),
                items: vec![
                    MenuItem::action("開く", OpenFile),
                    MenuItem::action("保存", SaveFile),
                ],
            },
            Menu {
                disabled: false,
                name: "Genko".into(),
                items: vec![
                    MenuItem::action("設定", OpenSettings),
                    MenuItem::separator(),
                    MenuItem::action("終了", Quit),
                ],
            },
        ]);

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.activate(true);
            })
            .unwrap();
    });
}
