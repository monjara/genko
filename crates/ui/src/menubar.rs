use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, BoxShadow, ClickEvent, Context, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, Render, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, anchored, deferred, div, point, prelude::FluentBuilder, px,
};
use theme::Theme;

const MENU_TRIGGER_HEIGHT: Pixels = px(24.0);
const MENU_POPUP_MIN_WIDTH: Pixels = px(180.0);
const MENU_POPUP_VERTICAL_OFFSET: Pixels = px(14.0);

#[derive(Clone)]
pub struct MenuBarItem {
    label: SharedString,
    on_select: Rc<dyn Fn(&mut Window, &mut App)>,
}

impl MenuBarItem {
    pub fn new(
        label: impl Into<SharedString>,
        on_select: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            on_select: Rc::new(on_select),
        }
    }
}

#[derive(Clone)]
pub struct MenuBarMenu {
    label: SharedString,
    items: Vec<MenuBarItem>,
}

impl MenuBarMenu {
    pub fn new(label: impl Into<SharedString>, items: Vec<MenuBarItem>) -> Self {
        Self {
            label: label.into(),
            items,
        }
    }
}

pub struct MenuBar {
    menus: Vec<MenuBarMenu>,
    open_menu_index: Option<usize>,
    open_menu_position: Point<Pixels>,
}

impl MenuBar {
    pub fn new(menus: Vec<MenuBarMenu>, _cx: &mut Context<Self>) -> Self {
        Self {
            menus,
            open_menu_index: None,
            open_menu_position: point(px(0.0), px(0.0)),
        }
    }

    fn toggle_menu(&mut self, menu_index: usize, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.open_menu_index == Some(menu_index) {
            self.open_menu_index = None;
            cx.notify();
            return;
        }

        self.open_menu_index = Some(menu_index);
        self.open_menu_position = point(
            position.x - px(8.0),
            position.y + MENU_POPUP_VERTICAL_OFFSET,
        );
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.open_menu_index.take().is_some() {
            cx.notify();
        }
    }

    fn select_menu_item(
        &mut self,
        menu_index: usize,
        item_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.menus.get(menu_index) else {
            self.close_menu(cx);
            return;
        };
        let Some(item) = menu.items.get(item_index) else {
            self.close_menu(cx);
            return;
        };

        let on_select = item.on_select.clone();
        self.close_menu(cx);
        on_select(window, cx);
    }

    fn render_menu_trigger(
        &self,
        menu_index: usize,
        menu: &MenuBarMenu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_open = self.open_menu_index == Some(menu_index);
        let active_background = mix(
            Theme::global(cx).bg_senodary(),
            Theme::global(cx).white(),
            0.24,
        );
        let hover_background = mix(
            Theme::global(cx).bg_senodary(),
            Theme::global(cx).white(),
            0.18,
        );

        div()
            .id(format!("menu-bar-trigger-{menu_index}"))
            .debug_selector(move || format!("menu-bar-trigger-{menu_index}"))
            .px_2()
            .h(MENU_TRIGGER_HEIGHT)
            .rounded_sm()
            .flex()
            .flex_row()
            .items_center()
            .text_size(px(12.0))
            .text_color(text_color(cx))
            .cursor_pointer()
            .when(is_open, |this| this.bg(active_background))
            .hover(move |style| style.bg(hover_background))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(menu_index, event.position(), cx);
            }))
            .child(menu.label.clone())
            .into_any_element()
    }

    fn render_menu_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu_index = self.open_menu_index?;
        let menu = self.menus.get(menu_index)?;
        let background = Theme::global(cx).white();
        let border = mix(Theme::global(cx).black(), Theme::global(cx).white(), 0.75);
        let item_hover_background = mix(
            Theme::global(cx).bg_senodary(),
            Theme::global(cx).white(),
            0.12,
        );

        Some(
            deferred(
                anchored()
                    .position(self.open_menu_position)
                    .anchor(Anchor::TopLeft)
                    .child(
                        div()
                            .id(format!("menu-bar-popup-{menu_index}"))
                            .debug_selector(move || format!("menu-bar-popup-{menu_index}"))
                            .min_w(MENU_POPUP_MIN_WIDTH)
                            .py_1()
                            .bg(background)
                            .border_1()
                            .border_color(border)
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
                                inset: false,
                            }])
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.close_menu(cx);
                            }))
                            .children(menu.items.iter().enumerate().map(|(item_index, item)| {
                                div()
                                    .id(format!("menu-bar-item-{menu_index}-{item_index}"))
                                    .debug_selector(move || {
                                        format!("menu-bar-item-{menu_index}-{item_index}")
                                    })
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
                                        this.select_menu_item(menu_index, item_index, window, cx);
                                    }))
                                    .child(item.label.clone())
                                    .into_any_element()
                            })),
                    ),
            )
            .into_any_element(),
        )
    }
}

impl Render for MenuBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("menu-bar")
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .children(
                self.menus
                    .iter()
                    .enumerate()
                    .map(|(menu_index, menu)| self.render_menu_trigger(menu_index, menu, cx)),
            )
            .children(self.render_menu_popup(cx))
    }
}

fn text_color(cx: &App) -> Hsla {
    Theme::global(cx).text_primary().into()
}

fn mix(left: Rgba, right: Rgba, ratio: f32) -> Rgba {
    let ratio = ratio.clamp(0.0, 1.0);
    let inverse_ratio = 1.0 - ratio;

    Rgba {
        r: left.r * inverse_ratio + right.r * ratio,
        g: left.g * inverse_ratio + right.g * ratio,
        b: left.b * inverse_ratio + right.b * ratio,
        a: left.a * inverse_ratio + right.a * ratio,
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use gpui::{Modifiers, TestAppContext};

    use super::*;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(theme::init);
    }

    #[gpui::test]
    fn opens_menu_and_selects_item(cx: &mut TestAppContext) {
        init_test(cx);
        let selected = Rc::new(Cell::new(false));
        let selected_for_item = selected.clone();

        let (menu_bar, cx) = cx.add_window_view(|_, cx| {
            MenuBar::new(
                vec![MenuBarMenu::new(
                    "File",
                    vec![MenuBarItem::new("Open", move |_, _| {
                        selected_for_item.set(true);
                    })],
                )],
                cx,
            )
        });

        assert!(cx.debug_bounds("menu-bar-trigger-0").is_some());
        assert!(cx.debug_bounds("menu-bar-popup-0").is_none());

        let trigger_bounds = cx
            .debug_bounds("menu-bar-trigger-0")
            .expect("menu trigger should be rendered");
        cx.simulate_click(trigger_bounds.center(), Modifiers::none());

        menu_bar.read_with(cx, |menu_bar, _| {
            assert_eq!(menu_bar.open_menu_index, Some(0));
        });
        assert!(cx.debug_bounds("menu-bar-popup-0").is_some());

        let item_bounds = cx
            .debug_bounds("menu-bar-item-0-0")
            .expect("menu item should be rendered after opening menu");
        cx.simulate_click(item_bounds.center(), Modifiers::none());

        menu_bar.read_with(cx, |menu_bar, _| {
            assert_eq!(menu_bar.open_menu_index, None);
        });
        assert!(selected.get());
    }
}
