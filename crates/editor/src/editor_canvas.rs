use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::time::Instant;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, Font, FontFeatures,
    GlobalElementId, IntoElement, LayoutId, Path, PathBuilder, Pixels, Style, TextAlign, TextRun,
    Window, fill, point, px, size,
};
use richtext::{BlockKind, InlineStyle, ResolvedBlock};
use rope::CellText;
use settings::{AppSettings, ColumnNumberMode};
use theme::{APP_FONT_FAMILY, Theme};

use crate::editor::layout::{
    cell_bounds_for_logical_index, column_number_header_height, row_column_for_logical_index,
};
use crate::editor::{Editor, RichTextDecorations};

use crate::perf::{PerfScope, log_paste_perf, paste_perf_enabled};

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

#[derive(Clone, Copy)]
struct PreparedCellPaint {
    bounds: Option<Bounds<Pixels>>,
    rich_style: CellRichStyle,
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
        let _paint_perf = paste_perf_enabled().then(|| {
            let visible_columns = self.editor.read(cx).visible_columns();
            let visible_rows = self.editor.read(cx).visible_rows();
            PerfScope::new(move |elapsed| {
                log_paste_perf(
                    "editor_canvas.paint",
                    move || format!("cols={} rows={}", visible_columns, visible_rows),
                    elapsed,
                );
            })
        });
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
        let prepared_cells = prepare_cell_paint_data(
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
        );

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
        paint_text(&visible_text, &prepared_cells, window, cx);
        paint_strikethrough_overlay(
            &visible_text,
            rows_per_column,
            scroll_column,
            visible_columns,
            &prepared_cells,
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
    prepared_cells: &[PreparedCellPaint],
    window: &mut Window,
    cx: &mut App,
) {
    for (cell_text, prepared) in visible_text.iter().zip(prepared_cells.iter()) {
        let Some(cell_bounds) = prepared.bounds else {
            continue;
        };

        match cell_paint_kind(cell_text) {
            CellPaintKind::Main => paint_cell_text(
                cell_text,
                cell_bounds,
                cell_bounds.size.width.as_f32(),
                prepared.rich_style,
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
    let paint_offset = vertical_text_paint_offset(&line);
    let text_origin = point(
        cell_bounds.left() + (px(cell_size) - line.width) / 2.0 + paint_offset.x,
        cell_bounds.top() + (px(cell_size) - line_height) / 2.0 + paint_offset.y,
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
    scroll_column: usize,
    rows_per_column: usize,
    visible_columns: usize,
    prepared_cells: &[PreparedCellPaint],
    window: &mut Window,
    cx: &mut App,
) {
    let mut current_segment: Option<StrikeSegment> = None;

    for (cell_text, prepared) in visible_text.iter().zip(prepared_cells.iter()) {
        let Some(cell_bounds) = prepared.bounds else {
            continue;
        };

        if !prepared.rich_style.strikethrough {
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
                    && segment.style == prepared.rich_style =>
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
                    style: prepared.rich_style,
                    start_bounds: cell_bounds,
                    end_bounds: cell_bounds,
                });
            }
        }
    }

    flush_strike_segment(&mut current_segment, window, cx);
}

fn prepare_cell_paint_data(
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
) -> Vec<PreparedCellPaint> {
    let perf_enabled = paste_perf_enabled();
    let perf_start = perf_enabled.then(Instant::now);
    let prepared: Vec<PreparedCellPaint> = visible_text
        .iter()
        .map(|cell_text| PreparedCellPaint {
            bounds: cell_bounds_for_logical_index(
                bounds,
                cell_text.logical_index,
                scroll_column,
                first_visible_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                ruby_gutter_size,
            ),
            rich_style: rich_style_for_range(richtext_decorations, &cell_text.range),
        })
        .collect();
    if let Some(start) = perf_start {
        log_paste_perf(
            "prepare_cell_paint_data",
            || {
                format!(
                    "cells={} cols={} rows={} inline_marks={} blocks={} cell_size={:.1}",
                    prepared.len(),
                    visible_columns,
                    visible_rows,
                    richtext_decorations.inline_marks.len(),
                    richtext_decorations.blocks.len(),
                    cell_size
                )
            },
            start.elapsed(),
        );
    }
    prepared
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
    let paint_offset = vertical_text_paint_offset(&line);
    let text_origin = point(
        cell_bounds.right() - line.width - px(3.0) + paint_offset.x,
        cell_bounds.bottom() - line_height - px(1.0) + paint_offset.y,
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
            vertical_text_font(style.font()),
            Theme::global(cx).text_primary(),
            cx,
        ),
    );
    let paint_offset = vertical_text_paint_offset(&line);
    let text_origin = point(
        cell_bounds.left() + (px(cell_size) - line.width) / 2.0 + paint_offset.x,
        if align_top {
            cell_bounds.top() + (px(cell_size) - line_height) / 2.0 + paint_offset.y
        } else {
            cell_bounds.bottom() - (px(cell_size) - line_height) / 2.0 + paint_offset.y
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

fn rich_style_for_range(decorations: &RichTextDecorations, range: &Range<usize>) -> CellRichStyle {
    let mut style = CellRichStyle::default();
    for mark in &decorations.inline_marks {
        if mark.start < range.end && range.start < mark.end {
            match mark.style {
                InlineStyle::Bold => style.bold = true,
                InlineStyle::Strikethrough => style.strikethrough = true,
            }
        }
    }
    style.block_kind = block_kind_for_range(&decorations.blocks, range);
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

fn vertical_text_paint_offset(line: &gpui::ShapedLine) -> gpui::Point<Pixels> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = line;
        point(px(0.0), px(0.0))
    }

    #[cfg(target_os = "macos")]
    {
        let (min_x, min_y) = line.runs.iter().flat_map(|run| run.glyphs.iter()).fold(
            (0.0f32, 0.0f32),
            |(min_x, min_y), glyph| {
                (
                    min_x.min(glyph.position.x.as_f32()),
                    min_y.min(glyph.position.y.as_f32()),
                )
            },
        );

        point(px(-min_x), px(-min_y))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::layout::logical_index_for_point;

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
    fn text_layout_hash_depends_on_text_content() {
        assert_eq!(text_layout_hash("文"), text_layout_hash("文"));
        assert_ne!(text_layout_hash("文"), text_layout_hash("字"));
    }
}
