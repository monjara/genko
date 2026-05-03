use editor::VimModeLabel;
use gpui::{
    App, AppContext, Context, Decorations, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use settings::AppSettings;
use theme::Theme;
use workspace::ToggleWorkspacePane;

const SIDE_SLOT_WIDTH: f32 = 160.0;

pub struct BottomBar {
    vim_mode_status: Entity<VimModeLabel>,
}

impl BottomBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let vim_mode_status = cx.new(VimModeLabel::new);
        Self { vim_mode_status }
    }
}

impl Render for BottomBar {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let height = title_bar::platform_title_bar_height(window);
        let background = background_color(window, cx);

        let bar = div()
            .id("genko-bottom-bar")
            .w_full()
            .h(height)
            .bg(background)
            .border_t_1()
            .border_color(border_color(cx))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .child(
                        div()
                            .id("workspace-pane-toggle")
                            .w(px(28.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(Theme::global(cx).primary())
                            .cursor_pointer()
                            .child("▤")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleWorkspacePane), cx);
                            }),
                    )
                    .when(AppSettings::global(cx).vim_mode, |this| {
                        this.child(
                            div()
                                .w(px(SIDE_SLOT_WIDTH))
                                .flex_none()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_end()
                                .child(self.vim_mode_status.clone().into_element()),
                        )
                    }),
            );

        match window.window_decorations() {
            Decorations::Server => bar.into_any_element(),
            Decorations::Client { tiling } => bar
                .when(!(tiling.bottom || tiling.right), |bar| {
                    bar.rounded_br(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                })
                .when(!(tiling.bottom || tiling.left), |bar| {
                    bar.rounded_bl(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                })
                .mt(px(-1.0))
                .mb(px(-1.0))
                .border_1()
                .border_color(background)
                .into_any_element(),
        }
    }
}

fn background_color(window: &Window, cx: &App) -> gpui::Hsla {
    let active = Theme::global(cx).bg_senodary();
    let inactive = mix(active, Theme::global(cx).white(), 0.1);
    if cfg!(any(target_os = "linux", target_os = "freebsd")) && !window.is_window_active() {
        inactive.into()
    } else {
        active.into()
    }
}

fn border_color(cx: &App) -> gpui::Hsla {
    mix(Theme::global(cx).black(), Theme::global(cx).white(), 0.75).into()
}

fn mix(left: gpui::Rgba, right: gpui::Rgba, ratio: f32) -> gpui::Rgba {
    let ratio = ratio.clamp(0.0, 1.0);
    let inv = 1.0 - ratio;
    gpui::Rgba {
        r: left.r * inv + right.r * ratio,
        g: left.g * inv + right.g * ratio,
        b: left.b * inv + right.b * ratio,
        a: left.a * inv + right.a * ratio,
    }
}
