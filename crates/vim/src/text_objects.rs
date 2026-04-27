use std::ops::Range;
use std::sync::OnceLock;

use env::lindera_dictinary_path;
use lindera::dictionary::{Dictionary, load_dictionary};
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer;
use rope::TextRope;

use crate::state::{MotionKind, RepeatTarget, TextObjectModifier, TextObjectTarget, VimOperator};

static JAPANESE_TOKENIZER: OnceLock<Result<Tokenizer, String>> = OnceLock::new();

pub(crate) fn resolve_text_object_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    modifier: TextObjectModifier,
    target: TextObjectTarget,
) -> Option<Range<usize>> {
    match target {
        TextObjectTarget::Word => {
            let (start, end) = find_japanese_word_range(rope, cursor_byte_offset)
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
    }
}

pub(crate) fn resolve_motion_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    motion: MotionKind,
    operator: VimOperator,
) -> Option<Range<usize>> {
    if rope.len_bytes() == 0 {
        return None;
    }

    let start = rope.floor_char_boundary(cursor_byte_offset.min(rope.len_bytes()));
    match motion {
        MotionKind::WordForward | MotionKind::BigWordForward => {
            if operator == VimOperator::Change {
                let target = match motion {
                    MotionKind::WordForward => find_japanese_word_range(rope, start)
                        .map(|(_, end)| end)
                        .or_else(|| find_word_range(rope, start, is_word_char).map(|(_, end)| end)),
                    MotionKind::BigWordForward => {
                        find_word_range(rope, start, |ch| !ch.is_whitespace()).map(|(_, end)| end)
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

pub(crate) fn resolve_repeat_target_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    target: RepeatTarget,
    is_change: bool,
) -> Option<Range<usize>> {
    match target {
        RepeatTarget::Motion(motion) => resolve_motion_range(
            rope,
            cursor_byte_offset,
            motion,
            if is_change {
                VimOperator::Change
            } else {
                VimOperator::Delete
            },
        ),
        RepeatTarget::TextObject(modifier, target) => {
            resolve_text_object_range(rope, cursor_byte_offset, modifier, target)
        }
        RepeatTarget::Line => None,
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

    let ranges = token_ranges_for_target(
        rope,
        if cursor_byte_offset < rope.len_bytes() {
            cursor_byte_offset
        } else {
            rope.previous_char_boundary(rope.len_bytes())
        },
        if big_word {
            TextObjectTarget::BigWord
        } else {
            TextObjectTarget::Word
        },
    )?;

    let current = current_token_index(&ranges, cursor_byte_offset);
    if let Some(index) = current {
        return ranges.get(index + 1).map(|range| range.start);
    }

    ranges
        .iter()
        .find(|range| range.start >= cursor_byte_offset)
        .map(|range| range.start)
}

fn resolve_forward_word_end(rope: &TextRope, cursor_byte_offset: usize) -> Option<usize> {
    if rope.len_bytes() == 0 {
        return None;
    }

    let ranges = token_ranges_for_target(
        rope,
        if cursor_byte_offset < rope.len_bytes() {
            cursor_byte_offset
        } else {
            rope.previous_char_boundary(rope.len_bytes())
        },
        TextObjectTarget::Word,
    )?;

    let current = current_token_index(&ranges, cursor_byte_offset);
    if let Some(index) = current {
        let range = &ranges[index];
        if cursor_byte_offset < range.end.saturating_sub(1) {
            return Some(rope.previous_char_boundary(range.end));
        }
        return ranges
            .get(index + 1)
            .map(|range| rope.previous_char_boundary(range.end));
    }

    ranges
        .iter()
        .find(|range| range.start >= cursor_byte_offset)
        .map(|range| rope.previous_char_boundary(range.end))
}

fn token_ranges_for_target(
    rope: &TextRope,
    cursor_byte_offset: usize,
    target: TextObjectTarget,
) -> Option<Vec<Range<usize>>> {
    match target {
        TextObjectTarget::Word => japanese_token_ranges(rope)
            .or_else(|| word_ranges(rope, is_word_char))
            .filter(|ranges| !ranges.is_empty())
            .or_else(|| {
                let _ = cursor_byte_offset;
                None
            }),
        TextObjectTarget::BigWord => word_ranges(rope, |ch| !ch.is_whitespace()),
        _ => None,
    }
}

fn japanese_token_ranges(rope: &TextRope) -> Option<Vec<Range<usize>>> {
    if !contains_non_ascii(rope) {
        return None;
    }

    let tokenizer = japanese_tokenizer()?;
    let text = rope.to_string();
    let tokens = tokenizer.tokenize(&text).ok()?;
    if tokens.is_empty() {
        return None;
    }

    let mut ranges = Vec::with_capacity(tokens.len());
    let mut offset = 0usize;
    for token in tokens {
        let surface = token.surface.as_ref();
        let relative_start = text[offset..].find(surface)?;
        let start = offset + relative_start;
        let end = start + surface.len();
        ranges.push(start..end);
        offset = end;
    }
    Some(ranges)
}

fn contains_non_ascii(rope: &TextRope) -> bool {
    let mut offset = 0;
    while let Some(ch) = rope.char_at(offset) {
        if !ch.is_ascii() {
            return true;
        }
        offset = rope.next_char_boundary(offset);
    }
    false
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

fn current_token_index(ranges: &[Range<usize>], cursor_byte_offset: usize) -> Option<usize> {
    ranges
        .iter()
        .position(|range| range.start <= cursor_byte_offset && cursor_byte_offset < range.end)
}

fn find_japanese_word_range(rope: &TextRope, cursor_byte_offset: usize) -> Option<(usize, usize)> {
    let ranges = japanese_token_ranges(rope)?;
    find_token_range_at_or_near_cursor(rope, &ranges, cursor_byte_offset)
}

fn japanese_tokenizer() -> Option<&'static Tokenizer> {
    JAPANESE_TOKENIZER
        .get_or_init(|| {
            let dictionary = load_japanese_dictionary()?;
            let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
            Ok(Tokenizer::new(segmenter))
        })
        .as_ref()
        .ok()
}

fn load_japanese_dictionary() -> Result<Dictionary, String> {
    if let Some(path) = lindera_dictinary_path()
        && !path.is_empty()
    {
        return load_dictionary(path.to_string_lossy().as_ref())
            .map_err(|error| format!("failed to load lindera dictionary from {path:?}: {error}"));
    }

    #[cfg(feature = "embedded-ipadic")]
    {
        load_dictionary("embedded://ipadic")
            .map_err(|error| format!("failed to load embedded lindera dictionary: {error}"))
    }

    #[cfg(not(feature = "embedded-ipadic"))]
    {
        Err(format!(
            "japanese tokenizer is disabled; enable the `embedded-ipadic` feature or set {path:?}"
        ))
    }
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

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
