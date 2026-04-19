use gpui::{
    App, Application, Bounds, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

const ROWS: usize = 20;
const COLUMNS: usize = 20;
const CELL_SIZE: f32 = 28.0;

struct GenkoApp {
    title: SharedString,
    draft: Vec<Option<char>>,
}

impl GenkoApp {
    fn new() -> Self {
        let mut draft = vec![None; ROWS * COLUMNS];

        for (index, character) in "GENKO".chars().enumerate() {
            draft[index] = Some(character);
        }

        Self {
            title: "Genko".into(),
            draft,
        }
    }

    fn render_cell(&self, index: usize) -> impl IntoElement {
        div()
            .size(px(CELL_SIZE))
            .border_1()
            .border_color(rgb(0xd94b4b))
            .bg(rgb(0xfffbf2))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0x2f241d))
            .text_lg()
            .child(
                self.draft[index]
                    .map(|character| character.to_string())
                    .unwrap_or_default(),
            )
    }

    fn render_row(&self, row: usize) -> impl IntoElement {
        div()
            .flex()
            .children((0..COLUMNS).map(|column| self.render_cell(row * COLUMNS + column)))
    }
}

impl Render for GenkoApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0xebe5d8))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .items_end()
                            .text_color(rgb(0x2f241d))
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(self.title.clone()),
                            )
                            .child(div().text_sm().text_color(rgb(0x705a4a)).child(format!(
                                "{} x {} / {} cells",
                                COLUMNS,
                                ROWS,
                                COLUMNS * ROWS
                            ))),
                    )
                    .child(
                        div()
                            .border_2()
                            .border_color(rgb(0xb93737))
                            .bg(rgb(0xfffbf2))
                            .children((0..ROWS).map(|row| self.render_row(row))),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(760.0), px(760.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("dev.genko".into()),
                ..Default::default()
            },
            |_, cx| cx.new(|_| GenkoApp::new()),
        )
        .unwrap();
    });
}
