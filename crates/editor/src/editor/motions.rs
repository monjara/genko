use std::ops::Range;

use rope::TextRope;
use tiny_segmenter::segment_ranges;

use super::command_types::{MotionKind, TextObjectModifier, TextObjectTarget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MotionRangeBehavior {
    Default,
    Change,
}

pub(crate) fn resolve_text_object_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    modifier: TextObjectModifier,
    target: TextObjectTarget,
) -> Option<Range<usize>> {
    match target {
        TextObjectTarget::Word => {
            let (start, end) = find_tiny_segmenter_word_range(rope, cursor_byte_offset)
                .or_else(|| find_word_range(rope, cursor_byte_offset, is_word_char))?;
            Some(match modifier {
                TextObjectModifier::Inner => start..end,
                TextObjectModifier::Around => expand_around_word(rope, start, end),
            })
        }
        TextObjectTarget::BigWord => {
            let (start, end) = find_word_range(rope, cursor_byte_offset, |ch| !ch.is_whitespace())?;
            Some(match modifier {
                TextObjectModifier::Inner => start..end,
                TextObjectModifier::Around => expand_around_word(rope, start, end),
            })
        }
        TextObjectTarget::DoubleQuote => {
            resolve_quoted_object_range(rope, cursor_byte_offset, '"', modifier)
        }
        TextObjectTarget::SingleQuote => {
            resolve_quoted_object_range(rope, cursor_byte_offset, '\'', modifier)
        }
        TextObjectTarget::Paren => {
            resolve_delimited_object_range(rope, cursor_byte_offset, '(', ')', modifier)
        }
        TextObjectTarget::Bracket => {
            resolve_delimited_object_range(rope, cursor_byte_offset, '[', ']', modifier)
        }
    }
}

pub(crate) fn resolve_motion_target(
    rope: &TextRope,
    cursor_byte_offset: usize,
    motion: MotionKind,
) -> Option<usize> {
    match motion {
        MotionKind::WordForward => resolve_forward_word_start(rope, cursor_byte_offset, false),
        MotionKind::BigWordForward => resolve_forward_word_start(rope, cursor_byte_offset, true),
        MotionKind::WordEndForward => resolve_forward_word_end(rope, cursor_byte_offset),
        MotionKind::WordBackward => resolve_backward_word_start(rope, cursor_byte_offset, false),
        MotionKind::BigWordBackward => resolve_backward_word_start(rope, cursor_byte_offset, true),
    }
}

pub(crate) fn resolve_motion_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    motion: MotionKind,
    behavior: MotionRangeBehavior,
) -> Option<Range<usize>> {
    if rope.len_bytes() == 0 {
        return None;
    }

    let start = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));
    match motion {
        MotionKind::WordForward
        | MotionKind::BigWordForward
        | MotionKind::WordBackward
        | MotionKind::BigWordBackward => {
            if behavior == MotionRangeBehavior::Change {
                let target = match motion {
                    MotionKind::WordForward => find_tiny_segmenter_word_range(rope, start)
                        .or_else(|| find_near_word_range(rope, start, false))
                        .map(|(_, end)| end),
                    MotionKind::BigWordForward => {
                        find_near_word_range(rope, start, true).map(|(_, end)| end)
                    }
                    MotionKind::WordBackward | MotionKind::BigWordBackward => {
                        resolve_motion_target(rope, start, motion)
                    }
                    MotionKind::WordEndForward => None,
                }
                .unwrap_or(rope.len_bytes());
                Some(start.min(target)..target.max(start))
            } else {
                let target = resolve_motion_target(rope, start, motion).unwrap_or(rope.len_bytes());
                Some(start.min(target)..target.max(start))
            }
        }
        MotionKind::WordEndForward => {
            let target = resolve_motion_target(rope, start, motion)?;
            let end = rope.next_char_boundary(target);
            Some(start.min(end)..end.max(start))
        }
    }
}

fn resolve_forward_word_start(
    rope: &TextRope,
    cursor_byte_offset: usize,
    big_word: bool,
) -> Option<usize> {
    if rope.len_bytes() == 0 {
        return None;
    }

    let mut offset = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));
    if offset >= rope.len_bytes() {
        offset = rope.previous_char_boundary(rope.len_bytes());
    }

    if !big_word && let Some(target) = find_tiny_segmenter_forward_word_start(rope, offset) {
        return Some(target);
    }

    let mut previous_character = rope.char_at(offset)?;
    offset = rope.next_char_boundary(offset);

    while offset < rope.len_bytes() {
        let character = rope.char_at(offset)?;
        if is_word_start_boundary(previous_character, character, big_word) {
            return Some(offset);
        }
        previous_character = character;
        offset = rope.next_char_boundary(offset);
    }

    None
}

fn resolve_forward_word_end(rope: &TextRope, cursor_byte_offset: usize) -> Option<usize> {
    if rope.len_bytes() == 0 {
        return None;
    }

    let start = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));
    if let Some(target) = find_tiny_segmenter_forward_word_end(rope, start) {
        return Some(target);
    }

    let mut offset = rope.next_char_boundary(start);
    if offset >= rope.len_bytes() {
        return Some(rope.previous_char_boundary(rope.len_bytes()));
    }

    let mut previous_offset = rope.previous_char_boundary(offset);
    let mut previous_character = rope.char_at(previous_offset)?;

    while offset < rope.len_bytes() {
        let character = rope.char_at(offset)?;
        if is_word_end_boundary(previous_character, character, false) {
            return Some(previous_offset);
        }
        previous_offset = offset;
        previous_character = character;
        offset = rope.next_char_boundary(offset);
    }

    Some(rope.previous_char_boundary(rope.len_bytes()))
}

fn resolve_backward_word_start(
    rope: &TextRope,
    cursor_byte_offset: usize,
    big_word: bool,
) -> Option<usize> {
    if rope.len_bytes() == 0 {
        return None;
    }

    let cursor = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));
    if cursor == 0 {
        return None;
    }

    if !big_word && let Some(target) = find_tiny_segmenter_backward_word_start(rope, cursor) {
        return Some(target);
    }

    let mut right_offset = rope.previous_char_boundary(cursor);
    let mut right_character = rope.char_at(right_offset)?;

    while right_offset > 0 {
        let left_offset = rope.previous_char_boundary(right_offset);
        let left_character = rope.char_at(left_offset)?;
        if is_word_start_boundary(left_character, right_character, big_word) {
            return Some(right_offset);
        }
        right_offset = left_offset;
        right_character = left_character;
    }

    Some(0)
}

fn find_near_word_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    ignore_punctuation: bool,
) -> Option<(usize, usize)> {
    let mut start = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));
    while start < rope.len_bytes() {
        let character = rope.char_at(start)?;
        if character_kind(character, ignore_punctuation) != CharacterKind::Whitespace {
            break;
        }
        start = rope.next_char_boundary(start);
    }

    if start >= rope.len_bytes() {
        return None;
    }

    let target_kind = character_kind(rope.char_at(start)?, ignore_punctuation);
    while start > 0 {
        let previous = rope.previous_char_boundary(start);
        let Some(character) = rope.char_at(previous) else {
            break;
        };
        if character_kind(character, ignore_punctuation) != target_kind {
            break;
        }
        start = previous;
    }

    let mut end = rope.next_char_boundary(start);
    while end < rope.len_bytes() {
        let Some(character) = rope.char_at(end) else {
            break;
        };
        if character_kind(character, ignore_punctuation) != target_kind {
            break;
        }
        end = rope.next_char_boundary(end);
    }

    Some((start, end))
}

const TINY_SEGMENTER_CONTEXT_CHARS: usize = 12;
const TINY_SEGMENTER_TRUST_MARGIN_BYTES: usize = 4;
const TINY_SEGMENTER_WINDOW_CHARS: [usize; 3] = [32, 96, 256];

struct SegmentWindow {
    start: usize,
    cursor: usize,
    end: usize,
    text: String,
}

fn find_tiny_segmenter_forward_word_start(
    rope: &TextRope,
    cursor_byte_offset: usize,
) -> Option<usize> {
    for after_chars in TINY_SEGMENTER_WINDOW_CHARS {
        let window = segment_window(
            rope,
            cursor_byte_offset,
            TINY_SEGMENTER_CONTEXT_CHARS,
            after_chars,
        );
        if !has_tiny_segmenter_context(&window.text) {
            return None;
        }

        for range in segment_ranges(&window.text) {
            if !is_tiny_segmenter_word(&window.text[range.clone()]) {
                continue;
            }
            if range.start <= window.cursor {
                continue;
            }
            if window.end < rope.len_bytes()
                && window.text.len().saturating_sub(range.start)
                    <= TINY_SEGMENTER_TRUST_MARGIN_BYTES
            {
                continue;
            }
            return Some(window.start + range.start);
        }
    }

    None
}

fn find_tiny_segmenter_forward_word_end(
    rope: &TextRope,
    cursor_byte_offset: usize,
) -> Option<usize> {
    for after_chars in TINY_SEGMENTER_WINDOW_CHARS {
        let window = segment_window(
            rope,
            cursor_byte_offset,
            TINY_SEGMENTER_CONTEXT_CHARS,
            after_chars,
        );
        if !has_tiny_segmenter_context(&window.text) {
            return None;
        }

        for range in segment_ranges(&window.text) {
            if !is_tiny_segmenter_word(&window.text[range.clone()]) {
                continue;
            }
            let segment_end = window.start + range.end;
            let target = rope.previous_char_boundary(segment_end);
            if target <= cursor_byte_offset {
                continue;
            }
            if window.end < rope.len_bytes()
                && window.text.len().saturating_sub(range.end) <= TINY_SEGMENTER_TRUST_MARGIN_BYTES
            {
                continue;
            }
            return Some(target);
        }
    }

    None
}

fn find_tiny_segmenter_backward_word_start(
    rope: &TextRope,
    cursor_byte_offset: usize,
) -> Option<usize> {
    for before_chars in TINY_SEGMENTER_WINDOW_CHARS {
        let window = segment_window(
            rope,
            cursor_byte_offset,
            before_chars,
            TINY_SEGMENTER_CONTEXT_CHARS,
        );
        if !has_tiny_segmenter_context(&window.text) {
            return None;
        }

        let mut candidate = None;
        for range in segment_ranges(&window.text) {
            if !is_tiny_segmenter_word(&window.text[range.clone()]) {
                continue;
            }
            if range.start < window.cursor {
                candidate = Some(range.start);
            }
        }

        if let Some(start) = candidate {
            if window.start > 0 && start <= TINY_SEGMENTER_TRUST_MARGIN_BYTES {
                continue;
            }
            return Some(window.start + start);
        }
    }

    None
}

fn find_tiny_segmenter_word_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
) -> Option<(usize, usize)> {
    for window_chars in TINY_SEGMENTER_WINDOW_CHARS {
        let window = segment_window(rope, cursor_byte_offset, window_chars, window_chars);
        if !has_tiny_segmenter_context(&window.text) {
            return None;
        }

        for range in segment_ranges(&window.text) {
            if !is_tiny_segmenter_word(&window.text[range.clone()]) {
                continue;
            }
            if range.start <= window.cursor && window.cursor < range.end
                || range.start >= window.cursor
            {
                if window.start > 0 && range.start <= TINY_SEGMENTER_TRUST_MARGIN_BYTES {
                    continue;
                }
                if window.end < rope.len_bytes()
                    && window.text.len().saturating_sub(range.end)
                        <= TINY_SEGMENTER_TRUST_MARGIN_BYTES
                {
                    continue;
                }
                return Some((window.start + range.start, window.start + range.end));
            }
        }
    }

    None
}

fn segment_window(
    rope: &TextRope,
    cursor_byte_offset: usize,
    before_chars: usize,
    after_chars: usize,
) -> SegmentWindow {
    let cursor = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));
    let mut start = cursor;
    for _ in 0..before_chars {
        if start == 0 {
            break;
        }
        start = rope.previous_char_boundary(start);
    }

    let mut end = cursor;
    for _ in 0..after_chars {
        if end >= rope.len_bytes() {
            break;
        }
        end = rope.next_char_boundary(end);
    }

    SegmentWindow {
        start,
        cursor: cursor - start,
        end,
        text: rope.slice(start..end),
    }
}

fn has_tiny_segmenter_context(text: &str) -> bool {
    text.chars().any(is_tiny_segmenter_character)
}

fn is_tiny_segmenter_word(text: &str) -> bool {
    text.chars()
        .any(|character| is_word_char(character) || is_tiny_segmenter_character(character))
}

fn is_tiny_segmenter_character(character: char) -> bool {
    matches!(
        character,
        '一'..='龠' | '々' | '〆' | 'ヵ' | 'ヶ' | 'ぁ'..='ん' | 'ァ'..='ヴ' | 'ー' | 'ｱ'..='ﾝ' | 'ﾞ'
    )
}

fn word_ranges<F>(rope: &TextRope, predicate: F) -> Option<Vec<Range<usize>>>
where
    F: Fn(char) -> bool,
{
    let mut ranges = Vec::new();
    let mut current_start = None;
    let mut index = 0;

    while let Some(ch) = rope.char_at(index) {
        if predicate(ch) {
            current_start.get_or_insert(index);
        } else if let Some(start) = current_start.take() {
            ranges.push(start..index);
        }
        index = rope.next_char_boundary(index);
    }

    if let Some(start) = current_start {
        ranges.push(start..rope.len_bytes());
    }

    (!ranges.is_empty()).then_some(ranges)
}

fn find_token_range_at_or_near_cursor(
    rope: &TextRope,
    ranges: &[Range<usize>],
    cursor_byte_offset: usize,
) -> Option<(usize, usize)> {
    let cursor = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));

    if let Some(range) = ranges
        .iter()
        .find(|range| range.start <= cursor && cursor < range.end)
    {
        return Some((range.start, range.end));
    }

    if let Some(range) = ranges.iter().find(|range| range.start >= cursor) {
        return Some((range.start, range.end));
    }

    ranges.last().map(|range| (range.start, range.end))
}

fn find_word_range<F>(
    rope: &TextRope,
    cursor_byte_offset: usize,
    predicate: F,
) -> Option<(usize, usize)>
where
    F: Fn(char) -> bool,
{
    let ranges = word_ranges(rope, predicate)?;
    find_token_range_at_or_near_cursor(rope, &ranges, cursor_byte_offset)
}

fn expand_around_word(rope: &TextRope, word_start: usize, word_end: usize) -> Range<usize> {
    let trailing_end = consume_whitespace_forward(rope, word_end);
    if trailing_end > word_end {
        word_start..trailing_end
    } else {
        consume_whitespace_backward(rope, word_start)..word_end
    }
}

fn resolve_quoted_object_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    quote: char,
    modifier: TextObjectModifier,
) -> Option<Range<usize>> {
    let (open, close) = find_enclosing_quotes(rope, cursor_byte_offset, quote)?;
    Some(match modifier {
        TextObjectModifier::Inner => (open + quote.len_utf8())..close,
        TextObjectModifier::Around => open..(close + quote.len_utf8()),
    })
}

fn find_enclosing_quotes(
    rope: &TextRope,
    cursor_byte_offset: usize,
    quote: char,
) -> Option<(usize, usize)> {
    let mut positions = Vec::new();
    let mut index = 0;

    while let Some(ch) = rope.char_at(index) {
        if ch == quote && !is_escaped(rope, index) {
            positions.push(index);
        }
        index = rope.next_char_boundary(index);
    }

    if positions.len() < 2 {
        return None;
    }

    for pair in positions.windows(2) {
        let open = pair[0];
        let close = pair[1];
        let inside_start = open + quote.len_utf8();
        if (inside_start..=close).contains(&cursor_byte_offset)
            || (open..close).contains(&cursor_byte_offset)
        {
            return Some((open, close));
        }
    }

    None
}

fn resolve_delimited_object_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    open: char,
    close: char,
    modifier: TextObjectModifier,
) -> Option<Range<usize>> {
    let (open_index, close_index) = find_enclosing_pair(rope, cursor_byte_offset, open, close)?;
    Some(match modifier {
        TextObjectModifier::Inner => (open_index + open.len_utf8())..close_index,
        TextObjectModifier::Around => open_index..(close_index + close.len_utf8()),
    })
}

fn find_enclosing_pair(
    rope: &TextRope,
    cursor_byte_offset: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let chars = collect_char_positions(rope);
    let cursor_index = chars.partition_point(|(byte_index, _)| *byte_index < cursor_byte_offset);

    for left_index in (0..chars.len()).rev() {
        let (open_offset, ch) = chars[left_index];
        if ch != open || open_offset > cursor_byte_offset {
            continue;
        }

        let mut depth = 0usize;
        for (close_offset, next_ch) in chars.iter().skip(left_index) {
            if *next_ch == open {
                depth += 1;
            } else if *next_ch == close {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let inside_start = open_offset + open.len_utf8();
                    if (inside_start..=*close_offset).contains(&cursor_byte_offset)
                        || (open_offset..*close_offset).contains(&cursor_byte_offset)
                        || cursor_index == chars.len()
                            && *close_offset == rope.len_bytes() - close.len_utf8()
                    {
                        return Some((open_offset, *close_offset));
                    }
                    break;
                }
            }
        }
    }

    None
}

fn collect_char_positions(rope: &TextRope) -> Vec<(usize, char)> {
    let mut chars = Vec::new();
    let mut index = 0;
    while let Some(ch) = rope.char_at(index) {
        chars.push((index, ch));
        index = rope.next_char_boundary(index);
    }
    chars
}

fn is_escaped(rope: &TextRope, byte_index: usize) -> bool {
    if byte_index == 0 {
        return false;
    }

    let mut index = rope.previous_char_boundary(byte_index);
    let mut backslashes = 0usize;
    loop {
        let Some(ch) = rope.char_at(index) else {
            break;
        };
        if ch != '\\' {
            break;
        }
        backslashes += 1;
        if index == 0 {
            break;
        }
        index = rope.previous_char_boundary(index);
    }

    backslashes % 2 == 1
}

fn consume_whitespace_forward(rope: &TextRope, mut offset: usize) -> usize {
    while let Some(ch) = rope.char_at(offset) {
        if !ch.is_whitespace() {
            break;
        }
        offset = rope.next_char_boundary(offset);
    }
    offset
}

fn consume_whitespace_backward(rope: &TextRope, offset: usize) -> usize {
    let mut start = offset;
    while start > 0 {
        let previous = rope.previous_char_boundary(start);
        let Some(ch) = rope.char_at(previous) else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        start = previous;
    }
    start
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharacterKind {
    Whitespace,
    Punctuation,
    Word,
}

fn character_kind(character: char, ignore_punctuation: bool) -> CharacterKind {
    if character.is_whitespace() {
        CharacterKind::Whitespace
    } else if ignore_punctuation || is_word_char(character) {
        CharacterKind::Word
    } else {
        CharacterKind::Punctuation
    }
}

fn is_word_start_boundary(left: char, right: char, ignore_punctuation: bool) -> bool {
    character_kind(left, ignore_punctuation) != character_kind(right, ignore_punctuation)
        && !right.is_whitespace()
}

fn is_word_end_boundary(left: char, right: char, ignore_punctuation: bool) -> bool {
    character_kind(left, ignore_punctuation) != character_kind(right, ignore_punctuation)
        && !left.is_whitespace()
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
