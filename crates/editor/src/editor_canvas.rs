use std::hash::{Hash, Hasher};
use std::ops::Range;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, Font, FontFeatures,
    GlobalElementId, IntoElement, LayoutId, Path, PathBuilder, Pixels, Style, TextAlign, TextRun,
    Window, fill, point, px, size,
};
use richtext::{BlockKind, InlineStyle, ResolvedBlock};
use rope::CellText;
use settings::{AppSettings, ColumnNumberMode};
use theme::{APP_FONT_FAMILY, Theme};

use crate::editor::{AUTOMATIC_ROWS_RESERVED_CELLS, Editor, RichTextDecorations};

#[cfg(target_os = "macos")]
static LOGGED_PROLONGED_SOUND_MARK_SHAPING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellPaintKind {
    Main,
    Attached,
    CornerTop,
    CornerBottom,
}

struct PaintState {
    visible_text: std::sync::Arc<[CellText]>,
    richtext_decorations: RichTextDecorations,
    selected_range: Range<usize>,
    marked_range: Option<Range<usize>>,
    block_selection: Option<crate::editor::BlockSelection>,
    cursor_index: usize,
    scroll_column: usize,
    scroll_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
}

#[derive(Clone)]
pub(crate) struct GridPathCache {
    bounds: Bounds<Pixels>,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    vertical_dashes: Option<Path<Pixels>>,
    horizontal_dashes: Option<Path<Pixels>>,
}

pub(crate) struct EditorCanvas {
    editor: Entity<Editor>,
}

impl EditorCanvas {
    pub(crate) fn new(editor: Entity<Editor>) -> Self {
        Self { editor }
    }
}

impl IntoElement for EditorCanvas {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorCanvas {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let editor = self.editor.read(cx);
        let header_height = column_number_header_height(
            AppSettings::global(cx).column_number_mode,
            editor.cell_size(),
        );
        let mut style = Style::default();
        style.size.width = board_width_for_columns(
            editor.visible_columns(),
            editor.cell_size(),
            editor.ruby_gutter_size(),
        )
        .into();
        style.size.height =
            (px(editor.cell_size() * editor.visible_rows() as f32) + header_height).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let column_number_mode = AppSettings::global(cx).column_number_mode;
        let header_height = {
            let editor = self.editor.read(cx);
            column_number_header_height(column_number_mode, editor.cell_size())
        };
        let content_bounds = Bounds::new(
            point(bounds.left(), bounds.top() + header_height),
            size(bounds.size.width, bounds.size.height - header_height),
        );
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        self.editor.update(cx, |editor, _cx| {
            editor.last_board_bounds = Some(content_bounds);
        });

        let show_grid = AppSettings::global(cx).show_grid_lines;
        let paint_state = self.editor.update(cx, |editor, _cx| PaintState {
            visible_text: editor.visible_text(),
            richtext_decorations: editor.richtext_decorations.clone(),
            selected_range: editor.selected_range.clone(),
            marked_range: editor.marked_range.clone(),
            block_selection: editor.block_selection,
            cursor_index: editor.cursor_cell,
            scroll_column: editor.scroll_column,
            scroll_row: editor.scroll_row,
            rows_per_column: editor.rows_per_column(),
            visible_columns: editor.visible_columns(),
            visible_rows: editor.visible_rows(),
            cell_size: editor.cell_size(),
            ruby_gutter_size: editor.ruby_gutter_size(),
        });
        let PaintState {
            visible_text,
            richtext_decorations,
            selected_range,
            marked_range,
            block_selection,
            cursor_index,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
        } = paint_state;

        paint_paper(bounds, window, cx);
        paint_column_numbers(
            content_bounds,
            column_number_mode,
            scroll_column,
            visible_columns,
            cell_size,
            ruby_gutter_size,
            window,
            cx,
        );
        paint_selection(
            &visible_text,
            &selected_range,
            marked_range.as_ref(),
            block_selection,
            content_bounds,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
            window,
            cx,
        );
        if show_grid {
            let grid_cache = self.editor.update(cx, |editor, _cx| {
                let needs_rebuild = editor.grid_path_cache.as_ref().is_none_or(|cache| {
                    cache.bounds != content_bounds
                        || cache.visible_columns != visible_columns
                        || cache.visible_rows != visible_rows
                        || cache.cell_size != cell_size
                        || cache.ruby_gutter_size != ruby_gutter_size
                });
                if needs_rebuild {
                    editor.grid_path_cache = Some(build_grid_path_cache(
                        content_bounds,
                        visible_columns,
                        visible_rows,
                        cell_size,
                        ruby_gutter_size,
                    ));
                }
                editor.grid_path_cache.as_ref().unwrap().clone()
            });
            paint_grid(
                content_bounds,
                rows_per_column,
                visible_columns,
                scroll_row,
                visible_rows,
                cell_size,
                ruby_gutter_size,
                &grid_cache,
                window,
                cx,
            );
        }
        paint_text(
            &visible_text,
            &richtext_decorations,
            content_bounds,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
            window,
            cx,
        );
        paint_strikethrough_overlay(
            &visible_text,
            &richtext_decorations,
            content_bounds,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
            window,
            cx,
        );
        if focus_handle.is_focused(window) {
            paint_cursor(
                cursor_index,
                content_bounds,
                scroll_column,
                scroll_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
                window,
                cx,
            );
        }
    }
}

fn paint_paper(bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    window.paint_quad(fill(bounds, Theme::global(cx).bg_primary()));
}

fn column_number_header_height(mode: ColumnNumberMode, cell_size: f32) -> Pixels {
    if mode == ColumnNumberMode::Hidden {
        Pixels::ZERO
    } else {
        px((cell_size * 0.8).round().max(18.0))
    }
}

fn paint_column_numbers(
    content_bounds: Bounds<Pixels>,
    mode: ColumnNumberMode,
    scroll_column: usize,
    visible_columns: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if mode == ColumnNumberMode::Hidden {
        return;
    }

    let style = window.text_style();
    let font_size = px((cell_size * 0.4).round().max(11.0));
    let line_height = px((cell_size * 0.5).round().max(14.0));
    let text_top = content_bounds.top() - line_height - px(2.0);

    for column in 0..visible_columns {
        let logical_column = scroll_column + (visible_columns - 1 - column);
        let column_number = logical_column + 1;
        if !mode.should_show(column_number) {
            continue;
        }

        let label = column_number.to_string();
        let run = TextRun {
            len: label.len(),
            font: {
                let mut font = style.font();
                font.family = APP_FONT_FAMILY.into();
                font
            },
            color: Theme::global(cx).text_primary().into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(label.clone().into(), font_size, &[run], None);
        let column_left =
            board_x_for_visible_column(content_bounds.left(), column, cell_size, ruby_gutter_size);
        let text_origin = point(column_left + (px(cell_size) - line.width) / 2.0, text_top);
        line.paint(
            text_origin,
            line_height,
            TextAlign::Center,
            None,
            window,
            cx,
        )
        .ok();
    }
}

fn paint_grid(
    bounds: Bounds<Pixels>,
    _rows_per_column: usize,
    visible_columns: usize,
    _first_visible_row: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    grid_cache: &GridPathCache,
    window: &mut Window,
    cx: &mut App,
) {
    let bottom_border_y = bounds.top() + px(visible_rows as f32 * cell_size);
    let right_border_x = bounds.left()
        + board_width_for_columns(visible_columns, cell_size, ruby_gutter_size)
        - px(1.0);

    let grid_line_color = Theme::global(cx).primary();
    window.paint_quad(fill(
        Bounds::new(
            point(bounds.left(), bounds.top()),
            size(bounds.size.width, px(1.0)),
        ),
        grid_line_color,
    ));
    window.paint_quad(fill(
        Bounds::new(
            point(bounds.left(), bottom_border_y),
            size(bounds.size.width, px(1.0)),
        ),
        grid_line_color,
    ));
    window.paint_quad(fill(
        Bounds::new(
            point(bounds.left(), bounds.top()),
            size(px(1.0), bounds.size.height),
        ),
        grid_line_color,
    ));
    window.paint_quad(fill(
        Bounds::new(
            point(right_border_x, bounds.top()),
            size(px(1.0), bounds.size.height),
        ),
        grid_line_color,
    ));

    for column in 0..visible_columns {
        let column_left =
            board_x_for_visible_column(bounds.left(), column, cell_size, ruby_gutter_size);
        let column_right = column_left + px(cell_size);

        window.paint_quad(fill(
            Bounds::new(
                point(column_right, bounds.top()),
                size(px(ruby_gutter_size), px(1.0)),
            ),
            grid_line_color,
        ));
        window.paint_quad(fill(
            Bounds::new(
                point(column_right, bottom_border_y),
                size(px(ruby_gutter_size), px(1.0)),
            ),
            grid_line_color,
        ));
    }

    if let Some(path) = &grid_cache.vertical_dashes {
        window.paint_path(path.clone(), grid_line_color);
    }
    if let Some(path) = &grid_cache.horizontal_dashes {
        window.paint_path(path.clone(), grid_line_color);
    }
}

fn build_grid_path_cache(
    bounds: Bounds<Pixels>,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> GridPathCache {
    let mut vertical_dashes = PathBuilder::stroke(px(1.0)).dash_array(&[px(2.0), px(2.0)]);
    let mut horizontal_dashes = PathBuilder::stroke(px(1.0)).dash_array(&[px(2.0), px(2.0)]);

    for column in 0..visible_columns {
        let column_left =
            board_x_for_visible_column(bounds.left(), column, cell_size, ruby_gutter_size);
        let column_right = column_left + px(cell_size);

        if column > 0 {
            let x = column_left + px(0.5);
            vertical_dashes.move_to(point(x, bounds.top()));
            vertical_dashes.line_to(point(x, bounds.bottom()));
        }
        let x = column_right + px(0.5);
        vertical_dashes.move_to(point(x, bounds.top()));
        vertical_dashes.line_to(point(x, bounds.bottom()));

        for row in 1..visible_rows {
            let y = bounds.top() + px(row as f32 * cell_size);
            horizontal_dashes.move_to(point(column_left, y + px(0.5)));
            horizontal_dashes.line_to(point(column_left + px(cell_size), y + px(0.5)));
        }
    }

    GridPathCache {
        bounds,
        visible_columns,
        visible_rows,
        cell_size,
        ruby_gutter_size,
        vertical_dashes: vertical_dashes.build().ok(),
        horizontal_dashes: horizontal_dashes.build().ok(),
    }
}

fn paint_selection(
    visible_text: &[CellText],
    selected_range: &Range<usize>,
    marked_range: Option<&Range<usize>>,
    block_selection: Option<crate::editor::BlockSelection>,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(block_selection) = block_selection {
        for logical_index in block_selection_indices(
            block_selection.anchor_cell,
            block_selection.cursor_cell,
            rows_per_column,
        ) {
            let Some(cell_bounds) = cell_bounds_for_logical_index(
                bounds,
                logical_index,
                scroll_column,
                first_visible_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
            ) else {
                continue;
            };
            window.paint_quad(fill(cell_bounds, Theme::global(cx).bg_senodary()));
        }
    }

    for cell_text in visible_text {
        if ranges_overlap(&cell_text.range, selected_range) {
            let Some(cell_bounds) = cell_bounds_for_logical_index(
                bounds,
                cell_text.logical_index,
                scroll_column,
                first_visible_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
            ) else {
                continue;
            };
            window.paint_quad(fill(cell_bounds, Theme::global(cx).bg_senodary()));
        } else if marked_range.is_some_and(|range| ranges_overlap(&cell_text.range, range)) {
            let Some(cell_bounds) = cell_bounds_for_logical_index(
                bounds,
                cell_text.logical_index,
                scroll_column,
                first_visible_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
            ) else {
                continue;
            };
            let underline_y = cell_bounds.bottom() - px(4.0);
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        cell_bounds.left() + px((cell_size * 0.18).round()),
                        underline_y,
                    ),
                    size(px((cell_size * 0.64).round()), px(2.0)),
                ),
                Theme::global(cx).text_senodary(),
            ));
        }
    }
}

fn paint_text(
    visible_text: &[CellText],
    richtext_decorations: &RichTextDecorations,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    for cell_text in visible_text {
        let Some(cell_bounds) = cell_bounds_for_logical_index(
            bounds,
            cell_text.logical_index,
            scroll_column,
            first_visible_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
        ) else {
            continue;
        };

        match cell_paint_kind(cell_text) {
            CellPaintKind::Main => paint_cell_text(
                cell_text,
                cell_bounds,
                cell_size,
                rich_style_for_range(richtext_decorations, cell_text.range.clone()),
                window,
                cx,
            ),
            CellPaintKind::Attached => {
                paint_attached_punctuation(cell_text, cell_bounds, window, cx)
            }
            CellPaintKind::CornerTop => {
                paint_corner_punctuation(cell_text, cell_bounds, window, cx, true)
            }
            CellPaintKind::CornerBottom => {
                paint_corner_punctuation(cell_text, cell_bounds, window, cx, false)
            }
        }
    }
}

fn paint_cell_text(
    cell_text: &CellText,
    cell_bounds: Bounds<Pixels>,
    cell_size: f32,
    rich_style: CellRichStyle,
    window: &mut Window,
    cx: &mut App,
) {
    let style = window.text_style();
    let font_size = px((cell_size * rich_style.font_scale()).round());
    let line_height = px((cell_size * 0.86).round());
    log_prolonged_sound_mark_shaping(cell_text, font_size, style.font(), window, cx);
    let line = shape_text(
        window,
        &cell_text.text,
        font_size,
        text_run(
            &cell_text.text,
            vertical_text_font(style.font()),
            rich_style.color(cx),
            cx,
        ),
    );
    let text_origin = point(
        cell_bounds.left() + (px(cell_size) - line.width) / 2.0,
        cell_bounds.top() + (px(cell_size) - line_height) / 2.0,
    );
    line.paint(
        text_origin,
        line_height,
        TextAlign::Center,
        None,
        window,
        cx,
    )
    .ok();
}

fn paint_strikethrough_overlay(
    visible_text: &[CellText],
    richtext_decorations: &RichTextDecorations,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let mut current_segment: Option<StrikeSegment> = None;

    for cell_text in visible_text {
        let rich_style = rich_style_for_range(richtext_decorations, cell_text.range.clone());
        let Some(cell_bounds) = cell_bounds_for_logical_index(
            bounds,
            cell_text.logical_index,
            scroll_column,
            first_visible_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            ruby_gutter_size,
        ) else {
            continue;
        };

        if !rich_style.strikethrough {
            flush_strike_segment(&mut current_segment, window, cx);
            continue;
        }

        let Some((row, column)) = row_column_for_logical_index(
            cell_text.logical_index,
            scroll_column,
            rows_per_column,
            visible_columns,
        ) else {
            flush_strike_segment(&mut current_segment, window, cx);
            continue;
        };

        match current_segment.as_mut() {
            Some(segment)
                if segment.column == column
                    && segment.last_row + 1 == row
                    && segment.style == rich_style =>
            {
                segment.last_row = row;
                segment.end_bounds = cell_bounds;
            }
            _ => {
                flush_strike_segment(&mut current_segment, window, cx);
                current_segment = Some(StrikeSegment {
                    column,
                    first_row: row,
                    last_row: row,
                    style: rich_style,
                    start_bounds: cell_bounds,
                    end_bounds: cell_bounds,
                });
            }
        }
    }

    flush_strike_segment(&mut current_segment, window, cx);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StrikeSegment {
    column: usize,
    first_row: usize,
    last_row: usize,
    style: CellRichStyle,
    start_bounds: Bounds<Pixels>,
    end_bounds: Bounds<Pixels>,
}

fn flush_strike_segment(segment: &mut Option<StrikeSegment>, window: &mut Window, cx: &mut App) {
    let Some(segment) = segment.take() else {
        return;
    };

    let width = px(2.0);
    let inset = px((segment.start_bounds.size.width.as_f32() * 0.16).round());
    let center_x = segment.start_bounds.left() + segment.start_bounds.size.width / 2.0;
    let top = segment.start_bounds.top() + inset;
    let bottom = segment.end_bounds.bottom() - inset;
    let left = center_x - width / 2.0;
    window.paint_quad(fill(
        Bounds::new(point(left, top), size(width, bottom - top)),
        segment.style.color(cx),
    ));
}

fn paint_attached_punctuation(
    cell_text: &CellText,
    cell_bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let cell_size = cell_bounds.size.width.as_f32();
    let style = window.text_style();
    let font_size = px((cell_size * 0.5).round());
    let line_height = px((cell_size * 0.57).round());
    let line = shape_text(
        window,
        &cell_text.text,
        font_size,
        text_run(
            &cell_text.text,
            vertical_text_font(style.font()),
            Theme::global(cx).text_primary(),
            cx,
        ),
    );
    let text_origin = point(
        cell_bounds.right() - line.width - px(3.0),
        cell_bounds.bottom() - line_height - px(1.0),
    );
    line.paint(
        text_origin,
        line_height,
        TextAlign::Center,
        None,
        window,
        cx,
    )
    .ok();
}

fn is_corner_punctuation(text: &str) -> bool {
    matches!(text, "。" | "、")
}

fn paint_corner_punctuation(
    cell_text: &CellText,
    cell_bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
    align_top: bool,
) {
    let cell_size = cell_bounds.size.width.as_f32();
    let style = window.text_style();
    let font_size = px((cell_size * 0.5).round());
    let line_height = px((cell_size * 0.57).round());
    let line = shape_text(
        window,
        &cell_text.text,
        font_size,
        text_run(
            &cell_text.text,
            punctuation_text_font(style.font()),
            Theme::global(cx).text_primary(),
            cx,
        ),
    );
    let text_origin = point(
        cell_bounds.left() + (px(cell_size) - line.width) / 2.0,
        if align_top {
            cell_bounds.top() + (px(cell_size) - line_height) / 2.0
        } else {
            cell_bounds.bottom() - (px(cell_size) - line_height) / 2.0
        },
    );
    line.paint(
        text_origin,
        line_height,
        TextAlign::Center,
        None,
        window,
        cx,
    )
    .ok();
}

fn vertical_text_font(mut font: Font) -> Font {
    font.family = APP_FONT_FAMILY.into();
    font.features = FontFeatures::vertical_alternates();
    font
}

fn punctuation_text_font(mut font: Font) -> Font {
    font.family = APP_FONT_FAMILY.into();
    font.features = FontFeatures::default();
    font
}

fn cell_paint_kind(cell_text: &CellText) -> CellPaintKind {
    if is_corner_punctuation(&cell_text.text) {
        if cell_text.attached_to_previous {
            CellPaintKind::CornerBottom
        } else {
            CellPaintKind::CornerTop
        }
    } else if cell_text.attached_to_previous {
        CellPaintKind::Attached
    } else {
        CellPaintKind::Main
    }
}

fn text_run(text: &str, font: Font, color: gpui::Rgba, _cx: &mut App) -> TextRun {
    TextRun {
        len: text.len(),
        font,
        color: color.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CellRichStyle {
    bold: bool,
    strikethrough: bool,
    block_kind: BlockKind,
}

impl CellRichStyle {
    fn font_scale(self) -> f32 {
        match self.block_kind {
            BlockKind::HeadingLarge => 0.94,
            BlockKind::HeadingMedium => 0.84,
            BlockKind::Body => {
                if self.bold {
                    0.8
                } else {
                    0.75
                }
            }
        }
    }

    fn color(self, cx: &App) -> gpui::Rgba {
        match self.block_kind {
            BlockKind::HeadingLarge => Theme::global(cx).text_primary(),
            BlockKind::HeadingMedium => mix(
                Theme::global(cx).text_primary(),
                Theme::global(cx).primary(),
                0.22,
            ),
            BlockKind::Body => {
                if self.bold {
                    mix(
                        Theme::global(cx).text_primary(),
                        Theme::global(cx).black(),
                        0.12,
                    )
                } else {
                    Theme::global(cx).text_primary()
                }
            }
        }
    }
}

fn rich_style_for_range(decorations: &RichTextDecorations, range: Range<usize>) -> CellRichStyle {
    let mut style = CellRichStyle::default();
    for mark in &decorations.inline_marks {
        if mark.start < range.end && range.start < mark.end {
            match mark.style {
                InlineStyle::Bold => style.bold = true,
                InlineStyle::Strikethrough => style.strikethrough = true,
            }
        }
    }
    style.block_kind = block_kind_for_range(&decorations.blocks, &range);
    style
}

fn block_kind_for_range(blocks: &[ResolvedBlock], range: &Range<usize>) -> BlockKind {
    blocks
        .iter()
        .find(|block| block.range.start < range.end && range.start < block.range.end)
        .map(|block| block.kind)
        .unwrap_or(BlockKind::Body)
}

fn shape_text(
    window: &mut Window,
    text: &str,
    font_size: Pixels,
    run: TextRun,
) -> gpui::ShapedLine {
    let text_hash = text_layout_hash(text);
    window
        .text_system()
        .shape_line_by_hash(text_hash, text.len(), font_size, &[run], None, || {
            text.to_owned().into()
        })
}

fn log_prolonged_sound_mark_shaping(
    _cell_text: &CellText,
    _font_size: Pixels,
    _font: Font,
    _window: &mut Window,
    _cx: &mut App,
) {
    #[cfg(target_os = "macos")]
    {
        if _cell_text.text != "ー"
            || LOGGED_PROLONGED_SOUND_MARK_SHAPING.swap(true, Ordering::Relaxed)
        {
            return;
        }

        let plain_line = shape_text(
            _window,
            &_cell_text.text,
            _font_size,
            text_run(
                &_cell_text.text,
                punctuation_text_font(_font.clone()),
                Theme::global(_cx).text_primary(),
                _cx,
            ),
        );
        let vertical_line = shape_text(
            _window,
            &_cell_text.text,
            _font_size,
            text_run(
                &_cell_text.text,
                vertical_text_font(_font),
                Theme::global(_cx).text_primary(),
                _cx,
            ),
        );

        let plain_glyph_ids: Vec<u32> = plain_line
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id.0))
            .collect();
        let vertical_glyph_ids: Vec<u32> = vertical_line
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id.0))
            .collect();
        let plain_glyph_positions: Vec<(f32, f32)> = plain_line
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|glyph| (glyph.position.x.as_f32(), glyph.position.y.as_f32()))
            })
            .collect();
        let vertical_glyph_positions: Vec<(f32, f32)> = vertical_line
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|glyph| (glyph.position.x.as_f32(), glyph.position.y.as_f32()))
            })
            .collect();
        let plain_font_ids: Vec<usize> = plain_line.runs.iter().map(|run| run.font_id.0).collect();
        let vertical_font_ids: Vec<usize> =
            vertical_line.runs.iter().map(|run| run.font_id.0).collect();

        eprintln!(
            "soukou vertical shaping debug: text={:?} plain_font_ids={:?} vertical_font_ids={:?} plain_glyph_ids={:?} vertical_glyph_ids={:?} plain_glyph_positions={:?} vertical_glyph_positions={:?} plain_width={:?} vertical_width={:?}",
            _cell_text.text,
            plain_font_ids,
            vertical_font_ids,
            plain_glyph_ids,
            vertical_glyph_ids,
            plain_glyph_positions,
            vertical_glyph_positions,
            plain_line.width(),
            vertical_line.width(),
        );
    }
}

fn text_layout_hash(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
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

fn paint_cursor(
    cursor_index: usize,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(cell_bounds) = cell_bounds_for_logical_index(
        bounds,
        cursor_index,
        scroll_column,
        first_visible_row,
        rows_per_column,
        visible_columns,
        visible_rows,
        cell_size,
        ruby_gutter_size,
    ) else {
        return;
    };
    window.paint_quad(fill(
        Bounds::new(
            point(cell_bounds.left() + px(4.0), cell_bounds.top() + px(3.0)),
            size(px(cell_size - 8.0), px(2.0)),
        ),
        Theme::global(cx).text_primary(),
    ));
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn block_selection_indices(
    anchor_cell: usize,
    cursor_cell: usize,
    rows_per_column: usize,
) -> impl Iterator<Item = usize> {
    let rows_per_column = rows_per_column.max(1);
    let anchor_row = anchor_cell % rows_per_column;
    let anchor_column = anchor_cell / rows_per_column;
    let cursor_row = cursor_cell % rows_per_column;
    let cursor_column = cursor_cell / rows_per_column;
    let row_start = anchor_row.min(cursor_row);
    let row_end = anchor_row.max(cursor_row);
    let column_start = anchor_column.min(cursor_column);
    let column_end = anchor_column.max(cursor_column);

    (column_start..=column_end).flat_map(move |column| {
        (row_start..=row_end).map(move |row| column * rows_per_column + row)
    })
}

fn row_column_for_logical_index(
    logical_index: usize,
    first_visible_column: usize,
    rows_per_column: usize,
    visible_columns: usize,
) -> Option<(usize, usize)> {
    let rows_per_column = rows_per_column.max(1);
    let visible_columns = visible_columns.max(1);
    let logical_column = logical_index / rows_per_column;
    if logical_column < first_visible_column {
        return None;
    }

    let column_from_right = logical_column - first_visible_column;
    if column_from_right >= visible_columns {
        return None;
    }

    let row = logical_index % rows_per_column;
    let column = visible_columns - 1 - column_from_right;
    Some((row, column))
}

pub(crate) fn cell_bounds_for_logical_index(
    board_bounds: Bounds<Pixels>,
    logical_index: usize,
    first_visible_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Option<Bounds<Pixels>> {
    let (row, column) = row_column_for_logical_index(
        logical_index,
        first_visible_column,
        rows_per_column,
        visible_columns,
    )?;
    if row < first_visible_row || row >= first_visible_row + visible_rows {
        return None;
    }
    Some(Bounds::new(
        point(
            board_x_for_visible_column(board_bounds.left(), column, cell_size, ruby_gutter_size),
            board_bounds.top() + px((row - first_visible_row) as f32 * cell_size),
        ),
        size(px(cell_size), px(cell_size)),
    ))
}

pub(crate) fn logical_index_for_point(
    board_bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
    first_visible_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Option<usize> {
    let rows_per_column = rows_per_column.max(1);
    let visible_columns = visible_columns.max(1);
    if !board_bounds.contains(&position) {
        return None;
    }

    let local_x = position.x - board_bounds.left();
    let stride = px(cell_size + ruby_gutter_size);
    let column = (local_x / stride)
        .floor()
        .clamp(0.0, (visible_columns - 1) as f32) as usize;
    let column_offset = local_x - px(column as f32 * (cell_size + ruby_gutter_size));
    if column_offset > px(cell_size) {
        return None;
    }
    let row = ((position.y - board_bounds.top()) / px(cell_size))
        .floor()
        .clamp(0.0, (visible_rows.saturating_sub(1)) as f32) as usize;
    let column_from_right = visible_columns - 1 - column;
    Some((first_visible_column + column_from_right) * rows_per_column + first_visible_row + row)
}

fn board_width_for_columns(
    visible_columns: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Pixels {
    if visible_columns == 0 {
        return Pixels::ZERO;
    }

    px(visible_columns as f32 * (cell_size + ruby_gutter_size))
}

fn board_x_for_visible_column(
    board_left: Pixels,
    column: usize,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> Pixels {
    board_left + px(column as f32 * (cell_size + ruby_gutter_size))
}

pub(crate) fn visible_columns_for_window_width(
    width: Pixels,
    cell_size: f32,
    ruby_gutter_size: f32,
) -> usize {
    ((width / px(cell_size + ruby_gutter_size)).floor() as usize)
        .saturating_sub(2)
        .max(1)
}

pub(crate) fn rows_per_column_for_window_height(height: Pixels, cell_size: f32) -> usize {
    ((height / px(cell_size)).floor() as usize)
        .saturating_sub(AUTOMATIC_ROWS_RESERVED_CELLS)
        .clamp(1, AppSettings::max_rows_per_column())
}

pub(crate) fn content_height_for_window_height(
    height: Pixels,
    mode: ColumnNumberMode,
    cell_size: f32,
) -> Pixels {
    (height - column_number_header_height(mode, cell_size)).max(Pixels::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_ROWS_PER_COLUMN: usize = 16;
    const VISIBLE_COLUMNS: usize = 20;
    const TEST_CELL_SIZE: f32 = 28.0;
    const TEST_RUBY_GUTTER_SIZE: f32 = TEST_CELL_SIZE * crate::editor::RUBY_GUTTER_RATIO;

    #[test]
    fn vertical_flow_starts_at_top_right() {
        let rows = DEFAULT_ROWS_PER_COLUMN;

        assert_eq!(
            row_column_for_logical_index(0, 0, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 1))
        );
        assert_eq!(
            row_column_for_logical_index(1, 0, rows, VISIBLE_COLUMNS),
            Some((1, VISIBLE_COLUMNS - 1))
        );
        assert_eq!(
            row_column_for_logical_index(rows, 0, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 2))
        );
    }

    #[test]
    fn virtual_scroll_offsets_visible_columns() {
        let rows = DEFAULT_ROWS_PER_COLUMN;

        assert_eq!(
            row_column_for_logical_index(0, 1, rows, VISIBLE_COLUMNS),
            None
        );
        assert_eq!(
            row_column_for_logical_index(rows, 1, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 1))
        );
        assert_eq!(
            row_column_for_logical_index(rows * VISIBLE_COLUMNS, 1, rows, VISIBLE_COLUMNS),
            Some((0, 0))
        );
        assert_eq!(
            row_column_for_logical_index(rows * (VISIBLE_COLUMNS + 1), 1, rows, VISIBLE_COLUMNS),
            None
        );
    }

    #[test]
    fn vertical_flow_uses_configured_rows_per_column() {
        let rows = 24;

        assert_eq!(
            row_column_for_logical_index(rows, 0, rows, VISIBLE_COLUMNS),
            Some((0, VISIBLE_COLUMNS - 2))
        );
        assert_eq!(
            row_column_for_logical_index(rows - 1, 0, rows, VISIBLE_COLUMNS),
            Some((rows - 1, VISIBLE_COLUMNS - 1))
        );
    }

    #[test]
    fn cell_bounds_leave_ruby_gutter_between_columns() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(200.0)));

        let left_column = cell_bounds_for_logical_index(
            bounds,
            DEFAULT_ROWS_PER_COLUMN,
            0,
            0,
            DEFAULT_ROWS_PER_COLUMN,
            2,
            DEFAULT_ROWS_PER_COLUMN,
            TEST_CELL_SIZE,
            TEST_RUBY_GUTTER_SIZE,
        )
        .unwrap();
        let right_column = cell_bounds_for_logical_index(
            bounds,
            0,
            0,
            0,
            DEFAULT_ROWS_PER_COLUMN,
            2,
            DEFAULT_ROWS_PER_COLUMN,
            TEST_CELL_SIZE,
            TEST_RUBY_GUTTER_SIZE,
        )
        .unwrap();

        assert_eq!(left_column.left(), px(0.0));
        assert_eq!(
            right_column.left(),
            px(TEST_CELL_SIZE + TEST_RUBY_GUTTER_SIZE)
        );
    }

    #[test]
    fn click_in_ruby_gutter_does_not_target_main_cell() {
        let bounds = Bounds::new(
            point(px(0.0), px(0.0)),
            size(
                board_width_for_columns(2, TEST_CELL_SIZE, TEST_RUBY_GUTTER_SIZE),
                px(200.0),
            ),
        );
        let gutter_point = point(px(TEST_CELL_SIZE + TEST_RUBY_GUTTER_SIZE / 2.0), px(8.0));

        assert_eq!(
            logical_index_for_point(
                bounds,
                gutter_point,
                0,
                0,
                DEFAULT_ROWS_PER_COLUMN,
                2,
                DEFAULT_ROWS_PER_COLUMN,
                TEST_CELL_SIZE,
                TEST_RUBY_GUTTER_SIZE,
            ),
            None
        );
    }

    #[test]
    fn board_width_includes_trailing_ruby_gutter() {
        assert_eq!(
            board_width_for_columns(2, TEST_CELL_SIZE, TEST_RUBY_GUTTER_SIZE),
            px(2.0 * (TEST_CELL_SIZE + TEST_RUBY_GUTTER_SIZE))
        );
    }

    #[test]
    fn click_in_trailing_ruby_gutter_does_not_target_main_cell() {
        let bounds = Bounds::new(
            point(px(0.0), px(0.0)),
            size(
                board_width_for_columns(2, TEST_CELL_SIZE, TEST_RUBY_GUTTER_SIZE),
                px(200.0),
            ),
        );
        let trailing_gutter_point = point(
            px(2.0 * TEST_CELL_SIZE + 1.5 * TEST_RUBY_GUTTER_SIZE),
            px(8.0),
        );

        assert_eq!(
            logical_index_for_point(
                bounds,
                trailing_gutter_point,
                0,
                0,
                DEFAULT_ROWS_PER_COLUMN,
                2,
                DEFAULT_ROWS_PER_COLUMN,
                TEST_CELL_SIZE,
                TEST_RUBY_GUTTER_SIZE,
            ),
            None
        );
    }

    #[test]
    fn cell_paint_kind_uses_corner_variants_for_kuten_touten() {
        let plain = CellText {
            logical_index: 0,
            text: "文".into(),
            range: 0..3,
            attached_to_previous: false,
        };
        let corner_top = CellText {
            logical_index: 1,
            text: "。".into(),
            range: 3..6,
            attached_to_previous: false,
        };
        let corner_bottom = CellText {
            logical_index: 1,
            text: "、".into(),
            range: 6..9,
            attached_to_previous: true,
        };

        assert_eq!(cell_paint_kind(&plain), CellPaintKind::Main);
        assert_eq!(cell_paint_kind(&corner_top), CellPaintKind::CornerTop);
        assert_eq!(cell_paint_kind(&corner_bottom), CellPaintKind::CornerBottom);
    }

    #[test]
    fn vertical_text_font_uses_app_font_with_vertical_alternates() {
        let font = vertical_text_font(Font::default());

        assert_eq!(font.family.as_ref(), APP_FONT_FAMILY);
        assert_eq!(font.features, FontFeatures::vertical_alternates());
    }

    #[test]
    fn punctuation_text_font_uses_app_font_without_vertical_alternates() {
        let font = punctuation_text_font(Font::default());

        assert_eq!(font.family.as_ref(), APP_FONT_FAMILY);
        assert_eq!(font.features, FontFeatures::default());
    }

    #[test]
    fn text_layout_hash_depends_on_text_content() {
        assert_eq!(text_layout_hash("文"), text_layout_hash("文"));
        assert_ne!(text_layout_hash("文"), text_layout_hash("字"));
    }
}
