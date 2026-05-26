mod app;
mod auth;
mod document;
mod font;
mod menu;

use app::{APP_ID, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH, SoukouApp};
use futures::StreamExt;
use gpui::{
    App, AppContext, AsyncApp, Bounds, Focusable, WindowBounds, WindowDecorations, WindowOptions,
    actions, px, size,
};
use settings::open_settings_window;

actions!(
    soukou,
    [
        OpenSettings,
        CheckForUpdates,
        OpenFile,
        SaveFile,
        ExportTxt,
        ExportWord,
        ExportEpub,
        Quit,
        SignIn,
        OpenAccountSettings,
        SignOut
    ]
);

fn main() {
    if env::development_mode() {
        let _ = dotenvy::dotenv();
    }

    let (open_url_tx, open_url_rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
    let application = gpui_platform::application();
    application.on_open_urls(move |urls| {
        let _ = open_url_tx.unbounded_send(urls);
    });

    application.run(move |cx: &mut App| {
        font::init(cx);
        theme::init(cx);
        settings::init(cx);
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

        let mut open_url_rx = open_url_rx;

        cx.spawn(move |cx: &mut AsyncApp| {
            let mut app = cx.clone();
            async move {
                while let Some(urls) = open_url_rx.next().await {
                    let _ = main_window.update(&mut app, |this, _, cx| {
                        this.handle_open_urls(urls, cx);
                    });
                }
            }
        })
        .detach();

        let callback_prefix = format!("{}://", auth::AuthConfig::from_env().callback_scheme());
        let startup_urls = std::env::args()
            .skip(1)
            .filter(|arg| arg.starts_with(callback_prefix.as_str()))
            .collect::<Vec<_>>();
        if !startup_urls.is_empty() {
            let _ = main_window.update(cx, |this, _, cx| {
                this.handle_open_urls(startup_urls, cx);
            });
        }
    })
}
