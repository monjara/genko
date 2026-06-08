mod node;
use node::{
    ROPE_LEAF_BYTES, RopeNode, concat_nodes, filler_text_between_cells, split_node,
    utf16_to_byte_in_str,
};

use settings::AppSettings;
use std::ops::Range;

pub const BLANK_CELL: char = '\u{3000}';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellText {
    pub logical_index: usize,
    pub text: String,
    pub range: Range<usize>,
    pub attached_to_previous: bool,
}

#[derive(Clone, Debug)]
pub struct TextRope {
    root: Option<Box<RopeNode>>,
    rows_per_column: usize,
    hanging_punctuation: bool,
}

impl Default for TextRope {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRope {
    pub fn new() -> Self {
        Self::new_with_rows(AppSettings::default_rows_per_column())
    }

    pub fn new_with_rows(rows_per_column: usize) -> Self {
        Self {
            root: None,
            rows_per_column: rows_per_column.max(1),
            hanging_punctuation: true,
        }
    }

    pub fn rows_per_column(&self) -> usize {
        self.rows_per_column
    }

    pub fn hanging_punctuation(&self) -> bool {
        self.hanging_punctuation
    }

    pub fn set_rows_per_column(&mut self, rows_per_column: usize) {
        let rows_per_column = rows_per_column.max(1);
        if self.rows_per_column == rows_per_column {
            return;
        }

        self.rows_per_column = rows_per_column;
        if let Some(root) = self.root.as_mut() {
            root.refresh_cell_advances(rows_per_column, self.hanging_punctuation);
        }
    }

    pub fn set_hanging_punctuation(&mut self, enabled: bool) {
        if self.hanging_punctuation == enabled {
            return;
        }

        self.hanging_punctuation = enabled;
        if let Some(root) = self.root.as_mut() {
            root.refresh_cell_advances(self.rows_per_column, self.hanging_punctuation);
        }
    }

    pub fn len_bytes(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.bytes())
    }

    pub fn len_graphemes(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.graphemes())
    }

    pub fn len_display_cells(&self) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.cell_advance_from(0, self.rows_per_column))
    }

    #[cfg(test)]
    fn height(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.height())
    }

    #[cfg(test)]
    fn assert_balanced(&self) {
        if let Some(root) = self.root.as_ref() {
            root.assert_balanced(self.rows_per_column);
        }
    }

    #[cfg(test)]
    fn shared_leaf_count(&self) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.shared_leaf_count())
    }

    pub fn from_str(text: &str) -> Self {
        Self::from_str_with_rows(text, AppSettings::default_rows_per_column())
    }

    pub fn from_str_with_rows(text: &str, rows_per_column: usize) -> Self {
        let rows_per_column = rows_per_column.max(1);
        Self {
            root: RopeNode::from_str(text, rows_per_column, true),
            rows_per_column,
            hanging_punctuation: true,
        }
    }

    #[cfg(test)]
    fn from_string(text: String) -> Self {
        Self::from_string_with_rows(text, AppSettings::default_rows_per_column())
    }

    #[cfg(test)]
    fn from_string_with_rows(text: String, rows_per_column: usize) -> Self {
        let rows_per_column = rows_per_column.max(1);
        Self {
            root: RopeNode::from_string(text, rows_per_column, true),
            rows_per_column,
            hanging_punctuation: true,
        }
    }

    pub fn to_string(&self) -> String {
        let mut output = String::with_capacity(self.len_bytes());
        if let Some(root) = self.root.as_ref() {
            root.push_to_string(&mut output);
        }
        output
    }

    pub fn slice(&self, range: Range<usize>) -> String {
        let mut output = String::with_capacity(range.end.saturating_sub(range.start));
        if let Some(root) = self.root.as_ref() {
            root.push_range(range, 0, &mut output);
        }
        output
    }

    pub fn visible_cells(&self, start_index: usize, max_count: usize) -> Vec<CellText> {
        let target_range = start_index..start_index.saturating_add(max_count);
        let mut cells = Vec::with_capacity(max_count.min(self.len_display_cells()));
        if let Some(root) = self.root.as_ref() {
            root.push_visible_cells(
                start_index..start_index.saturating_add(max_count).saturating_add(1),
                0,
                0,
                self.rows_per_column,
                self.hanging_punctuation,
                &mut cells,
            );
        }
        cells.retain(|cell| target_range.contains(&cell.logical_index));
        cells
    }

    #[cfg(test)]
    fn visible_graphemes(&self, start_index: usize, max_count: usize) -> Vec<CellText> {
        self.visible_cells(start_index, max_count)
    }

    pub fn replace_range(&mut self, range: Range<usize>, text: &str) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len_bytes());

        let normalized_start = self.floor_char_boundary(range.start);
        let normalized_end = self.ceil_char_boundary(range.end);
        let range = normalized_start..normalized_end;

        if range.start == 0 && range.end == self.len_bytes() {
            self.root = RopeNode::from_str(text, self.rows_per_column, self.hanging_punctuation);
            return;
        }

        if range.start == range.end
            && range.end == self.len_bytes()
            && self.try_append_to_last_leaf(text)
        {
            return;
        }

        if range.start == range.end && range.end == self.len_bytes() {
            self.root = concat_nodes(
                self.root.take(),
                RopeNode::from_str(text, self.rows_per_column, self.hanging_punctuation),
                self.rows_per_column,
                self.hanging_punctuation,
            );
            return;
        }

        let root = self.root.take();
        let (left, rest) = split_node(
            root,
            range.start,
            self.rows_per_column,
            self.hanging_punctuation,
        );
        let (_, right) = split_node(
            rest,
            range.end - range.start,
            self.rows_per_column,
            self.hanging_punctuation,
        );
        self.root = concat_nodes(
            concat_nodes(
                left,
                RopeNode::from_str(text, self.rows_per_column, self.hanging_punctuation),
                self.rows_per_column,
                self.hanging_punctuation,
            ),
            right,
            self.rows_per_column,
            self.hanging_punctuation,
        );
    }

    pub fn replace_range_owned(&mut self, range: Range<usize>, text: String) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len_bytes());

        let normalized_start = self.floor_char_boundary(range.start);
        let normalized_end = self.ceil_char_boundary(range.end);
        let range = normalized_start..normalized_end;

        if range.start == 0 && range.end == self.len_bytes() {
            self.root = RopeNode::from_string(text, self.rows_per_column, self.hanging_punctuation);
            return;
        }

        if range.start == range.end
            && range.end == self.len_bytes()
            && self.root.is_some()
            && self.try_append_to_last_leaf(&text)
        {
            return;
        }

        if range.start == range.end && range.end == self.len_bytes() {
            self.root = concat_nodes(
                self.root.take(),
                RopeNode::from_string(text, self.rows_per_column, self.hanging_punctuation),
                self.rows_per_column,
                self.hanging_punctuation,
            );
            return;
        }

        let inserted = RopeNode::from_string(text, self.rows_per_column, self.hanging_punctuation);
        let root = self.root.take();
        let (left, rest) = split_node(
            root,
            range.start,
            self.rows_per_column,
            self.hanging_punctuation,
        );
        let (_, right) = split_node(
            rest,
            range.end - range.start,
            self.rows_per_column,
            self.hanging_punctuation,
        );
        self.root = concat_nodes(
            concat_nodes(
                left,
                inserted,
                self.rows_per_column,
                self.hanging_punctuation,
            ),
            right,
            self.rows_per_column,
            self.hanging_punctuation,
        );
    }

    fn try_append_to_last_leaf(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return true;
        }

        if self.root.is_none() {
            self.root = RopeNode::from_str(text, self.rows_per_column, self.hanging_punctuation);
            return true;
        }

        if text.len() > ROPE_LEAF_BYTES {
            return false;
        }

        self.root.as_mut().is_some_and(|root| {
            root.try_append_to_last_leaf(text, self.rows_per_column, self.hanging_punctuation)
        })
    }

    pub fn byte_to_utf16(&self, byte_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.byte_to_utf16(byte_offset))
    }

    pub fn utf16_to_byte(&self, utf16_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.utf16_to_byte(utf16_offset))
    }

    pub fn byte_offset_for_grapheme_index(&self, grapheme_index: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.grapheme_to_byte(grapheme_index))
    }

    pub fn grapheme_index_for_byte(&self, byte_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.byte_to_grapheme(byte_offset))
    }

    pub fn display_cell_for_byte(&self, byte_offset: usize) -> usize {
        self.root.as_ref().map_or(0, |node| {
            node.byte_to_display_cell(
                byte_offset,
                0,
                self.rows_per_column,
                self.hanging_punctuation,
            )
        })
    }

    pub fn byte_offset_for_display_cell(&self, display_cell_index: usize) -> usize {
        self.root.as_ref().map_or(0, |node| {
            node.display_cell_to_byte(
                display_cell_index,
                0,
                0,
                self.rows_per_column,
                self.hanging_punctuation,
            )
        })
    }

    pub fn materialize_display_cell(&mut self, display_cell_index: usize) -> usize {
        let offset = self.byte_offset_for_display_cell(display_cell_index);
        let current_cell = self.display_cell_for_byte(offset);
        if display_cell_index <= current_cell {
            return offset;
        }

        let filler =
            filler_text_between_cells(current_cell, display_cell_index, self.rows_per_column);
        let filler_len = filler.len();
        self.replace_range(offset..offset, &filler);
        offset + filler_len
    }

    pub fn floor_char_boundary(&self, byte_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.floor_char_boundary(byte_offset))
    }

    pub fn ceil_char_boundary(&self, byte_offset: usize) -> usize {
        self.root
            .as_ref()
            .map_or(0, |node| node.ceil_char_boundary(byte_offset))
    }

    pub fn next_char_boundary(&self, byte_offset: usize) -> usize {
        let byte_offset = self.ceil_char_boundary(byte_offset.min(self.len_bytes()));
        if byte_offset >= self.len_bytes() {
            self.len_bytes()
        } else {
            self.ceil_char_boundary(byte_offset + 1)
        }
    }

    pub fn previous_char_boundary(&self, byte_offset: usize) -> usize {
        let byte_offset = self.floor_char_boundary(byte_offset.min(self.len_bytes()));
        if byte_offset == 0 {
            0
        } else {
            self.floor_char_boundary(byte_offset - 1)
        }
    }

    pub fn char_at(&self, byte_offset: usize) -> Option<char> {
        let start = self.floor_char_boundary(byte_offset.min(self.len_bytes()));
        if start >= self.len_bytes() {
            return None;
        }
        let end = self.next_char_boundary(start);
        self.slice(start..end).chars().next()
    }
}

pub fn utf16_to_byte_in_text(text: &str, utf16_offset: usize) -> usize {
    utf16_to_byte_in_str(text, utf16_offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ROWS: usize = 20;

    #[test]
    fn rope_replaces_japanese_text_on_char_boundaries() {
        let mut rope = TextRope::from_str("abc");
        rope.replace_range(1..2, "日本語");
        assert_eq!(rope.to_string(), "a日本語c");
        rope.replace_range(1.."日本語".len() + 1, "文");
        assert_eq!(rope.to_string(), "a文c");
    }

    #[test]
    fn rope_replaces_mid_char_ranges_on_surrounding_boundaries() {
        let mut rope = TextRope::from_str("aあb");

        rope.replace_range(2..3, "い");

        assert_eq!(rope.to_string(), "aいb");
    }

    #[test]
    fn rope_converts_utf16_offsets() {
        let rope = TextRope::from_str("a😀文");
        assert_eq!(rope.utf16_to_byte(0), 0);
        assert_eq!(rope.utf16_to_byte(1), 1);
        assert_eq!(rope.utf16_to_byte(3), "a😀".len());
        assert_eq!(rope.byte_to_utf16("a😀".len()), 3);
    }

    #[test]
    fn rope_returns_only_visible_graphemes() {
        let rope = TextRope::from_str("一二三四五");
        let visible = rope.visible_graphemes(2, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].logical_index, 2);
        assert_eq!(visible[0].text, "三");
        assert_eq!(visible[1].logical_index, 3);
        assert_eq!(visible[1].text, "四");
    }

    #[test]
    fn rope_attaches_leading_punctuation_to_previous_column_end() {
        let rope = TextRope::from_str_with_rows("一二三、四", 3);
        let visible = rope.visible_cells(0, 3);
        let after_punctuation = "一二三、".len();

        assert_eq!(visible.len(), 4);
        assert_eq!(rope.len_display_cells(), 4);
        assert_eq!(rope.display_cell_for_byte(after_punctuation), 3);
        assert_eq!(rope.byte_offset_for_display_cell(3), after_punctuation);
        assert_eq!(visible[0].logical_index, 0);
        assert_eq!(visible[0].text, "一");
        assert!(!visible[0].attached_to_previous);
        assert_eq!(visible[2].logical_index, 2);
        assert_eq!(visible[2].text, "三");
        assert!(!visible[2].attached_to_previous);
        assert_eq!(visible[3].logical_index, 2);
        assert_eq!(visible[3].text, "、");
        assert!(visible[3].attached_to_previous);
    }

    #[test]
    fn rope_does_not_attach_leading_punctuation_when_disabled() {
        let mut rope = TextRope::from_str_with_rows("一二三、四", 3);
        rope.set_hanging_punctuation(false);
        let visible = rope.visible_cells(0, 4);

        assert_eq!(rope.len_display_cells(), 5);
        assert_eq!(visible[3].logical_index, 3);
        assert_eq!(visible[3].text, "、");
        assert!(!visible[3].attached_to_previous);
    }

    #[test]
    fn rope_places_cursor_after_leading_punctuation_before_next_character() {
        let mut rope = TextRope::from_str_with_rows("一二三", 3);
        let punctuation_at = rope.byte_offset_for_display_cell(3);

        rope.replace_range(punctuation_at..punctuation_at, "、");
        let next_insert_at = rope.byte_offset_for_display_cell(3);
        rope.replace_range(next_insert_at..next_insert_at, "四");

        assert_eq!(rope.to_string(), "一二三、四");
        assert_eq!(rope.display_cell_for_byte("一二三、".len()), 3);
        assert_eq!(rope.display_cell_for_byte("一二三、四".len()), 4);
    }

    #[test]
    fn rope_attaches_leading_punctuation_across_node_boundary() {
        let rows = 4;
        let prefix = "a".repeat(ROPE_LEAF_BYTES);
        let text = format!("{prefix}、文");
        let rope = TextRope::from_str_with_rows(&text, rows);
        let after_punctuation = prefix.len() + "、".len();
        let visible = rope.visible_cells(prefix.len() - 2, 4);
        let punctuation = visible.iter().find(|cell| cell.text == "、").unwrap();

        assert!(rope.height() > 1);
        assert_eq!(rope.len_display_cells(), prefix.len() + 1);
        assert_eq!(rope.display_cell_for_byte(after_punctuation), prefix.len());
        assert_eq!(
            rope.byte_offset_for_display_cell(prefix.len()),
            after_punctuation
        );
        assert_eq!(punctuation.logical_index, prefix.len() - 1);
        assert!(punctuation.attached_to_previous);
    }

    #[test]
    fn rope_does_not_attach_punctuation_inside_column() {
        let rope = TextRope::from_str_with_rows("一二、三", 3);
        let visible = rope.visible_cells(0, 3);
        let punctuation = visible.iter().find(|cell| cell.text == "、").unwrap();

        assert_eq!(punctuation.logical_index, 2);
        assert!(!punctuation.attached_to_previous);
    }

    #[test]
    fn rope_newline_advances_to_next_vertical_column() {
        let rope = TextRope::from_str("あ\nい");
        let visible = rope.visible_cells(0, ROWS + 1);

        assert_eq!(rope.len_graphemes(), 3);
        assert_eq!(rope.len_display_cells(), ROWS + 1);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].logical_index, 0);
        assert_eq!(visible[0].text, "あ");
        assert_eq!(visible[1].logical_index, ROWS);
        assert_eq!(visible[1].text, "い");
    }

    #[test]
    fn rope_maps_newline_gap_to_line_break_byte_offset() {
        let rope = TextRope::from_str("あ\nい");
        let newline_start = "あ".len();
        let after_newline = "あ\n".len();

        assert_eq!(rope.display_cell_for_byte(newline_start), 1);
        assert_eq!(rope.display_cell_for_byte(after_newline), ROWS);
        assert_eq!(rope.byte_offset_for_display_cell(1), newline_start);
        assert_eq!(rope.byte_offset_for_display_cell(ROWS - 1), newline_start);
        assert_eq!(rope.byte_offset_for_display_cell(ROWS), after_newline);
    }

    #[test]
    fn rope_recomputes_display_cells_when_rows_change() {
        let mut rope = TextRope::from_str("あ\nい");
        let rows = 24;

        rope.set_rows_per_column(rows);

        assert_eq!(rope.rows_per_column(), rows);
        assert_eq!(rope.len_display_cells(), rows + 1);
        assert_eq!(rope.display_cell_for_byte("あ\n".len()), rows);
        assert_eq!(rope.byte_offset_for_display_cell(rows), "あ\n".len());
    }

    #[test]
    fn rope_materializes_empty_cells_before_insert() {
        let mut rope = TextRope::new();
        let insert_at = rope.materialize_display_cell(5);
        rope.replace_range(insert_at..insert_at, "文");

        assert_eq!(rope.len_display_cells(), 6);
        let visible = rope.visible_cells(0, 6);
        assert_eq!(visible.len(), 6);
        assert_eq!(visible[0].text, BLANK_CELL.to_string());
        assert_eq!(visible[4].text, BLANK_CELL.to_string());
        assert_eq!(visible[5].logical_index, 5);
        assert_eq!(visible[5].text, "文");
    }

    #[test]
    fn rope_materializes_newline_gap_before_insert() {
        let mut rope = TextRope::from_str("あ\nい");
        let insert_at = rope.materialize_display_cell(5);
        rope.replace_range(insert_at..insert_at, "文");

        let visible = rope.visible_cells(0, ROWS + 1);
        let inserted = visible.iter().find(|cell| cell.text == "文").unwrap();
        let after_newline = visible.iter().find(|cell| cell.text == "い").unwrap();

        assert_eq!(inserted.logical_index, 5);
        assert_eq!(after_newline.logical_index, ROWS);
    }

    #[test]
    fn rope_keeps_right_edge_appends_balanced() {
        let mut rope = TextRope::new();

        for _ in 0..20_000 {
            let end = rope.len_bytes();
            rope.replace_range(end..end, "文");
        }

        assert_eq!(rope.len_graphemes(), 20_000);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(19_998, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].logical_index, 19_998);
        assert_eq!(visible[1].logical_index, 19_999);
    }

    #[test]
    fn rope_keeps_middle_edits_balanced() {
        let mut rope = TextRope::from_str(&"a".repeat(20_000));

        for _ in 0..500 {
            let midpoint = rope.byte_offset_for_grapheme_index(rope.len_graphemes() / 2);
            rope.replace_range(midpoint..midpoint, "文");
        }

        assert_eq!(rope.len_graphemes(), 20_500);
        assert!(rope.height() <= 32);
        rope.assert_balanced();
    }

    #[test]
    fn rope_uses_shared_chunks_for_owned_large_text() {
        let rope = TextRope::from_string("文".repeat(20_000));

        assert_eq!(rope.len_graphemes(), 20_000);
        assert!(rope.shared_leaf_count() > 0);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(19_998, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].text, "文");
        assert_eq!(visible[1].text, "文");
    }

    #[test]
    fn rope_pastes_owned_large_text_into_existing_document() {
        let mut rope = TextRope::from_str("開始終了");
        let insert_at = "開始".len();

        rope.replace_range_owned(insert_at..insert_at, "文".repeat(20_000));

        assert_eq!(rope.len_graphemes(), 20_004);
        assert!(rope.shared_leaf_count() > 0);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(0, 3);
        assert_eq!(visible[0].text, "開");
        assert_eq!(visible[1].text, "始");
        assert_eq!(visible[2].text, "文");
    }

    #[test]
    fn rope_replaces_entire_document_with_owned_large_text() {
        let mut rope = TextRope::from_str("開始終了");
        let replacement = "文".repeat(20_000);

        rope.replace_range_owned(0..rope.len_bytes(), replacement.clone());

        assert_eq!(rope.len_graphemes(), 20_000);
        assert_eq!(rope.to_string(), replacement);
        assert!(rope.shared_leaf_count() > 0);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(0, 3);
        assert_eq!(visible[0].text, "文");
        assert_eq!(visible[1].text, "文");
        assert_eq!(visible[2].text, "文");
    }

    #[test]
    fn rope_appends_owned_large_text_at_document_end() {
        let mut rope = TextRope::from_str("開始終了");
        let append_at = rope.len_bytes();

        rope.replace_range_owned(append_at..append_at, "文".repeat(20_000));

        assert_eq!(rope.len_graphemes(), 20_004);
        assert!(rope.shared_leaf_count() > 0);
        assert!(rope.height() <= 32);
        rope.assert_balanced();

        let visible = rope.visible_graphemes(0, 6);
        assert_eq!(visible[0].text, "開");
        assert_eq!(visible[1].text, "始");
        assert_eq!(visible[2].text, "終");
        assert_eq!(visible[3].text, "了");
        assert_eq!(visible[4].text, "文");
        assert_eq!(visible[5].text, "文");
    }
}
