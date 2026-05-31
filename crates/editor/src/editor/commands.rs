use std::ops::Range;
use std::time::Instant;

use gpui::{
    ClipboardItem, Context, MouseMoveEvent, MouseUpEvent, Pixels, ScrollWheelEvent, Window, px,
};
use settings::AppSettings;

use super::command_types::{MotionKind, TextObjectModifier, TextObjectTarget};
use super::motions::{
    MotionRangeBehavior, resolve_motion_range, resolve_motion_target, resolve_text_object_range,
};
use super::{Editor, invalidate_ime_position};
use crate::perf::{log_paste_perf, paste_perf_enabled};
use crate::vim::block::current_column_cell_range;

impl Editor {
    pub(crate) fn motion_target_command(&self, motion: MotionKind) -> Option<usize> {
        resolve_motion_target(self.rope(), self.cursor_byte_offset(), motion)
    }

    pub(crate) fn motion_range_command(
        &self,
        motion: MotionKind,
        behavior: MotionRangeBehavior,
    ) -> Option<Range<usize>> {
        resolve_motion_range(self.rope(), self.cursor_byte_offset(), motion, behavior)
    }

    pub(crate) fn text_object_range_command(
        &self,
        modifier: TextObjectModifier,
        target: TextObjectTarget,
    ) -> Option<Range<usize>> {
        resolve_text_object_range(self.rope(), self.cursor_byte_offset(), modifier, target)
    }

    pub(crate) fn move_cursor_by_motion_command(
        &mut self,
        motion: MotionKind,
        select: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.motion_target_command(motion) else {
            return false;
        };
        if select {
            self.select_cursor_to_byte_offset_command(target, cx);
        } else {
            self.move_cursor_to_byte_offset(target, cx);
        }
        true
    }

    pub(crate) fn move_cursor_to_document_start_command(&mut self, cx: &mut Context<Self>) {
        self.move_cursor_to_display_cell(0, cx);
    }

    pub(crate) fn move_cursor_to_document_end_command(&mut self, cx: &mut Context<Self>) {
        self.move_cursor_to_display_cell(self.used_cells(), cx);
    }

    pub(crate) fn next_column_start_cell_command(&self) -> usize {
        let rows_per_column = self.rows_per_column();
        let used_cells = self.used_cells();
        let current_cell = if used_cells == 0 {
            0
        } else if self.cursor_cell() >= used_cells {
            used_cells.saturating_sub(1)
        } else {
            self.cursor_cell()
        };
        ((current_cell / rows_per_column) + 1) * rows_per_column
    }

    pub(crate) fn move_cursor_to_next_column_start_command(&mut self, cx: &mut Context<Self>) {
        self.move_cursor_to_display_cell(self.next_column_start_cell_command(), cx);
    }

    pub(crate) fn current_line_byte_range_command(&self) -> Option<Range<usize>> {
        let cell_range = current_column_cell_range(
            self.cursor_cell(),
            self.rows_per_column(),
            self.used_cells(),
        )?;
        Some(
            self.byte_offset_for_display_cell(cell_range.start)
                ..self.byte_offset_for_display_cell(cell_range.end),
        )
    }

    pub(crate) fn move_cursor_by_cells_command(
        &mut self,
        delta: isize,
        select: bool,
        cx: &mut Context<Self>,
    ) {
        if select {
            self.select_cursor_by(delta, cx);
        } else {
            self.move_cursor_by(delta, cx);
        }
    }

    pub(crate) fn move_cursor_left_cell_command(&mut self, select: bool, cx: &mut Context<Self>) {
        self.move_cursor_by_cells_command(self.rows_per_column() as isize, select, cx);
    }

    pub(crate) fn move_cursor_right_cell_command(&mut self, select: bool, cx: &mut Context<Self>) {
        self.move_cursor_by_cells_command(-(self.rows_per_column() as isize), select, cx);
    }

    pub(crate) fn select_cursor_to_byte_offset_command(
        &mut self,
        byte_offset: usize,
        cx: &mut Context<Self>,
    ) {
        let cell_index = self.display_cell_for_byte(byte_offset);
        self.select_between_display_cells(self.selection_anchor_cell(), cell_index, cx);
    }

    pub(crate) fn delete_backward_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            self.selected_range = previous..self.cursor_offset();
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
        invalidate_ime_position(window);
    }

    pub(crate) fn delete_forward_command(&mut self, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            self.selected_range = self.cursor_offset()..next;
        }
        self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
    }

    pub(crate) fn select_all_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.draft.len_bytes();
        self.selection_reversed = false;
        self.cursor_cell = self.used_cells();
        self.block_selection = None;
        invalidate_ime_position(window);
        cx.notify();
    }

    pub(crate) fn paste_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let perf_enabled = paste_perf_enabled();
            let perf_start = perf_enabled.then(Instant::now);
            let pasted_bytes = text.len();
            self.replace_text_in_byte_range_owned(self.selected_range.clone(), text, cx);
            invalidate_ime_position(window);
            if let Some(start) = perf_start {
                let revision = self.draft_revision;
                let cursor_cell = self.cursor_cell;
                log_paste_perf(
                    "paste_total",
                    move || {
                        format!(
                            "pasted_bytes={} cursor_cell={} revision={}",
                            pasted_bytes, cursor_cell, revision
                        )
                    },
                    start.elapsed(),
                );
            }
        }
    }

    pub(crate) fn insert_newline_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let inserted_text = if AppSettings::global(cx).indent_on_enter {
            "\n "
        } else {
            "\n"
        };
        self.replace_text_in_byte_range(self.selected_range.clone(), inserted_text, cx);
        invalidate_ime_position(window);
    }

    pub(crate) fn clear_selection_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.block_selection.is_some() {
            self.clear_block_selection(cx);
            invalidate_ime_position(window);
            return;
        }

        if !self.selected_range.is_empty() {
            self.collapse_selection_to_cursor_offset(cx);
            invalidate_ime_position(window);
        }
    }

    pub(crate) fn copy_command(&mut self, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.draft.slice(self.selected_range.clone()),
            ));
        }
    }

    pub(crate) fn cut_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.draft.slice(self.selected_range.clone()),
            ));
            self.replace_text_in_byte_range(self.selected_range.clone(), "", cx);
            invalidate_ime_position(window);
        }
    }

    pub(crate) fn undo_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.undo(cx) {
            invalidate_ime_position(window);
        }
    }

    pub(crate) fn redo_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.redo(cx) {
            invalidate_ime_position(window);
        }
    }

    pub(crate) fn mouse_selection_start_command(
        &mut self,
        position: gpui::Point<Pixels>,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let Some(bounds) = self.last_board_bounds else {
            return;
        };
        if let Some(cell_index) = crate::editor::layout::logical_index_for_point(
            bounds,
            position,
            self.scroll_column,
            self.scroll_row,
            self.rows_per_column(),
            self.visible_columns(),
            self.visible_rows(),
            self.cell_size(),
            self.ruby_gutter_size(),
        ) {
            self.is_mouse_selecting = true;
            if shift {
                let anchor_cell = self.selection_anchor_cell();
                self.mouse_selection_anchor_cell = Some(anchor_cell);
                self.select_between_display_cells(anchor_cell, cell_index, cx);
            } else {
                self.mouse_selection_anchor_cell = Some(cell_index);
                self.move_cursor_to_display_cell(cell_index, cx);
            }
            invalidate_ime_position(window);
        }
    }

    pub(crate) fn mouse_selection_update_command(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_mouse_selecting {
            return;
        }
        let Some(anchor_cell) = self.mouse_selection_anchor_cell else {
            return;
        };
        let Some(cell_index) = self.clamped_display_cell_for_point(event.position) else {
            return;
        };
        self.select_between_display_cells(anchor_cell, cell_index, cx);
        invalidate_ime_position(window);
    }

    pub(crate) fn mouse_selection_end_command(&mut self, _: &MouseUpEvent) {
        self.is_mouse_selecting = false;
        self.mouse_selection_anchor_cell = None;
    }

    pub(crate) fn scroll_wheel_command(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(self.cell_size()));
        let column_delta = if delta.x == Pixels::ZERO {
            -(delta.y / px(self.cell_size()))
        } else {
            -(delta.x / px(self.cell_size()))
        };

        self.scroll_remainder_columns += column_delta;
        let whole_columns = self.scroll_remainder_columns.trunc() as isize;
        if whole_columns != 0 {
            self.scroll_remainder_columns -= whole_columns as f32;
            self.scroll_columns_by(whole_columns, cx);
        }
    }
}
