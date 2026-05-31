use gpui::{
    Context, Decorations, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window,
    div, prelude::FluentBuilder, px, transparent_black,
};
use menu::MenuActionHandler;
use theme::APP_FONT_FAMILY;
use theme::Theme;
use workspace::WorkspaceState;

use crate::app::{SoukouApp, active_modal::ActiveModal, unsupported_document::UnsupportedDocument};

impl Render for SoukouApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        title_bar::sync_client_window_inset(window);
        self.sync_window_title(window, cx);
        let bar_height = title_bar::platform_title_bar_height(window);
        let occupied_workspace_width = if self.workspace_pane_visible(cx) {
            WorkspaceState::global(cx).pane_width()
        } else {
            0.0
        };
        let mut editor_viewport_size = window.viewport_size();
        editor_viewport_size.width =
            px((editor_viewport_size.width.as_f32() - occupied_workspace_width).max(0.0));
        editor_viewport_size.height -= bar_height * 2.0;
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.update_viewport_size(editor_viewport_size, cx);
        });

        let content = if WorkspaceState::global(cx).unsupported_file().is_some() {
            UnsupportedDocument::from_workspace(cx).into_any_element()
        } else {
            self.editor_controller.clone().into_any_element()
        };

        div()
            .size_full()
            .bg(transparent_black())
            .map(|this| match window.window_decorations() {
                Decorations::Server => this,
                Decorations::Client { tiling } => this
                    .when(!tiling.top, |this| {
                        this.pt(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.bottom, |this| {
                        this.pb(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.left, |this| {
                        this.pl(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    })
                    .when(!tiling.right, |this| {
                        this.pr(title_bar::CLIENT_SIDE_SHADOW_SIZE)
                    }),
            })
            .child(
                div()
                    .size_full()
                    .bg(Theme::global(cx).white())
                    .font_family(APP_FONT_FAMILY)
                    .flex()
                    .flex_col()
                    .items_center()
                    .overflow_hidden()
                    .map(|this| match window.window_decorations() {
                        Decorations::Server => this,
                        Decorations::Client { tiling } => this
                            .when(!(tiling.top || tiling.right), |this| {
                                this.rounded_tr(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.top || tiling.left), |this| {
                                this.rounded_tl(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.right), |this| {
                                this.rounded_br(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.left), |this| {
                                this.rounded_bl(title_bar::CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!tiling.is_tiled(), |this| {
                                this.shadow(title_bar::client_window_shadow())
                            }),
                    })
                    .can_drop(|value, _, _| value.is::<gpui::ExternalPaths>())
                    .on_drop(cx.listener(Self::drop_external_paths))
                    .on_action(cx.listener(Self::open_file_action))
                    .on_action(cx.listener(Self::open_workspace_action))
                    .on_action(cx.listener(Self::toggle_workspace_pane_action))
                    .on_action(cx.listener(Self::dismiss_active_modal_action))
                    .on_action(cx.listener(Self::open_modal_primary_action))
                    .on_action(cx.listener(Self::save_file_action))
                    .on_action(cx.listener(Self::export_txt_action))
                    .on_action(cx.listener(Self::check_for_updates_action))
                    .on_action(cx.listener(Self::vim_command_write_action))
                    .on_action(cx.listener(Self::vim_command_quit_action))
                    .child(self.title_bar.clone().into_element())
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .flex()
                            .when(self.workspace_pane_visible(cx), |this| {
                                this.child(self.workspace.clone().into_element())
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(content),
                            ),
                    )
                    .when_some(self.active_modal.clone(), |this, modal| {
                        this.child(ActiveModal::from_modal(modal))
                    })
                    .child(self.bottom_bar.clone().into_element()),
            )
    }
}
