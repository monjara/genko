mod app;
mod font;
mod notification;

use app::SoukouApp;
use futures::StreamExt;
use gpui::{
    App, AppContext, AsyncApp, Bounds, Focusable, WindowBounds, WindowDecorations, WindowOptions,
    actions, px, size,
};
use menu::{OpenSettings, Quit};
use settings::open_settings_window;

const APP_ID: &str = "dev.monj.soukou";
const MAIN_WINDOW_WIDTH: f32 = 1200.0;
const MAIN_WINDOW_HEIGHT: f32 = 800.0;

actions!(
    soukou,
    [
        DismissActiveModal,
        OpenModalPrimary,
        CancelRubyEditor,
        ConfirmEpubMeta,
        DismissEpubMetaForm,
        DismissPageBreakMenu,
    ]
);

fn main() {
    if std::env::var("SOUKOU_DEVELOPMENT_MODE").is_ok() {
        let _ = dotenvy::dotenv();
    }

    let application = gpui_platform::application();

    let (open_url_sender, open_url_receiver) = futures::channel::mpsc::unbounded::<Vec<String>>();
    application.on_open_urls(move |urls| {
        if let Err(error) = open_url_sender.unbounded_send(urls) {
            eprintln!("failed to receive auth callback url: {error}");
        }
    });

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

        let callback_prefix = format!("{}://", auth::AuthConfig::from_env().callback_scheme());
        let startup_urls = std::env::args()
            .skip(1)
            .filter(|argument| argument.starts_with(callback_prefix.as_str()))
            .collect::<Vec<_>>();
        if !startup_urls.is_empty()
            && let Err(error) = main_window.update(cx, |this, _, cx| {
                this.handle_open_urls(startup_urls, cx);
            })
        {
            eprintln!("failed to handle startup auth callback url: {error}");
        }

        let mut open_url_receiver = open_url_receiver;
        cx.spawn(move |cx: &mut AsyncApp| {
            let mut app = cx.clone();
            async move {
                while let Some(urls) = open_url_receiver.next().await {
                    if let Err(error) = main_window.update(&mut app, |this, _, cx| {
                        this.handle_open_urls(urls, cx);
                    }) {
                        eprintln!("failed to handle auth callback url: {error}");
                    }
                }
            }
        })
        .detach();
    })
}
