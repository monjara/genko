use std::cmp::{max, min};
use std::ops::Range;

use serde::{Deserialize, Serialize};

pub const FILE_EXTENSION: &str = "soukou";

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    #[default]
    RichText,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    HeadingLarge,
    HeadingMedium,
    #[default]
    Body,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum InlineStyle {
    Bold,
    Strikethrough,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InlineMark {
    pub start: usize,
    pub end: usize,
    pub style: InlineStyle,
}

impl InlineMark {
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockMark {
    pub start: usize,
    pub kind: BlockKind,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpubMetadata {
    pub title: String,
    pub creators: Vec<String>,
    pub language: String,
    pub identifier: String,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub rights: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBlock {
    pub range: Range<usize>,
    pub kind: BlockKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RichDocument {
    pub version: u32,
    pub kind: DocumentKind,
    pub text: String,
    #[serde(default)]
    pub epub_metadata: Option<EpubMetadata>,
    pub blocks: Vec<BlockMark>,
    pub spans: Vec<InlineMark>,
}

impl Default for RichDocument {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl RichDocument {
    pub fn new(text: String) -> Self {
        let mut document = Self {
            version: 1,
            kind: DocumentKind::RichText,
            text,
            epub_metadata: None,
            blocks: Vec::new(),
            spans: Vec::new(),
        };
        document.ensure_default_blocks();
        document
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut document = serde_json::from_str::<Self>(json)?;
        document.normalize();
        Ok(document)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn plain_text(&self) -> &str {
        self.text.as_str()
    }

    pub fn paragraph_range_at(&self, offset: usize) -> Range<usize> {
        paragraph_range_in_text(self.text.as_str(), offset)
    }

    pub fn replace_text(&mut self, range: Range<usize>, new_text: &str) {
        let old_text = self.text.clone();
        self.text.replace_range(range.clone(), new_text);
        transform_inline_marks(&mut self.spans, range.clone(), new_text.len());
        transform_block_marks(
            &mut self.blocks,
            old_text.as_str(),
            self.text.as_str(),
            range,
        );
        self.normalize();
    }

    pub fn toggle_inline_style(&mut self, range: Range<usize>, style: InlineStyle) {
        if range.is_empty() {
            return;
        }

        if self.range_has_style(range.clone(), style) {
            remove_inline_style(&mut self.spans, range, style);
        } else {
            self.spans.push(InlineMark {
                start: range.start,
                end: range.end,
                style,
            });
        }
        self.normalize();
    }

    pub fn set_block_kind_for_range(&mut self, range: Range<usize>, kind: BlockKind) {
        for paragraph in paragraph_ranges_intersecting(self.text.as_str(), range) {
            let start = paragraph.start;
            self.blocks.retain(|block| block.start != start);
            if kind != BlockKind::Body {
                self.blocks.push(BlockMark { start, kind });
            }
        }
        self.normalize_blocks();
    }

    pub fn block_kind_for_offset(&self, offset: usize) -> BlockKind {
        let paragraph_start = self.paragraph_range_at(offset).start;
        self.blocks
            .iter()
            .find(|block| block.start == paragraph_start)
            .map(|block| block.kind)
            .unwrap_or(BlockKind::Body)
    }

    pub fn inline_styles_for_range(&self, range: Range<usize>) -> Vec<InlineStyle> {
        let mut styles = self
            .spans
            .iter()
            .filter(|mark| mark.start < range.end && range.start < mark.end)
            .map(|mark| mark.style)
            .collect::<Vec<_>>();
        styles.sort();
        styles.dedup();
        styles
    }

    pub fn resolved_blocks(&self) -> Vec<ResolvedBlock> {
        self.blocks
            .iter()
            .map(|block| ResolvedBlock {
                range: self.paragraph_range_at(block.start),
                kind: block.kind,
            })
            .collect()
    }

    pub fn ensure_default_blocks(&mut self) {
        self.normalize_blocks();
    }

    fn range_has_style(&self, range: Range<usize>, style: InlineStyle) -> bool {
        let mut covered_until = range.start;
        let mut marks = self
            .spans
            .iter()
            .filter(|mark| mark.style == style && mark.end > range.start && mark.start < range.end)
            .collect::<Vec<_>>();
        marks.sort_by_key(|mark| mark.start);

        for mark in marks {
            if mark.start > covered_until {
                return false;
            }
            covered_until = covered_until.max(min(mark.end, range.end));
            if covered_until >= range.end {
                return true;
            }
        }

        false
    }

    fn normalize(&mut self) {
        self.normalize_spans();
        self.normalize_blocks();
    }

    fn normalize_spans(&mut self) {
        self.spans
            .retain(|mark| mark.start < mark.end && mark.end <= self.text.len());
        self.spans
            .sort_by_key(|mark| (mark.style, mark.start, mark.end));

        let mut merged: Vec<InlineMark> = Vec::with_capacity(self.spans.len());
        for mark in self.spans.drain(..) {
            if let Some(previous) = merged.last_mut()
                && previous.style == mark.style
                && mark.start <= previous.end
            {
                previous.end = max(previous.end, mark.end);
            } else {
                merged.push(mark);
            }
        }
        self.spans = merged;
    }

    fn normalize_blocks(&mut self) {
        let paragraph_starts = paragraph_starts(self.text.as_str());
        self.blocks.retain(|block| {
            block.kind != BlockKind::Body
                && block.start <= self.text.len()
                && paragraph_starts.binary_search(&block.start).is_ok()
        });
        self.blocks.sort_by_key(|block| block.start);
        self.blocks.dedup_by_key(|block| block.start);
    }
}

fn remove_inline_style(spans: &mut Vec<InlineMark>, range: Range<usize>, style: InlineStyle) {
    let mut replacement = Vec::with_capacity(spans.len());
    for mark in spans.drain(..) {
        if mark.style != style || mark.end <= range.start || range.end <= mark.start {
            replacement.push(mark);
            continue;
        }

        if mark.start < range.start {
            replacement.push(InlineMark {
                start: mark.start,
                end: range.start,
                style: mark.style,
            });
        }
        if range.end < mark.end {
            replacement.push(InlineMark {
                start: range.end,
                end: mark.end,
                style: mark.style,
            });
        }
    }
    *spans = replacement;
}

fn transform_inline_marks(spans: &mut [InlineMark], range: Range<usize>, inserted_len: usize) {
    let removed_len = range.end.saturating_sub(range.start);
    let delta = inserted_len as isize - removed_len as isize;

    for mark in spans.iter_mut() {
        if removed_len == 0 {
            if range.start <= mark.start {
                mark.start = mark.start.saturating_add_signed(delta);
                mark.end = mark.end.saturating_add_signed(delta);
            } else if range.start < mark.end {
                mark.end = mark.end.saturating_add_signed(delta);
            }
            continue;
        }

        if mark.end <= range.start {
            continue;
        }
        if mark.start >= range.end {
            mark.start = mark.start.saturating_add_signed(delta);
            mark.end = mark.end.saturating_add_signed(delta);
            continue;
        }

        if mark.start < range.start && mark.end > range.end {
            mark.end = mark.end.saturating_add_signed(delta);
            continue;
        }

        if mark.start < range.start {
            mark.end = range.start;
            continue;
        }

        if mark.end > range.end {
            mark.start = range.start.saturating_add(inserted_len);
            mark.end = mark.end.saturating_add_signed(delta);
            continue;
        }

        mark.end = mark.start;
    }
}

fn transform_block_marks(
    blocks: &mut [BlockMark],
    old_text: &str,
    new_text: &str,
    range: Range<usize>,
) {
    let old_changed = paragraph_range_in_text(old_text, range.start.min(old_text.len()));
    let new_change_anchor = range.start.min(new_text.len());
    let new_changed = paragraph_range_in_text(new_text, new_change_anchor);

    let delta = new_text.len() as isize - old_text.len() as isize;

    for block in blocks.iter_mut() {
        if block.start < old_changed.start {
            continue;
        }
        if block.start >= old_changed.end {
            block.start = block.start.saturating_add_signed(delta);
            continue;
        }
        block.start = new_changed.start;
    }
}

fn paragraph_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, ch) in text.char_indices() {
        if ch == '\n' && index < text.len() {
            starts.push(index + 1);
        }
    }
    starts.sort_unstable();
    starts.dedup();
    starts
}

fn paragraph_ranges_intersecting(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let start = range.start.min(text.len());
    let end = range.end.min(text.len());
    let mut current = paragraph_range_in_text(text, start);
    let mut ranges = vec![current.clone()];

    while current.end < end {
        current = paragraph_range_in_text(text, current.end.saturating_add(1).min(text.len()));
        if ranges.last() != Some(&current) {
            ranges.push(current.clone());
        } else {
            break;
        }
    }

    ranges
}

fn paragraph_range_in_text(text: &str, offset: usize) -> Range<usize> {
    let offset = offset.min(text.len());
    let start = text[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let end = text[offset..]
        .find('\n')
        .map(|index| offset + index)
        .unwrap_or(text.len());
    start..end
}

pub fn single_change(old_text: &str, new_text: &str) -> Option<(Range<usize>, String)> {
    if old_text == new_text {
        return None;
    }

    let mut prefix = 0;
    let max_prefix = min(old_text.len(), new_text.len());
    while prefix < max_prefix && old_text.as_bytes()[prefix] == new_text.as_bytes()[prefix] {
        prefix += 1;
    }

    let mut old_suffix = old_text.len();
    let mut new_suffix = new_text.len();
    while old_suffix > prefix
        && new_suffix > prefix
        && old_text.as_bytes()[old_suffix - 1] == new_text.as_bytes()[new_suffix - 1]
    {
        old_suffix -= 1;
        new_suffix -= 1;
    }

    while prefix > 0 && (!old_text.is_char_boundary(prefix) || !new_text.is_char_boundary(prefix)) {
        prefix -= 1;
    }

    while old_suffix < old_text.len() && !old_text.is_char_boundary(old_suffix) {
        old_suffix += 1;
    }

    while new_suffix < new_text.len() && !new_text.is_char_boundary(new_suffix) {
        new_suffix += 1;
    }

    Some((prefix..old_suffix, new_text[prefix..new_suffix].to_string()))
}

// pub struct RichTextToolbar {}
//
// impl Render for RichTextToolbar {
//     fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
//         todo!()
//     }
// }
//
#[cfg(test)]
mod tests {
    use super::{BlockKind, InlineStyle, RichDocument, single_change};

    #[test]
    fn toggles_inline_style() {
        let mut document = RichDocument::new("abcdef".to_string());
        document.toggle_inline_style(1..4, InlineStyle::Bold);
        assert_eq!(document.spans.len(), 1);
        document.toggle_inline_style(1..4, InlineStyle::Bold);
        assert!(document.spans.is_empty());
    }

    #[test]
    fn applies_heading_by_paragraph() {
        let mut document = RichDocument::new("aa\nbb\ncc".to_string());
        document.set_block_kind_for_range(1..4, BlockKind::HeadingLarge);
        assert_eq!(document.block_kind_for_offset(0), BlockKind::HeadingLarge);
        assert_eq!(document.block_kind_for_offset(3), BlockKind::HeadingLarge);
        assert_eq!(document.block_kind_for_offset(6), BlockKind::Body);
    }

    #[test]
    fn computes_single_change() {
        let change = single_change("abcde", "abZZde").unwrap();
        assert_eq!(change.0, 2..3);
        assert_eq!(change.1, "ZZ");
    }

    #[test]
    fn insertion_at_styled_range_end_does_not_extend_style() {
        let mut document = RichDocument::new("あいう".to_string());
        let start = "あ".len();
        let end = "あいう".len();
        document.toggle_inline_style(start..end, InlineStyle::Strikethrough);
        document.replace_text(end..end, "えお");

        assert_eq!(document.text, "あいうえお");
        assert_eq!(document.spans.len(), 1);
        assert_eq!(document.spans[0].start, start);
        assert_eq!(document.spans[0].end, end);
        assert_eq!(document.spans[0].style, InlineStyle::Strikethrough);
    }

    #[test]
    fn single_change_stays_on_char_boundaries_for_multibyte_text() {
        let old_text = "ああああこのように文章が表示上おりか";
        let new_text = "ああああこのように文章が表示上おり\nか";

        let change = single_change(old_text, new_text).unwrap();

        assert!(old_text.is_char_boundary(change.0.start));
        assert!(old_text.is_char_boundary(change.0.end));
        assert_eq!(change.1, "\n");
    }
}
