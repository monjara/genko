use settings::AppSettings;
use std::{ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

pub const BLANK_CELL: char = '\u{3000}';
const ROPE_LEAF_BYTES: usize = 1024;

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
}

impl Default for TextRope {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRope {
    pub fn new() -> Self {
        Self::new_with_rows(AppSettings::default_fixed_rows_per_column())
    }

    pub fn new_with_rows(rows_per_column: usize) -> Self {
        Self {
            root: None,
            rows_per_column: rows_per_column.max(1),
        }
    }

    pub fn rows_per_column(&self) -> usize {
        self.rows_per_column
    }

    pub fn set_rows_per_column(&mut self, rows_per_column: usize) {
        let rows_per_column = rows_per_column.max(1);
        if self.rows_per_column == rows_per_column {
            return;
        }

        self.rows_per_column = rows_per_column;
        if let Some(root) = self.root.as_mut() {
            root.refresh_cell_advances(rows_per_column);
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
        Self::from_str_with_rows(text, AppSettings::default_fixed_rows_per_column())
    }

    pub fn from_str_with_rows(text: &str, rows_per_column: usize) -> Self {
        let rows_per_column = rows_per_column.max(1);
        Self {
            root: RopeNode::from_str(text, rows_per_column),
            rows_per_column,
        }
    }

    #[cfg(test)]
    fn from_string(text: String) -> Self {
        Self::from_string_with_rows(text, AppSettings::default_fixed_rows_per_column())
    }

    #[cfg(test)]
    fn from_string_with_rows(text: String, rows_per_column: usize) -> Self {
        let rows_per_column = rows_per_column.max(1);
        Self {
            root: RopeNode::from_string(text, rows_per_column),
            rows_per_column,
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

        if range.start == range.end
            && range.end == self.len_bytes()
            && self.try_append_to_last_leaf(text)
        {
            return;
        }

        let root = self.root.take();
        let (left, rest) = split_node(root, range.start, self.rows_per_column);
        let (_, right) = split_node(rest, range.end - range.start, self.rows_per_column);
        self.root = concat_nodes(
            concat_nodes(
                left,
                RopeNode::from_str(text, self.rows_per_column),
                self.rows_per_column,
            ),
            right,
            self.rows_per_column,
        );
    }

    pub fn replace_range_owned(&mut self, range: Range<usize>, text: String) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len_bytes());

        if range.start == range.end
            && range.end == self.len_bytes()
            && self.root.is_some()
            && self.try_append_to_last_leaf(&text)
        {
            return;
        }

        let inserted = RopeNode::from_string(text, self.rows_per_column);
        let root = self.root.take();
        let (left, rest) = split_node(root, range.start, self.rows_per_column);
        let (_, right) = split_node(rest, range.end - range.start, self.rows_per_column);
        self.root = concat_nodes(
            concat_nodes(left, inserted, self.rows_per_column),
            right,
            self.rows_per_column,
        );
    }

    fn try_append_to_last_leaf(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return true;
        }

        if self.root.is_none() {
            self.root = RopeNode::from_str(text, self.rows_per_column);
            return true;
        }

        if text.len() > ROPE_LEAF_BYTES {
            return false;
        }

        self.root
            .as_mut()
            .is_some_and(|root| root.try_append_to_last_leaf(text, self.rows_per_column))
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
            node.byte_to_display_cell(byte_offset, 0, self.rows_per_column)
        })
    }

    pub fn byte_offset_for_display_cell(&self, display_cell_index: usize) -> usize {
        self.root.as_ref().map_or(0, |node| {
            node.display_cell_to_byte(display_cell_index, 0, 0, self.rows_per_column)
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
}

#[derive(Clone, Debug)]
enum RopeNode {
    Leaf {
        text: RopeLeafText,
        bytes: usize,
        utf16: usize,
        graphemes: usize,
        cell_advances: CellAdvances,
        height: usize,
    },
    Branch {
        left: Box<RopeNode>,
        right: Box<RopeNode>,
        bytes: usize,
        utf16: usize,
        graphemes: usize,
        cell_advances: CellAdvances,
        height: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CellAdvances {
    at_document_start: Vec<usize>,
    after_document_start: Vec<usize>,
}

#[derive(Clone, Debug)]
enum RopeLeafText {
    Owned(String),
    Shared {
        source: Arc<String>,
        range: Range<usize>,
    },
}

impl RopeLeafText {
    fn as_str(&self) -> &str {
        match self {
            Self::Owned(text) => text,
            Self::Shared { source, range } => &source[range.clone()],
        }
    }

    fn split_at(
        self,
        byte_offset: usize,
        rows_per_column: usize,
    ) -> (Option<Box<RopeNode>>, Option<Box<RopeNode>>) {
        match self {
            Self::Owned(mut text) => {
                let right = text.split_off(byte_offset);
                (
                    RopeNode::from_string(text, rows_per_column),
                    RopeNode::from_string(right, rows_per_column),
                )
            }
            Self::Shared { source, range } => {
                let split_offset = range.start + byte_offset;
                (
                    RopeNode::shared_leaf(
                        source.clone(),
                        range.start..split_offset,
                        rows_per_column,
                    ),
                    RopeNode::shared_leaf(source, split_offset..range.end, rows_per_column),
                )
            }
        }
    }
}

impl RopeNode {
    pub fn from_str(text: &str, rows_per_column: usize) -> Option<Box<Self>> {
        if text.is_empty() {
            return None;
        }

        let chunks = chunk_string(text);
        build_balanced(chunks, rows_per_column)
    }

    fn from_string(text: String, rows_per_column: usize) -> Option<Box<Self>> {
        if text.is_empty() {
            return None;
        }

        if text.len() <= ROPE_LEAF_BYTES {
            return Some(Self::leaf(text, rows_per_column));
        }

        build_balanced_nodes(
            chunk_shared_string(Arc::new(text), rows_per_column),
            rows_per_column,
        )
    }

    fn leaf(text: String, rows_per_column: usize) -> Box<Self> {
        Self::leaf_text(RopeLeafText::Owned(text), rows_per_column)
    }

    fn shared_leaf(
        source: Arc<String>,
        range: Range<usize>,
        rows_per_column: usize,
    ) -> Option<Box<Self>> {
        if range.is_empty() {
            None
        } else {
            Some(Self::leaf_text(
                RopeLeafText::Shared { source, range },
                rows_per_column,
            ))
        }
    }

    fn leaf_text(text: RopeLeafText, rows_per_column: usize) -> Box<Self> {
        let (bytes, utf16, graphemes) = {
            let text = text.as_str();
            (
                text.len(),
                text.encode_utf16().count(),
                text.graphemes(true).count(),
            )
        };
        let cell_advances = cell_advances_for_text(text.as_str(), graphemes, rows_per_column);
        Box::new(Self::Leaf {
            text,
            bytes,
            utf16,
            graphemes,
            cell_advances,
            height: 1,
        })
    }

    fn bytes(&self) -> usize {
        match self {
            Self::Leaf { bytes, .. } | Self::Branch { bytes, .. } => *bytes,
        }
    }

    fn utf16(&self) -> usize {
        match self {
            Self::Leaf { utf16, .. } | Self::Branch { utf16, .. } => *utf16,
        }
    }

    fn graphemes(&self) -> usize {
        match self {
            Self::Leaf { graphemes, .. } | Self::Branch { graphemes, .. } => *graphemes,
        }
    }

    fn cell_advance_from(&self, start_cell: usize, rows_per_column: usize) -> usize {
        let start_row = start_cell % rows_per_column;
        match self {
            Self::Leaf { cell_advances, .. } | Self::Branch { cell_advances, .. } => {
                if start_cell == 0 {
                    cell_advances.at_document_start[start_row]
                } else {
                    cell_advances.after_document_start[start_row]
                }
            }
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Leaf { height, .. } | Self::Branch { height, .. } => *height,
        }
    }

    fn try_append_to_last_leaf(&mut self, appended_text: &str, rows_per_column: usize) -> bool {
        match self {
            Self::Leaf {
                text: RopeLeafText::Owned(text),
                bytes,
                utf16,
                graphemes,
                cell_advances,
                ..
            } => {
                if text.len() + appended_text.len() > ROPE_LEAF_BYTES {
                    return false;
                }

                text.push_str(appended_text);
                *bytes = text.len();
                *utf16 = text.encode_utf16().count();
                *graphemes = text.graphemes(true).count();
                *cell_advances = cell_advances_for_text(text, *graphemes, rows_per_column);
                true
            }
            Self::Leaf { .. } => false,
            Self::Branch {
                left,
                right,
                bytes,
                utf16,
                graphemes,
                cell_advances,
                height,
            } => {
                if !right.try_append_to_last_leaf(appended_text, rows_per_column) {
                    return false;
                }

                *bytes = left.bytes() + right.bytes();
                *utf16 = left.utf16() + right.utf16();
                *graphemes = left.graphemes() + right.graphemes();
                *cell_advances = compose_cell_advances(&left, &right, rows_per_column);
                *height = left.height().max(right.height()) + 1;
                true
            }
        }
    }

    fn refresh_cell_advances(&mut self, rows_per_column: usize) {
        match self {
            Self::Leaf {
                text,
                graphemes,
                cell_advances,
                ..
            } => {
                *cell_advances = cell_advances_for_text(text.as_str(), *graphemes, rows_per_column);
            }
            Self::Branch {
                left,
                right,
                cell_advances,
                ..
            } => {
                left.refresh_cell_advances(rows_per_column);
                right.refresh_cell_advances(rows_per_column);
                *cell_advances = compose_cell_advances(left, right, rows_per_column);
            }
        }
    }

    fn advance_cell_for_grapheme(
        cell_index: usize,
        grapheme: &str,
        rows_per_column: usize,
    ) -> usize {
        if is_leading_attached_punctuation(grapheme, cell_index, rows_per_column) {
            cell_index
        } else if grapheme == "\n" {
            next_line_cell_index(cell_index, rows_per_column)
        } else {
            cell_index + 1
        }
    }

    fn push_to_string(&self, output: &mut String) {
        match self {
            Self::Leaf { text, .. } => output.push_str(text.as_str()),
            Self::Branch { left, right, .. } => {
                left.push_to_string(output);
                right.push_to_string(output);
            }
        }
    }

    fn push_range(&self, range: Range<usize>, node_start: usize, output: &mut String) {
        let node_end = node_start + self.bytes();
        if range.end <= node_start || range.start >= node_end {
            return;
        }

        match self {
            Self::Leaf { text, .. } => {
                let local_start = range.start.saturating_sub(node_start);
                let local_end = (range.end.min(node_end)) - node_start;
                output.push_str(&text.as_str()[local_start..local_end]);
            }
            Self::Branch { left, right, .. } => {
                left.push_range(range.clone(), node_start, output);
                right.push_range(range, node_start + left.bytes(), output);
            }
        }
    }

    fn push_visible_cells(
        &self,
        target_range: Range<usize>,
        node_byte_start: usize,
        node_cell_start: usize,
        rows_per_column: usize,
        output: &mut Vec<CellText>,
    ) {
        let node_cell_end =
            node_cell_start + self.cell_advance_from(node_cell_start, rows_per_column);
        if target_range.end <= node_cell_start || target_range.start >= node_cell_end {
            return;
        }

        match self {
            Self::Leaf { text, .. } => {
                let text = text.as_str();
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.grapheme_indices(true) {
                    if grapheme == "\n" {
                        cell_index =
                            Self::advance_cell_for_grapheme(cell_index, grapheme, rows_per_column);
                        continue;
                    }

                    let attached_to_previous =
                        is_leading_attached_punctuation(grapheme, cell_index, rows_per_column);
                    let display_cell_index = if attached_to_previous {
                        cell_index - 1
                    } else {
                        cell_index
                    };

                    if target_range.contains(&display_cell_index) {
                        let byte_start = node_byte_start + local_byte_start;
                        output.push(CellText {
                            logical_index: display_cell_index,
                            text: grapheme.to_string(),
                            range: byte_start..byte_start + grapheme.len(),
                            attached_to_previous,
                        });
                    }

                    cell_index =
                        Self::advance_cell_for_grapheme(cell_index, grapheme, rows_per_column);
                }
            }
            Self::Branch { left, right, .. } => {
                left.push_visible_cells(
                    target_range.clone(),
                    node_byte_start,
                    node_cell_start,
                    rows_per_column,
                    output,
                );
                let right_cell_start =
                    node_cell_start + left.cell_advance_from(node_cell_start, rows_per_column);
                right.push_visible_cells(
                    target_range,
                    node_byte_start + left.bytes(),
                    right_cell_start,
                    rows_per_column,
                    output,
                );
            }
        }
    }

    pub fn byte_to_utf16(&self, byte_offset: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                byte_to_utf16_in_str(text.as_str(), byte_offset.min(*bytes))
            }
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.byte_to_utf16(byte_offset)
                } else {
                    left.utf16() + right.byte_to_utf16(byte_offset - left.bytes())
                }
            }
        }
    }

    pub fn utf16_to_byte(&self, utf16_offset: usize) -> usize {
        match self {
            Self::Leaf { text, utf16, .. } => {
                utf16_to_byte_in_str(text.as_str(), utf16_offset.min(*utf16))
            }
            Self::Branch { left, right, .. } => {
                if utf16_offset <= left.utf16() {
                    left.utf16_to_byte(utf16_offset)
                } else {
                    left.bytes() + right.utf16_to_byte(utf16_offset - left.utf16())
                }
            }
        }
    }

    fn grapheme_to_byte(&self, grapheme_index: usize) -> usize {
        match self {
            Self::Leaf {
                text,
                bytes,
                graphemes,
                ..
            } => {
                if grapheme_index >= *graphemes {
                    *bytes
                } else {
                    text.as_str()
                        .grapheme_indices(true)
                        .nth(grapheme_index)
                        .map(|(offset, _)| offset)
                        .unwrap_or(*bytes)
                }
            }
            Self::Branch { left, right, .. } => {
                if grapheme_index <= left.graphemes() {
                    left.grapheme_to_byte(grapheme_index)
                } else {
                    left.bytes() + right.grapheme_to_byte(grapheme_index - left.graphemes())
                }
            }
        }
    }

    fn byte_to_grapheme(&self, byte_offset: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => text
                .as_str()
                .grapheme_indices(true)
                .take_while(|(offset, _)| *offset < byte_offset.min(*bytes))
                .count(),
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.byte_to_grapheme(byte_offset)
                } else {
                    left.graphemes() + right.byte_to_grapheme(byte_offset - left.bytes())
                }
            }
        }
    }

    fn byte_to_display_cell(
        &self,
        byte_offset: usize,
        node_cell_start: usize,
        rows_per_column: usize,
    ) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.as_str().grapheme_indices(true) {
                    if local_byte_start >= byte_offset.min(*bytes) {
                        break;
                    }

                    cell_index =
                        Self::advance_cell_for_grapheme(cell_index, grapheme, rows_per_column);
                }
                cell_index
            }
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.byte_to_display_cell(byte_offset, node_cell_start, rows_per_column)
                } else {
                    let right_cell_start =
                        node_cell_start + left.cell_advance_from(node_cell_start, rows_per_column);
                    right.byte_to_display_cell(
                        byte_offset - left.bytes(),
                        right_cell_start,
                        rows_per_column,
                    )
                }
            }
        }
    }

    fn display_cell_to_byte(
        &self,
        target_cell_index: usize,
        node_byte_start: usize,
        node_cell_start: usize,
        rows_per_column: usize,
    ) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.as_str().grapheme_indices(true) {
                    let next_cell_index =
                        Self::advance_cell_for_grapheme(cell_index, grapheme, rows_per_column);

                    if grapheme == "\n" {
                        if target_cell_index <= cell_index || target_cell_index < next_cell_index {
                            return node_byte_start + local_byte_start;
                        }
                    } else if next_cell_index == cell_index {
                        if target_cell_index < cell_index {
                            return node_byte_start + local_byte_start;
                        }
                    } else if target_cell_index <= cell_index {
                        return node_byte_start + local_byte_start;
                    }

                    cell_index = next_cell_index;
                }

                node_byte_start + bytes
            }
            Self::Branch { left, right, .. } => {
                let right_cell_start =
                    node_cell_start + left.cell_advance_from(node_cell_start, rows_per_column);
                if target_cell_index < right_cell_start {
                    left.display_cell_to_byte(
                        target_cell_index,
                        node_byte_start,
                        node_cell_start,
                        rows_per_column,
                    )
                } else {
                    right.display_cell_to_byte(
                        target_cell_index,
                        node_byte_start + left.bytes(),
                        right_cell_start,
                        rows_per_column,
                    )
                }
            }
        }
    }

    #[cfg(test)]
    fn assert_balanced(&self, rows_per_column: usize) -> usize {
        match self {
            Self::Leaf {
                text,
                bytes,
                utf16,
                graphemes,
                cell_advances,
                height,
            } => {
                let text = text.as_str();
                assert_eq!(*bytes, text.len());
                assert_eq!(*utf16, text.encode_utf16().count());
                assert_eq!(*graphemes, text.graphemes(true).count());
                assert_eq!(
                    *cell_advances,
                    cell_advances_for_text(text, *graphemes, rows_per_column)
                );
                assert_eq!(*height, 1);
                1
            }
            Self::Branch {
                left,
                right,
                bytes,
                utf16,
                graphemes,
                cell_advances,
                height,
            } => {
                let left_height = left.assert_balanced(rows_per_column);
                let right_height = right.assert_balanced(rows_per_column);
                assert!(
                    left_height.abs_diff(right_height) <= 1,
                    "rope branch is unbalanced: left height {left_height}, right height {right_height}"
                );
                assert_eq!(*bytes, left.bytes() + right.bytes());
                assert_eq!(*utf16, left.utf16() + right.utf16());
                assert_eq!(*graphemes, left.graphemes() + right.graphemes());
                assert_eq!(
                    *cell_advances,
                    compose_cell_advances(left, right, rows_per_column)
                );
                assert_eq!(*height, left_height.max(right_height) + 1);
                *height
            }
        }
    }

    #[cfg(test)]
    fn shared_leaf_count(&self) -> usize {
        match self {
            Self::Leaf {
                text: RopeLeafText::Shared { .. },
                ..
            } => 1,
            Self::Leaf { .. } => 0,
            Self::Branch { left, right, .. } => {
                left.shared_leaf_count() + right.shared_leaf_count()
            }
        }
    }
}

fn split_node(
    node: Option<Box<RopeNode>>,
    byte_offset: usize,
    rows_per_column: usize,
) -> (Option<Box<RopeNode>>, Option<Box<RopeNode>>) {
    let Some(node) = node else {
        return (None, None);
    };

    if byte_offset == 0 {
        return (None, Some(node));
    }

    if byte_offset >= node.bytes() {
        return (Some(node), None);
    }

    match *node {
        RopeNode::Leaf { text, .. } => {
            debug_assert!(text.as_str().is_char_boundary(byte_offset));
            text.split_at(byte_offset, rows_per_column)
        }
        RopeNode::Branch { left, right, .. } => {
            let left_len = left.bytes();
            if byte_offset < left_len {
                let (left_left, left_right) = split_node(Some(left), byte_offset, rows_per_column);
                (
                    left_left,
                    concat_nodes(left_right, Some(right), rows_per_column),
                )
            } else if byte_offset == left_len {
                (Some(left), Some(right))
            } else {
                let (right_left, right_right) =
                    split_node(Some(right), byte_offset - left_len, rows_per_column);
                (
                    concat_nodes(Some(left), right_left, rows_per_column),
                    right_right,
                )
            }
        }
    }
}

fn concat_nodes(
    left: Option<Box<RopeNode>>,
    right: Option<Box<RopeNode>>,
    rows_per_column: usize,
) -> Option<Box<RopeNode>> {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(left), Some(right)) => Some(concat_non_empty(left, right, rows_per_column)),
    }
}

fn concat_non_empty(
    left: Box<RopeNode>,
    right: Box<RopeNode>,
    rows_per_column: usize,
) -> Box<RopeNode> {
    if left.bytes() + right.bytes() <= ROPE_LEAF_BYTES {
        let mut text = String::with_capacity(left.bytes() + right.bytes());
        left.push_to_string(&mut text);
        right.push_to_string(&mut text);
        return RopeNode::leaf(text, rows_per_column);
    }

    if left.height() > right.height() + 1 {
        match *left {
            RopeNode::Branch {
                left: left_left,
                right: left_right,
                ..
            } => {
                return balance_branch(
                    left_left,
                    concat_non_empty(left_right, right, rows_per_column),
                    rows_per_column,
                );
            }
            leaf => return branch_node(Box::new(leaf), right, rows_per_column),
        }
    }

    if right.height() > left.height() + 1 {
        match *right {
            RopeNode::Branch {
                left: right_left,
                right: right_right,
                ..
            } => {
                return balance_branch(
                    concat_non_empty(left, right_left, rows_per_column),
                    right_right,
                    rows_per_column,
                );
            }
            leaf => return branch_node(left, Box::new(leaf), rows_per_column),
        }
    }

    branch_node(left, right, rows_per_column)
}

fn balance_branch(
    left: Box<RopeNode>,
    right: Box<RopeNode>,
    rows_per_column: usize,
) -> Box<RopeNode> {
    if left.height() > right.height() + 1 {
        return match *left {
            RopeNode::Branch {
                left: left_left,
                right: left_right,
                ..
            } => {
                if left_left.height() >= left_right.height() {
                    branch_node(
                        left_left,
                        branch_node(left_right, right, rows_per_column),
                        rows_per_column,
                    )
                } else {
                    match *left_right {
                        RopeNode::Branch {
                            left: left_right_left,
                            right: left_right_right,
                            ..
                        } => branch_node(
                            branch_node(left_left, left_right_left, rows_per_column),
                            branch_node(left_right_right, right, rows_per_column),
                            rows_per_column,
                        ),
                        leaf => branch_node(
                            left_left,
                            branch_node(Box::new(leaf), right, rows_per_column),
                            rows_per_column,
                        ),
                    }
                }
            }
            leaf => branch_node(Box::new(leaf), right, rows_per_column),
        };
    }

    if right.height() > left.height() + 1 {
        return match *right {
            RopeNode::Branch {
                left: right_left,
                right: right_right,
                ..
            } => {
                if right_right.height() >= right_left.height() {
                    branch_node(
                        branch_node(left, right_left, rows_per_column),
                        right_right,
                        rows_per_column,
                    )
                } else {
                    match *right_left {
                        RopeNode::Branch {
                            left: right_left_left,
                            right: right_left_right,
                            ..
                        } => branch_node(
                            branch_node(left, right_left_left, rows_per_column),
                            branch_node(right_left_right, right_right, rows_per_column),
                            rows_per_column,
                        ),
                        leaf => branch_node(
                            branch_node(left, Box::new(leaf), rows_per_column),
                            right_right,
                            rows_per_column,
                        ),
                    }
                }
            }
            leaf => branch_node(left, Box::new(leaf), rows_per_column),
        };
    }

    branch_node(left, right, rows_per_column)
}

fn branch_node(left: Box<RopeNode>, right: Box<RopeNode>, rows_per_column: usize) -> Box<RopeNode> {
    let bytes = left.bytes() + right.bytes();
    let utf16 = left.utf16() + right.utf16();
    let graphemes = left.graphemes() + right.graphemes();
    let cell_advances = compose_cell_advances(&left, &right, rows_per_column);
    let height = left.height().max(right.height()) + 1;

    Box::new(RopeNode::Branch {
        left,
        right,
        bytes,
        utf16,
        graphemes,
        cell_advances,
        height,
    })
}

fn next_line_cell_index(cell_index: usize, rows_per_column: usize) -> usize {
    ((cell_index / rows_per_column) + 1) * rows_per_column
}

fn is_leading_attached_punctuation(
    grapheme: &str,
    cell_index: usize,
    rows_per_column: usize,
) -> bool {
    cell_index > 0 && cell_index % rows_per_column == 0 && matches!(grapheme, "。" | "、")
}

fn filler_text_between_cells(
    mut current_cell: usize,
    target_cell: usize,
    rows_per_column: usize,
) -> String {
    let mut filler = String::new();

    while current_cell < target_cell {
        let next_line_cell = next_line_cell_index(current_cell, rows_per_column);
        if next_line_cell <= target_cell {
            filler.push('\n');
            current_cell = next_line_cell;
        } else {
            filler.push(BLANK_CELL);
            current_cell += 1;
        }
    }

    filler
}

fn cell_advances_for_text(text: &str, graphemes: usize, rows_per_column: usize) -> CellAdvances {
    if !has_layout_sensitive_grapheme(text) {
        let advances = vec![graphemes; rows_per_column];
        return CellAdvances {
            at_document_start: advances.clone(),
            after_document_start: advances,
        };
    }

    CellAdvances {
        at_document_start: cell_advances_for_text_context(text, rows_per_column, false),
        after_document_start: cell_advances_for_text_context(text, rows_per_column, true),
    }
}

fn cell_advances_for_text_context(
    text: &str,
    rows_per_column: usize,
    after_document_start: bool,
) -> Vec<usize> {
    let mut advances = vec![0; rows_per_column];
    for start_row in 0..rows_per_column {
        let start_cell = if after_document_start && start_row == 0 {
            rows_per_column
        } else {
            start_row
        };
        let mut cell_index = start_cell;
        for grapheme in text.graphemes(true) {
            cell_index = RopeNode::advance_cell_for_grapheme(cell_index, grapheme, rows_per_column);
        }
        advances[start_row] = cell_index - start_cell;
    }

    advances
}

fn has_layout_sensitive_grapheme(text: &str) -> bool {
    text.contains('\n') || text.contains('。') || text.contains('、')
}

fn compose_cell_advances(
    left: &RopeNode,
    right: &RopeNode,
    rows_per_column: usize,
) -> CellAdvances {
    CellAdvances {
        at_document_start: compose_cell_advances_context(left, right, rows_per_column, false),
        after_document_start: compose_cell_advances_context(left, right, rows_per_column, true),
    }
}

fn compose_cell_advances_context(
    left: &RopeNode,
    right: &RopeNode,
    rows_per_column: usize,
    after_document_start: bool,
) -> Vec<usize> {
    let mut advances = vec![0; rows_per_column];

    for start_row in 0..rows_per_column {
        let start_cell = if after_document_start && start_row == 0 {
            rows_per_column
        } else {
            start_row
        };
        let left_advance = left.cell_advance_from(start_cell, rows_per_column);
        let right_cell_start = start_cell + left_advance;
        let right_advance = right.cell_advance_from(right_cell_start, rows_per_column);
        advances[start_row] = left_advance + right_advance;
    }

    advances
}

fn build_balanced(leaves: Vec<String>, rows_per_column: usize) -> Option<Box<RopeNode>> {
    build_balanced_nodes(
        leaves
            .into_iter()
            .map(|leaf| RopeNode::leaf(leaf, rows_per_column))
            .collect(),
        rows_per_column,
    )
}

fn build_balanced_nodes(
    mut nodes: Vec<Box<RopeNode>>,
    rows_per_column: usize,
) -> Option<Box<RopeNode>> {
    while nodes.len() > 1 {
        let mut next_nodes = Vec::with_capacity(nodes.len().div_ceil(2));
        let mut iter = nodes.into_iter();

        while let Some(left) = iter.next() {
            if let Some(right) = iter.next() {
                next_nodes.push(concat_non_empty(left, right, rows_per_column));
            } else {
                next_nodes.push(left);
            }
        }

        nodes = next_nodes;
    }

    nodes.pop()
}

fn chunk_string(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for grapheme in text.graphemes(true) {
        if !current.is_empty() && current.len() + grapheme.len() > ROPE_LEAF_BYTES {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(grapheme);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn chunk_shared_string(source: Arc<String>, rows_per_column: usize) -> Vec<Box<RopeNode>> {
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    let mut chunk_bytes = 0;

    for (byte_offset, grapheme) in source.grapheme_indices(true) {
        if chunk_bytes > 0 && chunk_bytes + grapheme.len() > ROPE_LEAF_BYTES {
            if let Some(chunk) =
                RopeNode::shared_leaf(source.clone(), chunk_start..byte_offset, rows_per_column)
            {
                chunks.push(chunk);
            }
            chunk_start = byte_offset;
            chunk_bytes = 0;
        }

        chunk_bytes += grapheme.len();
    }

    if chunk_start < source.len()
        && let Some(chunk) =
            RopeNode::shared_leaf(source.clone(), chunk_start..source.len(), rows_per_column)
    {
        chunks.push(chunk);
    }

    chunks
}

fn byte_to_utf16_in_str(text: &str, byte_offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;

    for character in text.chars() {
        if utf8_count >= byte_offset {
            break;
        }
        utf8_count += character.len_utf8();
        utf16_offset += character.len_utf16();
    }

    utf16_offset
}

fn utf16_to_byte_in_str(text: &str, utf16_offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for character in text.chars() {
        if utf16_count >= utf16_offset {
            break;
        }
        utf16_count += character.len_utf16();
        utf8_offset += character.len_utf8();
    }

    utf8_offset
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
}
