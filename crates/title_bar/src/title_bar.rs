use gpui::{
    AnyElement, App, AppContext, BoxShadow, ClickEvent, Context, Decorations, Div, Entity, Hsla,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, Rgba,
    StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowButton, WindowButtonLayout,
    WindowControlArea, WindowDecorations, div, point, prelude::FluentBuilder, px,
};
use theme::Theme;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use ui::MenuBar;

pub use ui::MenuBarMenu as TitleBarMenu;

const MAC_TRAFFIC_LIGHT_PADDING: f32 = 71.0;
const SIDE_SLOT_WIDTH: f32 = 160.0;
pub const CLIENT_SIDE_DECORATION_ROUNDING: Pixels = px(10.0);
const CLIENT_SIDE_SHADOW_SIZE: Pixels = px(10.0);

pub fn configure_window_options(mut options: gpui::WindowOptions) -> gpui::WindowOptions {
    options.titlebar = Some(TitlebarOptions {
        title: None,
        appears_transparent: cfg!(target_os = "macos"),
        traffic_light_position: cfg!(target_os = "macos").then(|| point(px(9.0), px(9.0))),
    });

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        options.window_decorations = Some(WindowDecorations::Client);
    }

    options.window_background = gpui::WindowBackgroundAppearance::Transparent;

    options
}

pub fn sync_client_window_inset(window: &mut Window) {
    let inset = match window.window_decorations() {
        Decorations::Server => px(0.0),
        Decorations::Client { tiling } if tiling.is_tiled() => px(0.0),
        Decorations::Client { .. } => CLIENT_SIDE_SHADOW_SIZE,
    };

    window.set_client_inset(inset);
}

pub fn apply_client_side_shadow_padding(container: Div, window: &Window) -> Div {
    match window.window_decorations() {
        Decorations::Server => container,
        Decorations::Client { tiling } => container
            .when(!tiling.top, |container| {
                container.pt(CLIENT_SIDE_SHADOW_SIZE)
            })
            .when(!tiling.bottom, |container| {
                container.pb(CLIENT_SIDE_SHADOW_SIZE)
            })
            .when(!tiling.left, |container| {
                container.pl(CLIENT_SIDE_SHADOW_SIZE)
            })
            .when(!tiling.right, |container| {
                container.pr(CLIENT_SIDE_SHADOW_SIZE)
            }),
    }
}

pub fn client_side_shadow_padding_size(window: &Window) -> gpui::Size<Pixels> {
    match window.window_decorations() {
        Decorations::Server => gpui::size(Pixels::ZERO, Pixels::ZERO),
        Decorations::Client { tiling } => {
            let width = if tiling.left || tiling.right {
                CLIENT_SIDE_SHADOW_SIZE
            } else {
                CLIENT_SIDE_SHADOW_SIZE * 2.0
            };
            let height = if tiling.top || tiling.bottom {
                CLIENT_SIDE_SHADOW_SIZE
            } else {
                CLIENT_SIDE_SHADOW_SIZE * 2.0
            };
            gpui::size(width, height)
        }
    }
}

pub fn apply_client_side_window_frame(container: Div, window: &Window) -> Div {
    match window.window_decorations() {
        Decorations::Server => container,
        Decorations::Client { tiling } => container
            .when(!(tiling.top || tiling.right), |container| {
                container.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING)
            })
            .when(!(tiling.top || tiling.left), |container| {
                container.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING)
            })
            .when(!(tiling.bottom || tiling.right), |container| {
                container.rounded_br(CLIENT_SIDE_DECORATION_ROUNDING)
            })
            .when(!(tiling.bottom || tiling.left), |container| {
                container.rounded_bl(CLIENT_SIDE_DECORATION_ROUNDING)
            })
            .when(!tiling.is_tiled(), |container| {
                container.shadow(client_window_shadow())
            }),
    }
}

fn client_window_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 0.4,
        },
        offset: point(px(0.0), px(0.0)),
        blur_radius: CLIENT_SIDE_SHADOW_SIZE / 2.0,
        spread_radius: px(0.0),
        inset: false,
    }]
}

pub fn platform_title_bar_height(window: &Window) -> Pixels {
    #[cfg(target_os = "windows")]
    {
        let _ = window;
        px(32.0)
    }

    #[cfg(not(target_os = "windows"))]
    {
        (1.75 * window.rem_size()).max(px(34.0))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PlatformStyle {
    Mac,
    Linux,
    Windows,
}

impl PlatformStyle {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            Self::Linux
        } else {
            Self::Windows
        }
    }
}

pub struct TitleBar {
    account_control: Option<Entity<auth::TitleBarAccountControl>>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    menu_bar: Entity<MenuBar>,
}

impl TitleBar {
    pub fn new(
        #[cfg(any(target_os = "linux", target_os = "freebsd"))] menus: Vec<TitleBarMenu>,
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))] _menus: Vec<TitleBarMenu>,
        account_control: Option<Entity<auth::TitleBarAccountControl>>,
        #[cfg(any(target_os = "linux", target_os = "freebsd"))] cx: &mut Context<Self>,
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))] _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            account_control,
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            menu_bar: cx.new(|cx| MenuBar::new(menus, cx)),
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn render_for_linux(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let height = platform_title_bar_height(window);
        let background = self.title_bar_background(window, cx);
        let button_layout = cx
            .button_layout()
            .unwrap_or_else(WindowButtonLayout::linux_default);
        let left_controls = self.render_linux_window_controls(button_layout.left, window, cx);
        let right_controls = self.render_linux_window_controls(button_layout.right, window, cx);

        let bar = div()
            .id("soukou-title-bar-linux")
            .w_full()
            .h(height)
            .bg(background)
            .border_b_1()
            .border_color(border_color(cx))
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            .on_mouse_down(gpui::MouseButton::Right, |event, window, cx| {
                cx.stop_propagation();
                window.show_window_menu(event.position);
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .size_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(SIDE_SLOT_WIDTH))
                            .px_3()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .justify_start()
                            .children(left_controls)
                            .child(self.menu_bar.clone().into_element()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(SIDE_SLOT_WIDTH))
                            .flex()
                            .flex_row()
                            .justify_end()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .when_some(self.account_control.clone(), |this, account_control| {
                                this.child(account_control.into_element())
                            })
                            .children(right_controls),
                    ),
            );

        match window.window_decorations() {
            Decorations::Server => bar.into_any_element(),
            Decorations::Client { tiling } => bar
                .when(!(tiling.top || tiling.right), |bar| {
                    bar.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING)
                })
                .when(!(tiling.top || tiling.left), |bar| {
                    bar.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING)
                })
                .mt(px(-1.0))
                .mb(px(-1.0))
                .border_1()
                .border_color(background)
                .into_any_element(),
        }
    }

    fn render_for_darwin(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let height = platform_title_bar_height(window);
        let background = self.title_bar_background(window, cx);
        div()
            .id("soukou-title-bar-macos")
            .w_full()
            .h(height)
            .bg(background)
            .border_b_1()
            .border_color(border_color(cx))
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            .on_click(|event: &ClickEvent, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .size_full()
                    .pl(px(MAC_TRAFFIC_LIGHT_PADDING))
                    .pr_3()
                    .child(
                        div()
                            .w(px(SIDE_SLOT_WIDTH))
                            .flex_none()
                            .text_size(px(12.0))
                            .text_color(text_color(cx)),
                    ),
            )
            .when_some(self.account_control.clone(), |this, account_control| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .h_full()
                        .px_3()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_end()
                        .child(account_control.into_element()),
                )
            })
            .into_any_element()
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn render_linux_window_controls(
        &self,
        buttons: [Option<WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let supported_controls = window.window_controls();
        let rendered_buttons = buttons
            .into_iter()
            .flatten()
            .filter(|button| match button {
                WindowButton::Minimize => supported_controls.minimize,
                WindowButton::Maximize => supported_controls.maximize,
                WindowButton::Close => true,
            })
            .map(|button| self.render_linux_window_button(button, window.is_maximized(), cx))
            .collect::<Vec<_>>();

        if rendered_buttons.is_empty() {
            None
        } else {
            Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .children(rendered_buttons)
                    .into_any_element(),
            )
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn render_linux_window_button(
        &self,
        button: WindowButton,
        is_maximized: bool,
        cx: &App,
    ) -> AnyElement {
        let label = match button {
            WindowButton::Minimize => "−",
            WindowButton::Maximize if is_maximized => "❐",
            WindowButton::Maximize => "□",
            WindowButton::Close => "×",
        };

        let hover_background = if matches!(button, WindowButton::Close) {
            rgba(232.0, 17.0, 35.0, 255.0)
        } else {
            mix(
                Theme::global(cx).bg_senodary(),
                Theme::global(cx).white(),
                0.18,
            )
        };
        let hover_text = if matches!(button, WindowButton::Close) {
            Hsla::from(Theme::global(cx).white())
        } else {
            text_color(cx)
        };

        div()
            .id(button.id())
            .w(px(22.0))
            .h(px(22.0))
            .rounded_sm()
            .flex()
            .flex_row()
            .justify_center()
            .items_center()
            .text_size(px(12.0))
            .text_color(text_color(cx))
            .cursor_pointer()
            .hover(move |style| style.bg(hover_background).text_color(hover_text))
            // .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
            //     // cx.stop_propagation();
            // })
            .on_click(move |_, window, _| match button {
                WindowButton::Minimize => window.minimize_window(),
                WindowButton::Maximize => window.zoom_window(),
                WindowButton::Close => window.remove_window(),
            })
            .child(label)
            .into_any_element()
    }

    fn title_bar_background(&self, window: &Window, cx: &App) -> Hsla {
        let active = Theme::global(cx).bg_senodary();
        let inactive = mix(active, Theme::global(cx).white(), 0.1);
        let color = if cfg!(any(target_os = "linux", target_os = "freebsd"))
            && !window.is_window_active()
        {
            inactive
        } else {
            active
        };
        color.into()
    }
}

fn border_color(cx: &App) -> Hsla {
    mix(Theme::global(cx).black(), Theme::global(cx).white(), 0.75).into()
}

fn text_color(cx: &App) -> Hsla {
    Theme::global(cx).text_primary().into()
}

fn mix(left: Rgba, right: Rgba, ratio: f32) -> Rgba {
    let ratio = ratio.clamp(0.0, 1.0);
    let inv = 1.0 - ratio;

    Rgba {
        r: left.r * inv + right.r * ratio,
        g: left.g * inv + right.g * ratio,
        b: left.b * inv + right.b * ratio,
        a: left.a * inv + right.a * ratio,
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn rgba(r: f32, g: f32, b: f32, a: f32) -> Rgba {
    Rgba { r, g, b, a }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match PlatformStyle::current() {
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            PlatformStyle::Linux => self.render_for_linux(window, cx),
            PlatformStyle::Mac => self.render_for_darwin(window, cx),
            PlatformStyle::Windows => div().into_any_element(),
            #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
            PlatformStyle::Linux => div().into_any_element(),
        }
    }
}
