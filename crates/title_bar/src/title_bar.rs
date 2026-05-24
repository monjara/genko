use gpui::{
    Anchor, AnyElement, App, AppContext, BoxShadow, ClickEvent, Context, Decorations, Hsla,
    InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render, Rgba, SharedString,
    StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowControlArea, anchored,
    deferred, div, point, prelude::FluentBuilder, px,
};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use gpui::{Entity, WindowButton, WindowButtonLayout, WindowDecorations};
use std::rc::Rc;
use theme::Theme;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use ui::MenuBar;

pub use ui::{MenuBarItem as TitleBarMenuItem, MenuBarMenu as TitleBarMenu};

const MAC_TRAFFIC_LIGHT_PADDING: f32 = 71.0;
const SIDE_SLOT_WIDTH: f32 = 160.0;
const AUTH_MENU_MIN_WIDTH: Pixels = px(220.0);
const AUTH_MENU_VERTICAL_OFFSET: Pixels = px(12.0);
pub const CLIENT_SIDE_DECORATION_ROUNDING: Pixels = px(10.0);
pub const CLIENT_SIDE_SHADOW_SIZE: Pixels = px(10.0);

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

// TODO crates fix visibility of this function
pub fn client_window_shadow() -> Vec<BoxShadow> {
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
    }]
}

// TODO crates fix visibility of this function
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleBarUser {
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum TitleBarAuthState {
    #[default]
    Anonymous,
    Authenticated(TitleBarUser),
}

#[derive(Clone)]
pub struct TitleBarAuthActions {
    sign_in: Rc<dyn Fn(&mut Window, &mut App)>,
    open_account_settings: Rc<dyn Fn(&mut Window, &mut App)>,
    sign_out: Rc<dyn Fn(&mut Window, &mut App)>,
}

impl TitleBarAuthActions {
    pub fn new(
        sign_in: impl Fn(&mut Window, &mut App) + 'static,
        open_account_settings: impl Fn(&mut Window, &mut App) + 'static,
        sign_out: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            sign_in: Rc::new(sign_in),
            open_account_settings: Rc::new(open_account_settings),
            sign_out: Rc::new(sign_out),
        }
    }
}

pub struct TitleBar {
    title: SharedString,
    auth_state: TitleBarAuthState,
    auth_actions: Option<TitleBarAuthActions>,
    auth_menu_open: bool,
    auth_menu_position: Point<Pixels>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    menu_bar: Entity<MenuBar>,
}

impl TitleBar {
    pub fn new(
        title: &str,
        #[cfg(any(target_os = "linux", target_os = "freebsd"))] menus: Vec<TitleBarMenu>,
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))] _menus: Vec<TitleBarMenu>,
        auth_actions: Option<TitleBarAuthActions>,
        #[cfg(any(target_os = "linux", target_os = "freebsd"))] cx: &mut Context<Self>,
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))] _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            title: title.into(),
            auth_state: TitleBarAuthState::Anonymous,
            auth_actions,
            auth_menu_open: false,
            auth_menu_position: point(px(0.0), px(0.0)),
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            menu_bar: cx.new(|cx| MenuBar::new(menus, cx)),
        }
    }

    pub fn set_auth_state(&mut self, auth_state: TitleBarAuthState, cx: &mut Context<Self>) {
        self.auth_state = auth_state;
        if !matches!(self.auth_state, TitleBarAuthState::Authenticated(_)) {
            self.auth_menu_open = false;
        }
        cx.notify();
    }

    fn toggle_auth_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.auth_menu_open {
            self.auth_menu_open = false;
            cx.notify();
            return;
        }

        self.auth_menu_open = true;
        self.auth_menu_position = point(
            position.x - px(180.0),
            position.y + AUTH_MENU_VERTICAL_OFFSET,
        );
        cx.notify();
    }

    fn close_auth_menu(&mut self, cx: &mut Context<Self>) {
        if self.auth_menu_open {
            self.auth_menu_open = false;
            cx.notify();
        }
    }

    fn auth_initial(display_name: &str) -> String {
        display_name
            .chars()
            .next()
            .map(|ch| ch.to_uppercase().collect())
            .unwrap_or_else(|| "U".to_string())
    }

    fn render_auth_trigger(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let actions = self.auth_actions.clone()?;

        match &self.auth_state {
            TitleBarAuthState::Anonymous => Some(
                div()
                    .id("title-bar-sign-in")
                    .px_3()
                    .h(px(26.0))
                    .rounded_sm()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(text_color(cx))
                    .border_1()
                    .border_color(border_color(cx))
                    .bg(Theme::global(cx).white())
                    .cursor_pointer()
                    .hover(|style| {
                        style.bg(mix(
                            Theme::global(cx).bg_senodary(),
                            Theme::global(cx).white(),
                            0.14,
                        ))
                    })
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| {
                        (actions.sign_in)(window, cx);
                    })
                    .child("Sign In")
                    .into_any_element(),
            ),
            TitleBarAuthState::Authenticated(user) => {
                let display_name = user.display_name.clone();
                let initial = Self::auth_initial(display_name.as_str());

                Some(
                    div()
                        .id("title-bar-account-trigger")
                        .w(px(30.0))
                        .h(px(30.0))
                        .rounded_full()
                        .border_1()
                        .border_color(border_color(cx))
                        .bg(mix(
                            Theme::global(cx).primary(),
                            Theme::global(cx).white(),
                            0.78,
                        ))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.0))
                        .text_color(text_color(cx))
                        .cursor_pointer()
                        .hover(|style| {
                            style.bg(mix(
                                Theme::global(cx).primary(),
                                Theme::global(cx).white(),
                                0.68,
                            ))
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.toggle_auth_menu(event.position(), cx);
                        }))
                        .child(initial)
                        .into_any_element(),
                )
            }
        }
    }

    fn render_auth_menu_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let TitleBarAuthState::Authenticated(user) = &self.auth_state else {
            return None;
        };
        let actions = self.auth_actions.clone()?;
        if !self.auth_menu_open {
            return None;
        }

        let display_name = user.display_name.clone();
        let email = user.email.clone();
        let initial = Self::auth_initial(display_name.as_str());
        let popup_background = Theme::global(cx).white();
        let item_hover_background = mix(
            Theme::global(cx).bg_senodary(),
            Theme::global(cx).white(),
            0.12,
        );

        Some(
            deferred(
                anchored()
                    .position(self.auth_menu_position)
                    .anchor(Anchor::TopLeft)
                    .child(
                        div()
                            .id("title-bar-auth-popup")
                            .min_w(AUTH_MENU_MIN_WIDTH)
                            .py_2()
                            .bg(popup_background)
                            .border_1()
                            .border_color(border_color(cx))
                            .rounded_md()
                            .shadow(vec![BoxShadow {
                                color: Hsla {
                                    h: 0.0,
                                    s: 0.0,
                                    l: 0.0,
                                    a: 0.18,
                                },
                                offset: point(px(0.0), px(8.0)),
                                blur_radius: px(24.0),
                                spread_radius: px(0.0),
                            }])
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.close_auth_menu(cx);
                            }))
                            .child(
                                div()
                                    .px_3()
                                    .pb_2()
                                    .flex()
                                    .flex_row()
                                    .gap_3()
                                    .items_center()
                                    .child(
                                        div()
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(border_color(cx))
                                            .bg(mix(
                                                Theme::global(cx).primary(),
                                                Theme::global(cx).white(),
                                                0.78,
                                            ))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(13.0))
                                            .text_color(text_color(cx))
                                            .child(initial),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(text_color(cx))
                                                    .child(display_name),
                                            )
                                            .when_some(email, |this, email| {
                                                this.child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(secondary_text_color(cx))
                                                        .child(email),
                                                )
                                            }),
                                    ),
                            )
                            .child(div().h(px(1.0)).mx_2().bg(border_color(cx)))
                            .child(
                                div()
                                    .mt_1()
                                    .child(
                                        div()
                                            .id("title-bar-account-settings")
                                            .w_full()
                                            .px_3()
                                            .py_1p5()
                                            .rounded_sm()
                                            .text_size(px(12.0))
                                            .text_color(text_color(cx))
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(item_hover_background))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.close_auth_menu(cx);
                                                (actions.open_account_settings)(window, cx);
                                            }))
                                            .child("アカウント設定"),
                                    )
                                    .child(
                                        div()
                                            .id("title-bar-sign-out")
                                            .w_full()
                                            .px_3()
                                            .py_1p5()
                                            .rounded_sm()
                                            .text_size(px(12.0))
                                            .text_color(text_color(cx))
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(item_hover_background))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.close_auth_menu(cx);
                                                (actions.sign_out)(window, cx);
                                            }))
                                            .child("ログアウト"),
                                    ),
                            ),
                    ),
            )
            .into_any_element(),
        )
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
        let auth_trigger = self.render_auth_trigger(cx);
        let auth_menu_popup = self.render_auth_menu_popup(cx);

        let bar = div()
            .id("soukou-title-bar-linux")
            .w_full()
            .h(height)
            .bg(background)
            .border_b_1()
            .border_color(border_color(cx))
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
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            })
                            .on_mouse_down(gpui::MouseButton::Right, |event, window, cx| {
                                cx.stop_propagation();
                                window.show_window_menu(event.position);
                            })
                            .text_center()
                            .text_size(px(12.0))
                            .text_color(text_color(cx))
                            .child(self.title.clone()),
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
                            .children(right_controls),
                    ),
            )
            .when_some(auth_trigger, |this, auth_trigger| {
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
                        .child(auth_trigger),
                )
            })
            .children(auth_menu_popup);

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
        let auth_trigger = self.render_auth_trigger(cx);
        let auth_menu_popup = self.render_auth_menu_popup(cx);
        div()
            .id("soukou-title-bar-macos")
            .w_full()
            .h(height)
            .bg(background)
            .border_b_1()
            .border_color(border_color(cx))
            .window_control_area(WindowControlArea::Drag)
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
            .when_some(auth_trigger, |this, auth_trigger| {
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
                        .child(auth_trigger),
                )
            })
            .children(auth_menu_popup)
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

fn secondary_text_color(cx: &App) -> Hsla {
    Theme::global(cx).text_senodary().into()
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
