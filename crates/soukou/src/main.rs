mod app;
mod font;

use app::SoukouApp;
use gpui::{
    App, AppContext, Bounds, Focusable, WindowBounds, WindowDecorations, WindowOptions, actions,
    px, size,
};
use menu::{OpenSettings, Quit};
use settings::open_settings_window;

const APP_ID: &str = "dev.monj.soukou";
const MAIN_WINDOW_WIDTH: f32 = 1200.0;
const MAIN_WINDOW_HEIGHT: f32 = 800.0;

actions!(
    soukou,
    [DismissActiveModal, OpenModalPrimary, CancelRubyEditor]
);

fn main() {
    if std::env::var("SOUKOU_DEVELOPMENT_MODE").is_ok() {
        let _ = dotenvy::dotenv();
    }

    let application = gpui_platform::application();

    application.run(move |cx: &mut App| {
        font::init(cx);
        theme::init(cx);
        settings::init(cx);
        workspace::WorkspaceState::init(cx);
        editor::init(cx);
        menu::init(cx);
        ui::init(cx);

        cx.on_action(|_: &Quit, cx| cx.quit())
            .on_action(|_: &OpenSettings, cx| open_settings_window(cx));

        let main_window = cx
            .open_window(
                title_bar::configure_window_options(WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(MAIN_WINDOW_WIDTH), px(MAIN_WINDOW_HEIGHT)),
                        cx,
                    ))),
                    app_id: Some(APP_ID.into()),
                    is_movable: true,
                    is_resizable: true,
                    window_decorations: Some(WindowDecorations::Client),
                    ..Default::default()
                }),
                move |_, cx| cx.new(SoukouApp::new),
            )
            .expect("Failed to open main window");

        main_window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.activate(true);
            })
            .expect("Failed to focus main window");
    })
}
