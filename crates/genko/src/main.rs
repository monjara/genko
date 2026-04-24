mod settings_window;

use editor::Editor;
use settings_window::SettingsWindow;

use theme::APP_BACKGROUND;

use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, IntoElement, KeyBinding,
    Menu, MenuItem, ParentElement, Render, Styled, Window, WindowBounds, WindowOptions, actions,
    div, px, rgb, size,
};

actions!(genko, [OpenSettings, Quit]);

pub(crate) struct GenkoApp {
    editor: Entity<Editor>,
}

impl GenkoApp {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.bind_keys([KeyBinding::new("ctrl-,", OpenSettings, None)]);

        let editor = cx.new(Editor::new);
        cx.observe(&editor, |_, _, cx| cx.notify()).detach();

        Self { editor }
    }
}

impl Render for GenkoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_size = window.viewport_size();
        self.editor.update(cx, |editor, cx| {
            editor.update_viewport_size(viewport_size, cx);
        });

        div()
            .size_full()
            .bg(rgb(APP_BACKGROUND))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .items_center()
                    .child(self.editor.clone()),
            )
    }
}

impl Focusable for GenkoApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

fn open_settings_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(520.0), px(460.0)), cx);

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
        settings::init(cx);
        Editor::bind_keys(cx);

        let bounds = Bounds::centered(None, size(px(760.0), px(760.0)), cx);

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

        cx.set_menus(vec![Menu {
            disabled: false,
            name: "Genko".into(),
            items: vec![
                MenuItem::action("設定", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("終了", Quit),
            ],
        }]);

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.activate(true);
            })
            .unwrap();
    });
}
