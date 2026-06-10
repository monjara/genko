use editor::VimModeLabel;
use gpui::{
    App, AppContext, Context, Decorations, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, svg,
};
use settings::AppSettings;
use theme::Theme;
use workspace::{ToggleWorkspacePane, WorkspaceState};

const BOTTOM_BAR_HEIGHT: f32 = 26.0;
const SIDE_SLOT_WIDTH: f32 = 128.0;

pub fn height(_window: &Window) -> gpui::Pixels {
    px(BOTTOM_BAR_HEIGHT)
}

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
        let background = background_color(window, cx);
        let workspace_visible = WorkspaceState::global(cx).is_pane_visible();
        let workspace_icon_color = if workspace_visible {
            Theme::global(cx).primary()
        } else {
            Theme::global(cx).text_senodary()
        };

        let bar = div()
            .id("soukou-bottom-bar")
            .w_full()
            .h(height(window))
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
                    .px_2()
                    .child(
                        div()
                            .w(px(SIDE_SLOT_WIDTH))
                            .flex_none()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .id("workspace-bottom-bar-toggle")
                                    .debug_selector(|| "workspace-bottom-bar-toggle".to_string())
                                    .w(px(22.0))
                                    .h(px(22.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .text_color(workspace_icon_color)
                                    .hover(|style| style.bg(Theme::global(cx).white()))
                                    .child(
                                        svg()
                                            .external_path(icons::PANEL_LEFT_OPEN)
                                            .size_4()
                                            .text_color(workspace_icon_color),
                                    )
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(ToggleWorkspacePane), cx);
                                    }),
                            ),
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

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            theme::init(cx);
            cx.set_global(AppSettings::default());
            workspace::WorkspaceState::init(cx);
            editor::init(cx);
        });
    }

    #[gpui::test]
    fn renders_workspace_toggle_button(cx: &mut TestAppContext) {
        init_test(cx);
        let (_bottom_bar, cx) = cx.add_window_view(|_, cx| BottomBar::new(cx));

        let button_bounds = cx
            .debug_bounds("workspace-bottom-bar-toggle")
            .expect("workspace toggle button should be rendered");
        assert!(button_bounds.size.width > px(0.0));
        assert!(button_bounds.size.height > px(0.0));
    }
}
