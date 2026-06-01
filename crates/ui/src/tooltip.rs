use gpui::{
    Action, AnyView, App, AppContext, Context, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, div, prelude::FluentBuilder, px,
};
use theme::{APP_FONT_FAMILY, Theme};

pub struct Tooltip {
    title: SharedString,
    shortcut: Option<SharedString>,
}

impl Tooltip {
    pub fn new(
        title: impl Into<SharedString>,
        shortcut: Option<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title = title.into();
        move |_, cx| {
            cx.new(|_| Self {
                title: title.clone(),
                shortcut: shortcut.clone(),
            })
            .into()
        }
    }

    pub fn text(title: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title = title.into();
        move |_, cx| {
            cx.new(|_| Self {
                title: title.clone(),
                shortcut: None,
            })
            .into()
        }
    }

    pub fn with_shortcut(
        title: impl Into<SharedString>,
        shortcut: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title = title.into();
        let shortcut = shortcut.into();
        move |_, cx| {
            cx.new(|_| Self {
                title: title.clone(),
                shortcut: Some(shortcut.clone()),
            })
            .into()
        }
    }

    pub fn for_action<ActionType>(
        title: impl Into<SharedString>,
        action: ActionType,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView
    where
        ActionType: Action + Clone + 'static,
    {
        let title = title.into();
        move |window, cx| {
            let shortcut = window
                .highest_precedence_binding_for_action(&action)
                .map(|binding| {
                    binding
                        .keystrokes()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .filter(|shortcut| !shortcut.is_empty())
                .map(SharedString::from);

            cx.new(|_| Self {
                title: title.clone(),
                shortcut,
            })
            .into()
        }
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().pl_2().pt_2().child(
            div()
                .px_3()
                .py_2()
                .flex()
                .gap_4()
                .items_center()
                .rounded_sm()
                .bg(Theme::global(cx).black())
                .text_color(Theme::global(cx).white())
                .font_family(APP_FONT_FAMILY)
                .text_size(px(12.0))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(self.title.clone()),
                )
                .when_some(self.shortcut.clone(), |this, shortcut| {
                    this.child(
                        div()
                            .px_1()
                            .rounded_sm()
                            .bg(Theme::global(cx).text_primary())
                            .text_color(Theme::global(cx).white())
                            .child(shortcut),
                    )
                }),
        )
    }
}
