use gpui::{
    App, BoxShadow, Entity, Focusable, Hsla, InteractiveElement, IntoElement, ParentElement,
    Pixels, RenderOnce, StatefulInteractiveElement, Styled, Window, actions, div, point,
    prelude::FluentBuilder, px,
};
use theme::Theme;
use ui::{
    TextInput, Tooltip, menu_item, small_icon_button, toolbar_border_color, vertical_toolbar_button,
};

use crate::editor::{PageBreakMenuKind, PageBreakMenuRequest};

actions!(
    editor,
    [
        ApplyRichTextBold,
        ApplyRichTextEmphasis,
        ApplyRichTextHeading,
        ApplyRichTextRotated,
        ApplyRubyEdit,
        CancelRubyEdit,
        OpenSearch,
        CloseSearch,
        FindNext,
        FindPrevious
    ]
);

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = editor, no_json, no_register)]
pub struct SetPageBreakRightOfColumn {
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = editor, no_json, no_register)]
pub struct SetPageBreakLeftOfColumn {
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = editor, no_json, no_register)]
pub struct RemovePageBreakColumn {
    pub column: usize,
}

#[derive(IntoElement)]
pub struct PageBreakMenu {
    request: PageBreakMenuRequest,
}

impl PageBreakMenu {
    pub fn new(request: PageBreakMenuRequest) -> Self {
        Self { request }
    }
}

impl RenderOnce for PageBreakMenu {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let column_for_right = self.request.column;
        let column_for_left = self.request.column;
        let column_for_remove = self.request.column;
        let left = (self.request.bounds.right() + px(6.0)).max(px(8.0));
        let top = self.request.bounds.top().max(px(8.0));

        div()
            .absolute()
            .left(left)
            .top(top)
            .flex()
            .flex_col()
            .bg(Theme::global(cx).white())
            .border_1()
            .border_color(toolbar_border_color(cx))
            .rounded_md()
            .shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.16,
                },
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(18.0),
                spread_radius: px(0.0),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .when(self.request.kind == PageBreakMenuKind::Set, |this| {
                this.child(page_break_menu_item(
                    "右側に改ページ",
                    SetPageBreakRightOfColumn {
                        column: column_for_right,
                    },
                    cx,
                ))
                .child(page_break_menu_item(
                    "左側に改ページ",
                    SetPageBreakLeftOfColumn {
                        column: column_for_left,
                    },
                    cx,
                ))
            })
            .when(self.request.kind == PageBreakMenuKind::Remove, |this| {
                this.child(page_break_menu_item(
                    "改ページを削除",
                    RemovePageBreakColumn {
                        column: column_for_remove,
                    },
                    cx,
                ))
            })
    }
}

fn page_break_menu_item<Action>(
    label: &'static str,
    action: Action,
    cx: &mut App,
) -> impl IntoElement
where
    Action: gpui::Action + Clone + 'static,
{
    menu_item(label, cx).on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
        window.dispatch_action(Box::new(action.clone()), cx);
        cx.stop_propagation();
    })
}

#[derive(IntoElement)]
pub struct RubyEditorPopover {
    bounds: gpui::Bounds<Pixels>,
    input: Entity<TextInput>,
}

impl RubyEditorPopover {
    pub fn new(bounds: gpui::Bounds<Pixels>, input: Entity<TextInput>) -> Self {
        Self { bounds, input }
    }
}

impl RenderOnce for RubyEditorPopover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let input = self.input;
        let focus_handle = input.focus_handle(cx);
        window.focus(&focus_handle, cx);

        let left = (self.bounds.right() + px(4.0)).max(px(8.0));
        let top = (self.bounds.top() - px(4.0)).max(px(8.0));

        div()
            .absolute()
            .left(left)
            .top(top)
            .w(px(78.0))
            .h(px(190.0))
            .p_2()
            .flex()
            .flex_row()
            .gap_2()
            .bg(Theme::global(cx).white())
            .border_1()
            .border_color(toolbar_border_color(cx))
            .rounded_md()
            .shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.18,
                },
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(20.0),
                spread_radius: px(0.0),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(input)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(ruby_editor_button(
                        icons::CHECK,
                        0,
                        "適用",
                        None,
                        ApplyRubyEdit,
                        cx,
                    ))
                    .child(ruby_editor_button(
                        icons::X,
                        1,
                        "取り消し",
                        Some("Esc"),
                        CancelRubyEdit,
                        cx,
                    )),
            )
    }
}

fn ruby_editor_button<Action>(
    icon_path: &'static str,
    id_index: usize,
    tooltip_title: &'static str,
    shortcut: Option<&'static str>,
    action: Action,
    cx: &mut App,
) -> impl IntoElement
where
    Action: gpui::Action + Clone + 'static,
{
    small_icon_button(("ruby-editor-button", id_index), icon_path, cx)
        .tooltip(Tooltip::new(
            tooltip_title,
            shortcut.map(gpui::SharedString::from),
        ))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(action.clone()), cx);
            cx.stop_propagation();
        })
}

#[derive(IntoElement)]
pub struct RichTextToolbar {
    selection_bounds: gpui::Bounds<Pixels>,
}

impl RichTextToolbar {
    pub fn new(selection_bounds: gpui::Bounds<Pixels>) -> Self {
        Self { selection_bounds }
    }
}

impl RenderOnce for RichTextToolbar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let x = (self.selection_bounds.right() + px(1.0)).max(px(1.0));
        let y = (self.selection_bounds.top() - px(10.0)).max(px(8.0));
        let border = toolbar_border_color(cx);

        div()
            .absolute()
            .left(x)
            .top(y)
            .flex()
            .flex_col()
            .items_center()
            .bg(Theme::global(cx).white())
            .border_1()
            .border_color(border)
            .rounded_md()
            .shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.2,
                },
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(22.0),
                spread_radius: px(0.0),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(toolbar_button("B", 0, "太字", ApplyRichTextBold, cx))
            .child(toolbar_button("•", 1, "傍点", ApplyRichTextEmphasis, cx))
            .child(toolbar_button("見", 2, "見出し", ApplyRichTextHeading, cx))
            .child(toolbar_button("回", 3, "回転", ApplyRichTextRotated, cx))
    }
}

fn toolbar_button<Action>(
    label: &'static str,
    id_index: usize,
    tooltip_title: &'static str,
    action: Action,
    cx: &mut App,
) -> impl IntoElement
where
    Action: gpui::Action + Clone + 'static,
{
    let tooltip_action = action.clone();
    vertical_toolbar_button(("rich-text-toolbar-button", id_index), label, cx)
        .tooltip(Tooltip::for_action(tooltip_title, tooltip_action))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(action.clone()), cx);
        })
}
