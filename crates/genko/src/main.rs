mod board;
mod settings_window;

use board::BoardElement;
use settings::AppSettings;
use settings_window::SettingsWindow;

use gpui::{
    App, Application, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Menu, MenuItem,
    ParentElement, Render, SharedString, Styled, Window, WindowBounds, WindowOptions, actions, div,
    prelude::*, px, rgb, size,
};

const DEFAULT_VISIBLE_COLUMNS: usize = 20;
const AUTOMATIC_ROWS_RESERVED_CELLS: usize = 4;
const CELL_SIZE: f32 = 28.0;

actions!(
    genko,
    [
        Backspace,
        Delete,
        Up,
        Down,
        Left,
        Right,
        SelectUp,
        SelectDown,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Enter,
        ShowCharacterPalette,
        OpenSettings,
        VimEnterInsertMode,
        VimAppend,
        VimNormalMode,
        VimVisualMode,
        VimDeleteChar,
        Quit,
    ]
);

pub(crate) struct GenkoApp {
    title: SharedString,
    board: Entity<BoardElement>,
}

impl GenkoApp {
    fn new(cx: &mut Context<Self>) -> Self {
        AppSettings::init(cx);

        let board = cx.new(BoardElement::new);
        cx.observe(&board, |_, _, cx| cx.notify()).detach();

        Self {
            title: "Genko".into(),
            board,
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let board = self.board.read(cx);
        let mode_label = board.vim_status_label(cx);
        let scroll_column = board.scroll_column();
        let visible_columns = board.visible_columns();
        let total_columns = board.total_columns();

        div()
            .w_full()
            .flex()
            .justify_between()
            .items_end()
            .text_color(rgb(0x2f241d))
            .child(
                div()
                    .text_2xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(self.title.clone()),
            )
            .child(div().text_sm().text_color(rgb(0x705a4a)).child(format!(
                "vertical{} / {} cells / columns {}-{} of {}",
                mode_label,
                board.used_cells(),
                scroll_column + 1,
                (scroll_column + visible_columns).min(total_columns),
                total_columns
            )))
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
            .bg(rgb(0xebe5d8))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .items_center()
                    .child(self.render_header(cx))
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
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("up", Up, None),
            KeyBinding::new("down", Down, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("shift-up", SelectUp, None),
            KeyBinding::new("shift-down", SelectDown, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("cmd-a", SelectAll, None),
            KeyBinding::new("ctrl-a", SelectAll, None),
            KeyBinding::new("cmd-v", Paste, None),
            KeyBinding::new("ctrl-v", Paste, None),
            KeyBinding::new("cmd-c", Copy, None),
            KeyBinding::new("ctrl-c", Copy, None),
            KeyBinding::new("cmd-x", Cut, None),
            KeyBinding::new("ctrl-x", Cut, None),
            KeyBinding::new("enter", Enter, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
            KeyBinding::new("i", VimEnterInsertMode, Some("Genko && vim_mode == normal")),
            KeyBinding::new("a", VimAppend, Some("Genko && vim_mode == normal")),
            KeyBinding::new("escape", VimNormalMode, Some("Genko && vim_mode == insert")),
            KeyBinding::new("escape", VimNormalMode, Some("Genko && vim_mode == visual")),
            KeyBinding::new("v", VimVisualMode, Some("Genko && vim_mode == normal")),
            KeyBinding::new("v", VimNormalMode, Some("Genko && vim_mode == visual")),
            KeyBinding::new("h", Left, Some("Genko && vim_mode == normal")),
            KeyBinding::new("j", Down, Some("Genko && vim_mode == normal")),
            KeyBinding::new("k", Up, Some("Genko && vim_mode == normal")),
            KeyBinding::new("l", Right, Some("Genko && vim_mode == normal")),
            KeyBinding::new("h", Left, Some("Genko && vim_mode == visual")),
            KeyBinding::new("j", Down, Some("Genko && vim_mode == visual")),
            KeyBinding::new("k", Up, Some("Genko && vim_mode == visual")),
            KeyBinding::new("l", Right, Some("Genko && vim_mode == visual")),
            KeyBinding::new("x", VimDeleteChar, Some("Genko && vim_mode == normal")),
            KeyBinding::new("x", VimDeleteChar, Some("Genko && vim_mode == visual")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
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
            name: "Genko".into(),
            items: vec![
                MenuItem::action("設定", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("終了", Quit),
            ],
        }]);

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
            })
            .unwrap();
    });
}
