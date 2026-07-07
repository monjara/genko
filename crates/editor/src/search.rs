use std::ops::Range;

use gpui::{
    App, BoxShadow, Entity, Focusable, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    Styled, Window, div, point, px, svg,
};
use theme::Theme;
use ui::TextInput;

use crate::editor_ui::{CloseSearch, FindNext, FindPrevious};

pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) matches: Vec<Range<usize>>,
    pub(crate) current_match: Option<usize>,
    pub(crate) synced_at_revision: u64,
}

impl SearchState {
    pub(crate) fn new(revision: u64) -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            current_match: None,
            synced_at_revision: revision,
        }
    }

    pub(crate) fn current_match_range(&self) -> Option<&Range<usize>> {
        self.current_match.and_then(|i| self.matches.get(i))
    }
}

pub(crate) fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut search_start = 0;
    while search_start < text.len() {
        match text[search_start..].find(query) {
            None => break,
            Some(rel_pos) => {
                let match_start = search_start + rel_pos;
                let match_end = match_start + query.len();
                matches.push(match_start..match_end);
                search_start = match_end;
            }
        }
    }
    matches
}

// Returns index of first match at or after cursor, wrapping around if none found.
pub(crate) fn nearest_match_index(matches: &[Range<usize>], cursor: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(matches.iter().position(|m| m.start >= cursor).unwrap_or(0))
}

fn toolbar_border_color(cx: &App) -> gpui::Hsla {
    let text = Theme::global(cx).text_primary();
    let surface = Theme::global(cx).surface();
    let ratio = 0.72_f32;
    let inv = 1.0 - ratio;
    gpui::Rgba {
        r: text.r * inv + surface.r * ratio,
        g: text.g * inv + surface.g * ratio,
        b: text.b * inv + surface.b * ratio,
        a: 1.0,
    }
    .into()
}

#[derive(IntoElement)]
pub(crate) struct SearchPanel {
    pub(crate) input: Entity<TextInput>,
    pub(crate) match_count: usize,
    pub(crate) current_match: Option<usize>, // 1-indexed for display
}

impl SearchPanel {
    pub(crate) fn new(
        input: Entity<TextInput>,
        match_count: usize,
        current_match: Option<usize>,
    ) -> Self {
        Self {
            input,
            match_count,
            current_match,
        }
    }
}

impl RenderOnce for SearchPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self.input.focus_handle(cx);
        window.focus(&focus_handle, cx);

        let match_label = match (self.current_match, self.match_count) {
            (_, 0) => "見つかりません".to_string(),
            (Some(current), count) => format!("{current} / {count}"),
            (None, _) => String::new(),
        };

        div()
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(Theme::global(cx).surface())
            .border_1()
            .border_color(toolbar_border_color(cx))
            .rounded_md()
            .shadow(vec![BoxShadow {
                color: gpui::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.16,
                },
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(18.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(div().w(px(160.0)).child(self.input))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(Theme::global(cx).text_senodary())
                    .min_w(px(60.0))
                    .child(match_label),
            )
            .child(search_button(0, "↑", "前を検索", FindPrevious, cx))
            .child(search_button(1, "↓", "次を検索", FindNext, cx))
            .child(
                div()
                    .id("search-close-button")
                    .w(px(24.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|style| style.bg(Theme::global(cx).bg_senodary()))
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        window.dispatch_action(Box::new(CloseSearch), cx);
                        cx.stop_propagation();
                    })
                    .child(
                        svg()
                            .external_path(icons::X)
                            .size_4()
                            .text_color(Theme::global(cx).text_primary()),
                    ),
            )
    }
}

fn search_button<Action>(
    id_index: usize,
    label: &'static str,
    _tooltip: &'static str,
    action: Action,
    cx: &mut App,
) -> impl IntoElement
where
    Action: gpui::Action + Clone + 'static,
{
    div()
        .id(("search-nav-button", id_index))
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_size(px(14.0))
        .text_color(Theme::global(cx).text_primary())
        .cursor_pointer()
        .hover(|style| style.bg(Theme::global(cx).bg_senodary()))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(action.clone()), cx);
            cx.stop_propagation();
        })
        .child(label)
}
