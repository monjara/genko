use std::hash::{Hash, Hasher};
use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, Font, FontFeatures,
    GlobalElementId, IntoElement, LayoutId, Path, PathBuilder, Pixels, Style, TextAlign, TextRun,
    Window, fill, point, px, size,
};
use rope::CellText;
use settings::{AppSettings, ColumnNumberMode};
use theme::{APP_FONT_FAMILY, Theme};

use crate::editor::Editor;
#[cfg(test)]
use crate::editor::layout::row_column_for_logical_index;
use crate::editor::layout::{cell_bounds_for_logical_index, column_number_header_height};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellPaintKind {
    Main,
    Attached,
    CornerTop,
    CornerBottom,
}

struct PaintState {
    visible_text: std::sync::Arc<[CellText]>,
    selected_range: Range<usize>,
    selected_cell_range: Option<Range<usize>>,
    marked_range: Option<Range<usize>>,
    block_selection: Option<crate::editor::BlockSelection>,
    cursor_index: usize,
    scroll_column: usize,
    scroll_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    search_matches: Vec<Range<usize>>,
    search_current_match: Option<Range<usize>>,
}

#[derive(Clone, Copy)]
struct PreparedCellPaint {
    bounds: Option<Bounds<Pixels>>,
    text_style: CellTextStyle,
}

#[derive(Clone)]
pub(crate) struct GridPathCache {
    bounds: Bounds<Pixels>,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    column_gap: f32,
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
            editor.column_gap_size(),
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
        let paint_state = self.editor.update(cx, |editor, _cx| {
            let (search_matches, search_current_match) = editor
                .search_state()
                .map(|state| (state.matches.clone(), state.current_match_range().cloned()))
                .unwrap_or_default();
            PaintState {
                visible_text: editor.visible_text(),
                selected_cell_range: selected_cell_range(editor, &editor.selected_range),
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
                search_matches,
                search_current_match,
            }
        });
        let PaintState {
            visible_text,
            selected_range,
            selected_cell_range,
            marked_range,
            block_selection,
            cursor_index,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            search_matches,
            search_current_match,
        } = paint_state;

        let column_gap = Editor::column_gap_for_cell_size(cell_size);
        let prepared_cells = prepare_cell_paint_data(
            &visible_text,
            content_bounds,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            column_gap,
            marked_range.as_ref(),
        );

        paint_paper(bounds, window, cx);
        paint_column_numbers(
            content_bounds,
            column_number_mode,
            scroll_column,
            visible_columns,
            cell_size,
            column_gap,
            window,
            cx,
        );
        paint_selection(
            &visible_text,
            &selected_range,
            selected_cell_range.as_ref(),
            marked_range.as_ref(),
            block_selection,
            content_bounds,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            column_gap,
            window,
            cx,
        );
        paint_search_matches(
            &visible_text,
            &search_matches,
            search_current_match.as_ref(),
            content_bounds,
            scroll_column,
            scroll_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            column_gap,
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
                        || cache.column_gap != column_gap
                });
                if needs_rebuild {
                    editor.grid_path_cache = Some(build_grid_path_cache(
                        content_bounds,
                        visible_columns,
                        visible_rows,
                        cell_size,
                        column_gap,
                    ));
                }
                match editor.grid_path_cache.as_ref() {
                    Some(cache) => cache.clone(),
                    None => {
                        let cache = build_grid_path_cache(
                            content_bounds,
                            visible_columns,
                            visible_rows,
                            cell_size,
                            column_gap,
                        );
                        editor.grid_path_cache = Some(cache.clone());
                        cache
                    }
                }
            });
            paint_grid(
                content_bounds,
                visible_columns,
                scroll_row,
                visible_rows,
                cell_size,
                column_gap,
                &grid_cache,
                window,
                cx,
            );
        }
        paint_text(&visible_text, &prepared_cells, window, cx);
        if theme::cursor_should_be_visible(cx) && focus_handle.is_focused(window) {
            paint_cursor(
                cursor_index,
                content_bounds,
                scroll_column,
                scroll_row,
                rows_per_column,
                visible_columns,
                visible_rows,
                cell_size,
                column_gap,
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
    column_gap: f32,
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
        if !should_show_column_number(mode, column_number) {
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
            board_x_for_visible_column(content_bounds.left(), column, cell_size, column_gap);
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

fn should_show_column_number(mode: ColumnNumberMode, column_number: usize) -> bool {
    match mode {
        ColumnNumberMode::Hidden => false,
        ColumnNumberMode::EveryFive => column_number.is_multiple_of(5),
        ColumnNumberMode::EveryTen => column_number.is_multiple_of(10),
        ColumnNumberMode::All => true,
    }
}

fn paint_grid(
    bounds: Bounds<Pixels>,
    visible_columns: usize,
    _first_visible_row: usize,
    visible_rows: usize,
    cell_size: f32,
    column_gap: f32,
    grid_cache: &GridPathCache,
    window: &mut Window,
    cx: &mut App,
) {
    let bottom_border_y = bounds.top() + px(visible_rows as f32 * cell_size);
    let right_border_x =
        bounds.left() + board_width_for_columns(visible_columns, cell_size, column_gap) - px(1.0);

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
        let column_left = board_x_for_visible_column(bounds.left(), column, cell_size, column_gap);
        let column_right = column_left + px(cell_size);

        window.paint_quad(fill(
            Bounds::new(
                point(column_right, bounds.top()),
                size(px(column_gap), px(1.0)),
            ),
            grid_line_color,
        ));
        window.paint_quad(fill(
            Bounds::new(
                point(column_right, bottom_border_y),
                size(px(column_gap), px(1.0)),
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
    column_gap: f32,
) -> GridPathCache {
    let mut vertical_dashes = PathBuilder::stroke(px(1.0)).dash_array(&[px(2.0), px(2.0)]);
    let mut horizontal_dashes = PathBuilder::stroke(px(1.0)).dash_array(&[px(2.0), px(2.0)]);

    for column in 0..visible_columns {
        let column_left = board_x_for_visible_column(bounds.left(), column, cell_size, column_gap);
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
        column_gap,
        vertical_dashes: vertical_dashes.build().ok(),
        horizontal_dashes: horizontal_dashes.build().ok(),
    }
}

fn paint_search_matches(
    visible_text: &[CellText],
    all_matches: &[Range<usize>],
    current_match: Option<&Range<usize>>,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    column_gap: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if all_matches.is_empty() {
        return;
    }
    let Some(visible_start) = visible_text.first().map(|c| c.range.start) else {
        return;
    };
    let Some(visible_end) = visible_text.last().map(|c| c.range.end) else {
        return;
    };

    // バイナリサーチで可視範囲に重なる最初のマッチを特定
    let first_relevant = all_matches.partition_point(|m| m.end <= visible_start);
    let visible_matches: &[Range<usize>] = {
        let end = all_matches[first_relevant..].partition_point(|m| m.start < visible_end);
        &all_matches[first_relevant..first_relevant + end]
    };

    if visible_matches.is_empty() {
        return;
    }

    let primary = Theme::global(cx).primary();
    let match_color: gpui::Hsla = gpui::Rgba {
        r: primary.r,
        g: primary.g,
        b: primary.b,
        a: 0.35,
    }
    .into();
    let current_color: gpui::Hsla = gpui::Rgba {
        r: primary.r,
        g: primary.g,
        b: primary.b,
        a: 0.70,
    }
    .into();

    for cell_text in visible_text {
        let is_any_match = visible_matches
            .iter()
            .any(|m| ranges_overlap(&cell_text.range, m));
        if !is_any_match {
            continue;
        }

        let Some(cell_bounds) = cell_bounds_for_logical_index(
            bounds,
            cell_text.logical_index,
            scroll_column,
            first_visible_row,
            rows_per_column,
            visible_columns,
            visible_rows,
            cell_size,
            column_gap,
        ) else {
            continue;
        };

        let is_current = current_match.is_some_and(|mr| ranges_overlap(&cell_text.range, mr));
        let color = if is_current {
            current_color
        } else {
            match_color
        };
        window.paint_quad(fill(cell_bounds, color));
    }
}

fn paint_selection(
    visible_text: &[CellText],
    selected_range: &Range<usize>,
    selected_cell_range: Option<&Range<usize>>,
    marked_range: Option<&Range<usize>>,
    block_selection: Option<crate::editor::BlockSelection>,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    column_gap: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(selected_cell_range) = selected_cell_range {
        for logical_index in visible_selected_indices(
            selected_cell_range,
            scroll_column,
            first_visible_row,
            rows_per_column,
            visible_columns,
            visible_rows,
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
                column_gap,
            ) else {
                continue;
            };
            window.paint_quad(fill(cell_bounds, Theme::global(cx).selection_range()));
        }
    }

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
                column_gap,
            ) else {
                continue;
            };
            window.paint_quad(fill(cell_bounds, Theme::global(cx).selection_range()));
        }
    }

    for cell_text in visible_text {
        if ranges_overlap(&cell_text.range, selected_range) {
            continue;
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
                column_gap,
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

fn selected_cell_range(editor: &Editor, selected_range: &Range<usize>) -> Option<Range<usize>> {
    if selected_range.is_empty() {
        return None;
    }

    let (start, end) = if editor.selection_reversed {
        let cursor_cell = editor.cursor_cell;
        let anchor_offset = editor.previous_boundary(selected_range.end);
        let anchor_cell = editor.display_cell_for_byte(anchor_offset);
        (
            cursor_cell.min(anchor_cell),
            cursor_cell.max(anchor_cell) + 1,
        )
    } else {
        let anchor_cell = editor.display_cell_for_byte(selected_range.start);
        let cursor_cell = editor.cursor_cell;
        let cursor_offset = editor.byte_offset_for_display_cell(cursor_cell);
        let includes_cursor_cell = editor.previous_boundary(selected_range.end) == cursor_offset;
        let end = if includes_cursor_cell {
            cursor_cell.saturating_add(1)
        } else {
            cursor_cell
        };
        (anchor_cell.min(end), anchor_cell.max(end))
    };

    (start < end).then_some(start..end)
}

fn visible_selected_indices(
    selected_cell_range: &Range<usize>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
) -> impl Iterator<Item = usize> {
    let rows_per_column = rows_per_column.max(1);
    let selected_start_column = selected_cell_range.start / rows_per_column;
    let selected_start_row = selected_cell_range.start % rows_per_column;
    let selected_end_cell = selected_cell_range.end.saturating_sub(1);
    let selected_end_column = selected_end_cell / rows_per_column;
    let selected_end_row = selected_end_cell % rows_per_column;
    let first_column = scroll_column.max(selected_start_column);
    let last_column_exclusive = scroll_column
        .saturating_add(visible_columns)
        .min(selected_end_column.saturating_add(1));
    let visible_end_row = first_visible_row.saturating_add(visible_rows);

    (first_column..last_column_exclusive).flat_map(move |column| {
        let first_row = if column == selected_start_column {
            selected_start_row
        } else {
            0
        }
        .max(first_visible_row);
        let last_row_exclusive = if column == selected_end_column {
            selected_end_row.saturating_add(1)
        } else {
            rows_per_column
        }
        .min(visible_end_row);

        (first_row..last_row_exclusive).map(move |row| column * rows_per_column + row)
    })
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
            CellPaintKind::Main => {
                let cell_size = cell_bounds.size.width.as_f32();
                paint_cell_text(
                    cell_text,
                    cell_bounds,
                    cell_size,
                    prepared.text_style,
                    window,
                    cx,
                );
            }
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
    text_style: CellTextStyle,
    window: &mut Window,
    cx: &mut App,
) {
    let style = window.text_style();
    let font_size = px((cell_size * text_style.font_scale()).round());
    let line_height = px((cell_size * 0.86).round());
    let line = shape_text(
        window,
        &cell_text.text,
        font_size,
        text_run(
            &cell_text.text,
            vertical_text_font(style.font()),
            text_style.color(cx),
            text_style,
            cx,
        ),
        text_style.layout_hash(),
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

fn prepare_cell_paint_data(
    visible_text: &[CellText],
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    column_gap: f32,
    marked_range: Option<&Range<usize>>,
) -> Vec<PreparedCellPaint> {
    visible_text
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
                column_gap,
            ),
            text_style: style_for_cell(cell_text, marked_range),
        })
        .collect()
}

fn style_for_cell(cell_text: &CellText, marked_range: Option<&Range<usize>>) -> CellTextStyle {
    if marked_range.is_some_and(|range| ranges_overlap(&cell_text.range, range)) {
        return CellTextStyle::default();
    }

    CellTextStyle::default()
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
            CellTextStyle::default(),
            cx,
        ),
        0,
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
            CellTextStyle::default(),
            cx,
        ),
        0,
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

fn text_run(
    text: &str,
    font: Font,
    color: gpui::Rgba,
    _text_style: CellTextStyle,
    _cx: &mut App,
) -> TextRun {
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
struct CellTextStyle;

impl CellTextStyle {
    fn font_scale(self) -> f32 {
        0.82
    }

    fn color(self, cx: &App) -> gpui::Rgba {
        Theme::global(cx).text_primary()
    }

    fn layout_hash(self) -> u64 {
        0
    }
}

fn shape_text(
    window: &mut Window,
    text: &str,
    font_size: Pixels,
    run: TextRun,
    layout_hash: u64,
) -> gpui::ShapedLine {
    let text_hash = text_layout_hash(text) ^ layout_hash;
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

fn paint_cursor(
    cursor_index: usize,
    bounds: Bounds<Pixels>,
    scroll_column: usize,
    first_visible_row: usize,
    rows_per_column: usize,
    visible_columns: usize,
    visible_rows: usize,
    cell_size: f32,
    column_gap: f32,
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
        column_gap,
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

fn board_width_for_columns(visible_columns: usize, cell_size: f32, column_gap: f32) -> Pixels {
    if visible_columns == 0 {
        return Pixels::ZERO;
    }

    px(visible_columns as f32 * (cell_size + column_gap))
}

fn board_x_for_visible_column(
    board_left: Pixels,
    column: usize,
    cell_size: f32,
    column_gap: f32,
) -> Pixels {
    board_left + px(column as f32 * (cell_size + column_gap))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_ROWS_PER_COLUMN: usize = 16;
    const VISIBLE_COLUMNS: usize = 20;

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
