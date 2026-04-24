mod board;
mod color;
mod settings_window;

use board::BoardElement;
use settings::AppSettings;
use settings_window::SettingsWindow;

use crate::color::APP_BACKGROUND;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Menu, MenuItem,
    ParentElement, Render, Styled, Window, WindowBounds, WindowOptions, actions, div, prelude::*,
    px, rgb, size,
};

const DEFAULT_VISIBLE_COLUMNS: usize = 20;
const AUTOMATIC_ROWS_RESERVED_CELLS: usize = 4;
const CELL_SIZE: f32 = 28.0;
const RUBY_GUTTER_SIZE: f32 = 10.0;

actions!(genko, [OpenSettings, Quit]);

pub(crate) struct GenkoApp {
    board: Entity<BoardElement>,
}

impl GenkoApp {
    fn new(cx: &mut Context<Self>) -> Self {
        AppSettings::init(cx);

        let board = cx.new(BoardElement::new);
        cx.observe(&board, |_, _, cx| cx.notify()).detach();

        Self { board }
    }
}

impl Render for GenkoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_size = window.viewport_size();
        self.board.update(cx, |board, cx| {
            board.update_viewport_size(viewport_size, cx);
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
                    .child(self.board.clone()),
            )
    }
}

impl Focusable for GenkoApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.board.focus_handle(cx)
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
        BoardElement::bind_keys(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());

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

        cx.on_action(move |_: &OpenSettings, cx| open_settings_window(cx));
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
