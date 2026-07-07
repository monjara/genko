use gpui::{
    App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use theme::Theme;

pub fn primary_button(label: impl IntoElement, cx: &App) -> Div {
    div()
        .px_4()
        .py_2()
        .rounded_sm()
        .bg(Theme::global(cx).primary())
        .text_color(Theme::global(cx).white())
        .cursor_pointer()
        .hover(|style| style.opacity(0.92))
        .child(label)
}

pub fn secondary_button(label: impl IntoElement, cx: &App) -> Div {
    div()
        .px_4()
        .py_2()
        .rounded_sm()
        .border_1()
        .border_color(subtle_border_color(cx))
        .cursor_pointer()
        .hover(|style| style.bg(Theme::global(cx).bg_senodary()))
        .child(label)
}

pub fn selectable_chip(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    active: bool,
    cx: &App,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(Theme::global(cx).primary())
        .bg(if active {
            Theme::global(cx).primary()
        } else {
            Theme::global(cx).white()
        })
        .text_color(if active {
            Theme::global(cx).white()
        } else {
            Theme::global(cx).text_primary()
        })
        .cursor_pointer()
        .active(|this| this.opacity(0.85))
        .child(label)
}

pub fn toggle_button(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_sm()
        .bg(if enabled {
            Theme::global(cx).primary()
        } else {
            Theme::global(cx).white()
        })
        .text_color(if enabled {
            Theme::global(cx).white()
        } else {
            Theme::global(cx).primary()
        })
        .cursor_pointer()
        .child(label)
}

pub fn square_button(id: impl Into<ElementId>, label: impl IntoElement, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(32.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(Theme::global(cx).primary())
        .cursor_pointer()
        .active(|this| this.opacity(0.85))
        .child(label)
}

pub fn menu_item(label: impl IntoElement, cx: &App) -> Div {
    div()
        .px_3()
        .py_2()
        .text_size(px(12.0))
        .text_color(Theme::global(cx).text_primary())
        .cursor_pointer()
        .hover(|style| style.bg(Theme::global(cx).bg_senodary()))
        .child(label)
}

pub fn compact_menu_item(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    hover_background: gpui::Rgba,
    cx: &App,
) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .px_3()
        .py_1p5()
        .rounded_sm()
        .text_size(px(12.0))
        .text_color(Theme::global(cx).text_primary())
        .cursor_pointer()
        .hover(move |style| style.bg(hover_background))
        .child(label)
}

pub fn small_icon_button(
    id: impl Into<ElementId>,
    icon_path: &'static str,
    cx: &App,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_color(Theme::global(cx).text_primary())
        .bg(Theme::global(cx).bg_senodary())
        .cursor_pointer()
        .hover(|style| style.bg(Theme::global(cx).white()))
        .child(
            svg()
                .external_path(icon_path)
                .size_4()
                .text_color(Theme::global(cx).text_primary()),
        )
}

pub fn vertical_toolbar_button(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(38.0))
        .h(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(Theme::global(cx).black())
        .border_b_1()
        .border_color(toolbar_border_color(cx))
        .cursor_pointer()
        .hover(|style| style.bg(Theme::global(cx).bg_senodary()))
        .child(label)
}

pub fn toolbar_border_color(cx: &App) -> gpui::Hsla {
    mix(Theme::global(cx).black(), Theme::global(cx).white(), 0.72).into()
}

fn subtle_border_color(cx: &App) -> gpui::Hsla {
    mix(gpui::black().into(), Theme::global(cx).white(), 0.75).into()
}

fn mix(left: gpui::Rgba, right: gpui::Rgba, ratio: f32) -> gpui::Rgba {
    gpui::Rgba {
        r: left.r * (1.0 - ratio) + right.r * ratio,
        g: left.g * (1.0 - ratio) + right.g * ratio,
        b: left.b * (1.0 - ratio) + right.b * ratio,
        a: left.a * (1.0 - ratio) + right.a * ratio,
    }
}
