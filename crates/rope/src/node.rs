use std::{ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

use super::{BLANK_CELL, CellText};

pub(super) const ROPE_LEAF_BYTES: usize = 1024;

#[derive(Clone, Debug)]
pub(super) enum RopeNode {
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
pub(super) struct CellAdvances {
    pub(super) at_document_start: Vec<usize>,
    pub(super) after_document_start: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(super) enum RopeLeafText {
    Owned(String),
    Shared {
        source: Arc<String>,
        range: Range<usize>,
    },
}

impl RopeLeafText {
    pub(super) fn as_str(&self) -> &str {
        match self {
            Self::Owned(text) => text,
            Self::Shared { source, range } => &source[range.clone()],
        }
    }

    pub(super) fn split_at(
        self,
        byte_offset: usize,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> (Option<Box<RopeNode>>, Option<Box<RopeNode>>) {
        match self {
            Self::Owned(mut text) => {
                let right = text.split_off(byte_offset);
                (
                    RopeNode::from_string(text, rows_per_column, hanging_punctuation),
                    RopeNode::from_string(right, rows_per_column, hanging_punctuation),
                )
            }
            Self::Shared { source, range } => {
                let split_offset = range.start + byte_offset;
                (
                    RopeNode::shared_leaf(
                        source.clone(),
                        range.start..split_offset,
                        rows_per_column,
                        hanging_punctuation,
                    ),
                    RopeNode::shared_leaf(
                        source,
                        split_offset..range.end,
                        rows_per_column,
                        hanging_punctuation,
                    ),
                )
            }
        }
    }
}

impl RopeNode {
    pub fn from_str(
        text: &str,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> Option<Box<Self>> {
        if text.is_empty() {
            return None;
        }

        let chunks = chunk_string(text);
        build_balanced(chunks, rows_per_column, hanging_punctuation)
    }

    pub(super) fn from_string(
        text: String,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> Option<Box<Self>> {
        if text.is_empty() {
            return None;
        }

        if text.len() <= ROPE_LEAF_BYTES {
            return Some(Self::leaf(text, rows_per_column, hanging_punctuation));
        }

        build_balanced_nodes(
            chunk_shared_string(Arc::new(text), rows_per_column, hanging_punctuation),
            rows_per_column,
            hanging_punctuation,
        )
    }

    pub(super) fn leaf(text: String, rows_per_column: usize, hanging_punctuation: bool) -> Box<Self> {
        Self::leaf_text(
            RopeLeafText::Owned(text),
            rows_per_column,
            hanging_punctuation,
        )
    }

    pub(super) fn shared_leaf(
        source: Arc<String>,
        range: Range<usize>,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> Option<Box<Self>> {
        if range.is_empty() {
            None
        } else {
            Some(Self::leaf_text(
                RopeLeafText::Shared { source, range },
                rows_per_column,
                hanging_punctuation,
            ))
        }
    }

    pub(super) fn leaf_text(
        text: RopeLeafText,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> Box<Self> {
        let (bytes, utf16, graphemes) = {
            let text = text.as_str();
            (
                text.len(),
                text.encode_utf16().count(),
                text.graphemes(true).count(),
            )
        };
        let cell_advances = cell_advances_for_text(
            text.as_str(),
            graphemes,
            rows_per_column,
            hanging_punctuation,
        );
        Box::new(Self::Leaf {
            text,
            bytes,
            utf16,
            graphemes,
            cell_advances,
            height: 1,
        })
    }

    pub(super) fn bytes(&self) -> usize {
        match self {
            Self::Leaf { bytes, .. } | Self::Branch { bytes, .. } => *bytes,
        }
    }

    pub(super) fn utf16(&self) -> usize {
        match self {
            Self::Leaf { utf16, .. } | Self::Branch { utf16, .. } => *utf16,
        }
    }

    pub(super) fn graphemes(&self) -> usize {
        match self {
            Self::Leaf { graphemes, .. } | Self::Branch { graphemes, .. } => *graphemes,
        }
    }

    pub(super) fn cell_advance_from(&self, start_cell: usize, rows_per_column: usize) -> usize {
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

    pub(super) fn height(&self) -> usize {
        match self {
            Self::Leaf { height, .. } | Self::Branch { height, .. } => *height,
        }
    }

    pub(super) fn try_append_to_last_leaf(
        &mut self,
        appended_text: &str,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> bool {
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
                *cell_advances =
                    cell_advances_for_text(text, *graphemes, rows_per_column, hanging_punctuation);
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
                if !right.try_append_to_last_leaf(
                    appended_text,
                    rows_per_column,
                    hanging_punctuation,
                ) {
                    return false;
                }

                *bytes = left.bytes() + right.bytes();
                *utf16 = left.utf16() + right.utf16();
                *graphemes = left.graphemes() + right.graphemes();
                *cell_advances =
                    compose_cell_advances(left, right, rows_per_column, hanging_punctuation);
                *height = left.height().max(right.height()) + 1;
                true
            }
        }
    }

    pub(super) fn refresh_cell_advances(&mut self, rows_per_column: usize, hanging_punctuation: bool) {
        match self {
            Self::Leaf {
                text,
                graphemes,
                cell_advances,
                ..
            } => {
                *cell_advances = cell_advances_for_text(
                    text.as_str(),
                    *graphemes,
                    rows_per_column,
                    hanging_punctuation,
                );
            }
            Self::Branch {
                left,
                right,
                cell_advances,
                ..
            } => {
                left.refresh_cell_advances(rows_per_column, hanging_punctuation);
                right.refresh_cell_advances(rows_per_column, hanging_punctuation);
                *cell_advances =
                    compose_cell_advances(left, right, rows_per_column, hanging_punctuation);
            }
        }
    }

    pub(super) fn advance_cell_for_grapheme(
        cell_index: usize,
        grapheme: &str,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> usize {
        if is_leading_attached_punctuation(
            grapheme,
            cell_index,
            rows_per_column,
            hanging_punctuation,
        ) {
            cell_index
        } else if grapheme == "\n" {
            next_line_cell_index(cell_index, rows_per_column)
        } else {
            cell_index + 1
        }
    }

    pub(super) fn push_to_string(&self, output: &mut String) {
        match self {
            Self::Leaf { text, .. } => output.push_str(text.as_str()),
            Self::Branch { left, right, .. } => {
                left.push_to_string(output);
                right.push_to_string(output);
            }
        }
    }

    pub(super) fn push_range(&self, range: Range<usize>, node_start: usize, output: &mut String) {
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

    pub(super) fn push_visible_cells(
        &self,
        target_range: Range<usize>,
        node_byte_start: usize,
        node_cell_start: usize,
        rows_per_column: usize,
        hanging_punctuation: bool,
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
                        cell_index = Self::advance_cell_for_grapheme(
                            cell_index,
                            grapheme,
                            rows_per_column,
                            hanging_punctuation,
                        );
                        continue;
                    }

                    let attached_to_previous = is_leading_attached_punctuation(
                        grapheme,
                        cell_index,
                        rows_per_column,
                        hanging_punctuation,
                    );
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

                    cell_index = Self::advance_cell_for_grapheme(
                        cell_index,
                        grapheme,
                        rows_per_column,
                        hanging_punctuation,
                    );
                }
            }
            Self::Branch { left, right, .. } => {
                left.push_visible_cells(
                    target_range.clone(),
                    node_byte_start,
                    node_cell_start,
                    rows_per_column,
                    hanging_punctuation,
                    output,
                );
                let right_cell_start =
                    node_cell_start + left.cell_advance_from(node_cell_start, rows_per_column);
                right.push_visible_cells(
                    target_range,
                    node_byte_start + left.bytes(),
                    right_cell_start,
                    rows_per_column,
                    hanging_punctuation,
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

    pub(super) fn grapheme_to_byte(&self, grapheme_index: usize) -> usize {
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

    pub(super) fn byte_to_grapheme(&self, byte_offset: usize) -> usize {
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

    pub(super) fn byte_to_display_cell(
        &self,
        byte_offset: usize,
        node_cell_start: usize,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.as_str().grapheme_indices(true) {
                    if local_byte_start >= byte_offset.min(*bytes) {
                        break;
                    }

                    cell_index = Self::advance_cell_for_grapheme(
                        cell_index,
                        grapheme,
                        rows_per_column,
                        hanging_punctuation,
                    );
                }
                cell_index
            }
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.byte_to_display_cell(
                        byte_offset,
                        node_cell_start,
                        rows_per_column,
                        hanging_punctuation,
                    )
                } else {
                    let right_cell_start =
                        node_cell_start + left.cell_advance_from(node_cell_start, rows_per_column);
                    right.byte_to_display_cell(
                        byte_offset - left.bytes(),
                        right_cell_start,
                        rows_per_column,
                        hanging_punctuation,
                    )
                }
            }
        }
    }

    pub(super) fn floor_char_boundary(&self, byte_offset: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let text = text.as_str();
                let mut boundary = byte_offset.min(*bytes);
                while boundary > 0 && !text.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                boundary
            }
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.floor_char_boundary(byte_offset)
                } else {
                    left.bytes() + right.floor_char_boundary(byte_offset - left.bytes())
                }
            }
        }
    }

    pub(super) fn ceil_char_boundary(&self, byte_offset: usize) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let text = text.as_str();
                let mut boundary = byte_offset.min(*bytes);
                while boundary < *bytes && !text.is_char_boundary(boundary) {
                    boundary += 1;
                }
                boundary
            }
            Self::Branch { left, right, .. } => {
                if byte_offset <= left.bytes() {
                    left.ceil_char_boundary(byte_offset)
                } else {
                    left.bytes() + right.ceil_char_boundary(byte_offset - left.bytes())
                }
            }
        }
    }

    pub(super) fn display_cell_to_byte(
        &self,
        target_cell_index: usize,
        node_byte_start: usize,
        node_cell_start: usize,
        rows_per_column: usize,
        hanging_punctuation: bool,
    ) -> usize {
        match self {
            Self::Leaf { text, bytes, .. } => {
                let mut cell_index = node_cell_start;
                for (local_byte_start, grapheme) in text.as_str().grapheme_indices(true) {
                    let next_cell_index = Self::advance_cell_for_grapheme(
                        cell_index,
                        grapheme,
                        rows_per_column,
                        hanging_punctuation,
                    );

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
                        hanging_punctuation,
                    )
                } else {
                    right.display_cell_to_byte(
                        target_cell_index,
                        node_byte_start + left.bytes(),
                        right_cell_start,
                        rows_per_column,
                        hanging_punctuation,
                    )
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn assert_balanced(&self, rows_per_column: usize) -> usize {
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
                    cell_advances_for_text(text, *graphemes, rows_per_column, true)
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
                    compose_cell_advances(left, right, rows_per_column, true)
                );
                assert_eq!(*height, left_height.max(right_height) + 1);
                *height
            }
        }
    }

    #[cfg(test)]
    pub(super) fn shared_leaf_count(&self) -> usize {
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

pub(super) fn split_node(
    node: Option<Box<RopeNode>>,
    byte_offset: usize,
    rows_per_column: usize,
    hanging_punctuation: bool,
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
            text.split_at(byte_offset, rows_per_column, hanging_punctuation)
        }
        RopeNode::Branch { left, right, .. } => {
            let left_len = left.bytes();
            if byte_offset < left_len {
                let (left_left, left_right) = split_node(
                    Some(left),
                    byte_offset,
                    rows_per_column,
                    hanging_punctuation,
                );
                (
                    left_left,
                    concat_nodes(
                        left_right,
                        Some(right),
                        rows_per_column,
                        hanging_punctuation,
                    ),
                )
            } else if byte_offset == left_len {
                (Some(left), Some(right))
            } else {
                let (right_left, right_right) = split_node(
                    Some(right),
                    byte_offset - left_len,
                    rows_per_column,
                    hanging_punctuation,
                );
                (
                    concat_nodes(Some(left), right_left, rows_per_column, hanging_punctuation),
                    right_right,
                )
            }
        }
    }
}

pub(super) fn concat_nodes(
    left: Option<Box<RopeNode>>,
    right: Option<Box<RopeNode>>,
    rows_per_column: usize,
    hanging_punctuation: bool,
) -> Option<Box<RopeNode>> {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(left), Some(right)) => Some(concat_non_empty(
            left,
            right,
            rows_per_column,
            hanging_punctuation,
        )),
    }
}

fn concat_non_empty(
    left: Box<RopeNode>,
    right: Box<RopeNode>,
    rows_per_column: usize,
    hanging_punctuation: bool,
) -> Box<RopeNode> {
    if left.bytes() + right.bytes() <= ROPE_LEAF_BYTES {
        let mut text = String::with_capacity(left.bytes() + right.bytes());
        left.push_to_string(&mut text);
        right.push_to_string(&mut text);
        return RopeNode::leaf(text, rows_per_column, hanging_punctuation);
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
                    concat_non_empty(left_right, right, rows_per_column, hanging_punctuation),
                    rows_per_column,
                    hanging_punctuation,
                );
            }
            leaf => {
                return branch_node(Box::new(leaf), right, rows_per_column, hanging_punctuation);
            }
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
                    concat_non_empty(left, right_left, rows_per_column, hanging_punctuation),
                    right_right,
                    rows_per_column,
                    hanging_punctuation,
                );
            }
            leaf => return branch_node(left, Box::new(leaf), rows_per_column, hanging_punctuation),
        }
    }

    branch_node(left, right, rows_per_column, hanging_punctuation)
}

fn balance_branch(
    left: Box<RopeNode>,
    right: Box<RopeNode>,
    rows_per_column: usize,
    hanging_punctuation: bool,
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
                        branch_node(left_right, right, rows_per_column, hanging_punctuation),
                        rows_per_column,
                        hanging_punctuation,
                    )
                } else {
                    match *left_right {
                        RopeNode::Branch {
                            left: left_right_left,
                            right: left_right_right,
                            ..
                        } => branch_node(
                            branch_node(
                                left_left,
                                left_right_left,
                                rows_per_column,
                                hanging_punctuation,
                            ),
                            branch_node(
                                left_right_right,
                                right,
                                rows_per_column,
                                hanging_punctuation,
                            ),
                            rows_per_column,
                            hanging_punctuation,
                        ),
                        leaf => branch_node(
                            left_left,
                            branch_node(
                                Box::new(leaf),
                                right,
                                rows_per_column,
                                hanging_punctuation,
                            ),
                            rows_per_column,
                            hanging_punctuation,
                        ),
                    }
                }
            }
            leaf => branch_node(Box::new(leaf), right, rows_per_column, hanging_punctuation),
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
                        branch_node(left, right_left, rows_per_column, hanging_punctuation),
                        right_right,
                        rows_per_column,
                        hanging_punctuation,
                    )
                } else {
                    match *right_left {
                        RopeNode::Branch {
                            left: right_left_left,
                            right: right_left_right,
                            ..
                        } => branch_node(
                            branch_node(
                                left,
                                right_left_left,
                                rows_per_column,
                                hanging_punctuation,
                            ),
                            branch_node(
                                right_left_right,
                                right_right,
                                rows_per_column,
                                hanging_punctuation,
                            ),
                            rows_per_column,
                            hanging_punctuation,
                        ),
                        leaf => branch_node(
                            branch_node(left, Box::new(leaf), rows_per_column, hanging_punctuation),
                            right_right,
                            rows_per_column,
                            hanging_punctuation,
                        ),
                    }
                }
            }
            leaf => branch_node(left, Box::new(leaf), rows_per_column, hanging_punctuation),
        };
    }

    branch_node(left, right, rows_per_column, hanging_punctuation)
}

fn branch_node(
    left: Box<RopeNode>,
    right: Box<RopeNode>,
    rows_per_column: usize,
    hanging_punctuation: bool,
) -> Box<RopeNode> {
    let bytes = left.bytes() + right.bytes();
    let utf16 = left.utf16() + right.utf16();
    let graphemes = left.graphemes() + right.graphemes();
    let cell_advances = compose_cell_advances(&left, &right, rows_per_column, hanging_punctuation);
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
    hanging_punctuation: bool,
) -> bool {
    hanging_punctuation
        && cell_index > 0
        && cell_index % rows_per_column == 0
        && matches!(grapheme, "。" | "、")
}

pub(super) fn filler_text_between_cells(
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

fn cell_advances_for_text(
    text: &str,
    graphemes: usize,
    rows_per_column: usize,
    hanging_punctuation: bool,
) -> CellAdvances {
    if !has_layout_sensitive_grapheme(text) {
        let advances = vec![graphemes; rows_per_column];
        return CellAdvances {
            at_document_start: advances.clone(),
            after_document_start: advances,
        };
    }

    CellAdvances {
        at_document_start: cell_advances_for_text_context(
            text,
            rows_per_column,
            false,
            hanging_punctuation,
        ),
        after_document_start: cell_advances_for_text_context(
            text,
            rows_per_column,
            true,
            hanging_punctuation,
        ),
    }
}

fn cell_advances_for_text_context(
    text: &str,
    rows_per_column: usize,
    after_document_start: bool,
    hanging_punctuation: bool,
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
            cell_index = RopeNode::advance_cell_for_grapheme(
                cell_index,
                grapheme,
                rows_per_column,
                hanging_punctuation,
            );
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
    hanging_punctuation: bool,
) -> CellAdvances {
    CellAdvances {
        at_document_start: compose_cell_advances_context(
            left,
            right,
            rows_per_column,
            false,
            hanging_punctuation,
        ),
        after_document_start: compose_cell_advances_context(
            left,
            right,
            rows_per_column,
            true,
            hanging_punctuation,
        ),
    }
}

fn compose_cell_advances_context(
    left: &RopeNode,
    right: &RopeNode,
    rows_per_column: usize,
    after_document_start: bool,
    hanging_punctuation: bool,
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
        let _ = hanging_punctuation;
        let right_advance = right.cell_advance_from(right_cell_start, rows_per_column);
        advances[start_row] = left_advance + right_advance;
    }

    advances
}

fn build_balanced(
    leaves: Vec<String>,
    rows_per_column: usize,
    hanging_punctuation: bool,
) -> Option<Box<RopeNode>> {
    build_balanced_nodes(
        leaves
            .into_iter()
            .map(|leaf| *RopeNode::leaf(leaf, rows_per_column, hanging_punctuation))
            .collect(),
        rows_per_column,
        hanging_punctuation,
    )
}

fn build_balanced_nodes(
    mut nodes: Vec<RopeNode>,
    rows_per_column: usize,
    hanging_punctuation: bool,
) -> Option<Box<RopeNode>> {
    while nodes.len() > 1 {
        let mut next_nodes = Vec::with_capacity(nodes.len().div_ceil(2));
        let mut iter = nodes.into_iter();

        while let Some(left) = iter.next() {
            if let Some(right) = iter.next() {
                next_nodes.push(*concat_non_empty(
                    Box::new(left),
                    Box::new(right),
                    rows_per_column,
                    hanging_punctuation,
                ));
            } else {
                next_nodes.push(left);
            }
        }

        nodes = next_nodes;
    }

    nodes.pop().map(Box::new)
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

fn chunk_shared_string(
    source: Arc<String>,
    rows_per_column: usize,
    hanging_punctuation: bool,
) -> Vec<RopeNode> {
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    let mut chunk_bytes = 0;

    for (byte_offset, grapheme) in source.grapheme_indices(true) {
        if chunk_bytes > 0 && chunk_bytes + grapheme.len() > ROPE_LEAF_BYTES {
            if let Some(chunk) = RopeNode::shared_leaf(
                source.clone(),
                chunk_start..byte_offset,
                rows_per_column,
                hanging_punctuation,
            ) {
                chunks.push(*chunk);
            }
            chunk_start = byte_offset;
            chunk_bytes = 0;
        }

        chunk_bytes += grapheme.len();
    }

    if chunk_start < source.len()
        && let Some(chunk) = RopeNode::shared_leaf(
            source.clone(),
            chunk_start..source.len(),
            rows_per_column,
            hanging_punctuation,
        )
    {
        chunks.push(*chunk);
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

pub(super) fn utf16_to_byte_in_str(text: &str, utf16_offset: usize) -> usize {
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
