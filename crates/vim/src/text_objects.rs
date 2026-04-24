use std::env;
use std::ops::Range;
use std::sync::OnceLock;

use lindera::dictionary::{Dictionary, load_dictionary};
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer;

use crate::state::{MotionKind, RepeatTarget, TextObjectModifier, TextObjectTarget, VimOperator};

const LINDERA_DICTIONARY_PATH_ENV: &str = "GENKO_LINDERA_DICTIONARY_PATH";

static JAPANESE_TOKENIZER: OnceLock<Result<Tokenizer, String>> = OnceLock::new();

pub(crate) fn resolve_text_object_range(
    text: &str,
    cursor_byte_offset: usize,
    modifier: TextObjectModifier,
    target: TextObjectTarget,
) -> Option<Range<usize>> {
    match target {
        TextObjectTarget::Word => {
            let (start, end) = find_japanese_word_range(text, cursor_byte_offset)
                .or_else(|| find_word_range(text, cursor_byte_offset, is_word_char))?;
            Some(match modifier {
                TextObjectModifier::Inner => start..end,
                TextObjectModifier::Around => expand_around_word(text, start, end),
            })
        }
        TextObjectTarget::BigWord => {
            let (start, end) = find_word_range(text, cursor_byte_offset, |ch| !ch.is_whitespace())?;
            Some(match modifier {
                TextObjectModifier::Inner => start..end,
                TextObjectModifier::Around => expand_around_word(text, start, end),
            })
        }
        TextObjectTarget::DoubleQuote => {
            resolve_quoted_object_range(text, cursor_byte_offset, '"', modifier)
        }
        TextObjectTarget::SingleQuote => {
            resolve_quoted_object_range(text, cursor_byte_offset, '\'', modifier)
        }
        TextObjectTarget::Paren => {
            resolve_delimited_object_range(text, cursor_byte_offset, '(', ')', modifier)
        }
        TextObjectTarget::Bracket => {
            resolve_delimited_object_range(text, cursor_byte_offset, '[', ']', modifier)
        }
    }
}

pub(crate) fn resolve_motion_target(
    text: &str,
    cursor_byte_offset: usize,
    motion: MotionKind,
) -> Option<usize> {
    match motion {
        MotionKind::WordForward => resolve_forward_word_start(text, cursor_byte_offset, false),
        MotionKind::BigWordForward => resolve_forward_word_start(text, cursor_byte_offset, true),
        MotionKind::WordEndForward => resolve_forward_word_end(text, cursor_byte_offset),
    }
}

pub(crate) fn resolve_motion_range(
    text: &str,
    cursor_byte_offset: usize,
    motion: MotionKind,
    operator: VimOperator,
) -> Option<Range<usize>> {
    if text.is_empty() {
        return None;
    }

    let start = cursor_byte_offset.min(text.len());
    match motion {
        MotionKind::WordForward | MotionKind::BigWordForward => {
            if operator == VimOperator::Change {
                let target = match motion {
                    MotionKind::WordForward => find_japanese_word_range(text, start)
                        .map(|(_, end)| end)
                        .or_else(|| find_word_range(text, start, is_word_char).map(|(_, end)| end)),
                    MotionKind::BigWordForward => {
                        find_word_range(text, start, |ch| !ch.is_whitespace()).map(|(_, end)| end)
                    }
                    MotionKind::WordEndForward => None,
                }
                .unwrap_or(text.len());
                Some(start.min(target)..target.max(start))
            } else {
                let target = resolve_motion_target(text, start, motion).unwrap_or(text.len());
                Some(start.min(target)..target.max(start))
            }
        }
        MotionKind::WordEndForward => {
            let target = resolve_motion_target(text, start, motion)?;
            let end = next_char_end(text, target);
            Some(start.min(end)..end.max(start))
        }
    }
}

pub(crate) fn resolve_repeat_target_range(
    text: &str,
    cursor_byte_offset: usize,
    target: RepeatTarget,
    is_change: bool,
) -> Option<Range<usize>> {
    match target {
        RepeatTarget::Motion(motion) => resolve_motion_range(
            text,
            cursor_byte_offset,
            motion,
            if is_change {
                VimOperator::Change
            } else {
                VimOperator::Delete
            },
        ),
        RepeatTarget::TextObject(modifier, target) => {
            resolve_text_object_range(text, cursor_byte_offset, modifier, target)
        }
        RepeatTarget::Line => None,
    }
}

pub(crate) fn next_char_end(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let ch = text[offset..].chars().next().unwrap();
    offset + ch.len_utf8()
}

pub(crate) fn inserted_text_between(before: &str, after: &str) -> String {
    let mut prefix_len = 0usize;
    let mut before_chars = before.chars();
    let mut after_chars = after.chars();
    loop {
        match (before_chars.next(), after_chars.next()) {
            (Some(before_ch), Some(after_ch)) if before_ch == after_ch => {
                prefix_len += before_ch.len_utf8();
            }
            _ => break,
        }
    }

    let before_remaining = &before[prefix_len..];
    let after_remaining = &after[prefix_len..];
    let mut shared_suffix_len = 0usize;
    let mut before_rev = before_remaining.chars().rev();
    let mut after_rev = after_remaining.chars().rev();
    loop {
        match (before_rev.next(), after_rev.next()) {
            (Some(before_ch), Some(after_ch))
                if before_ch == after_ch
                    && shared_suffix_len + before_ch.len_utf8() <= before_remaining.len()
                    && shared_suffix_len + after_ch.len_utf8() <= after_remaining.len() =>
            {
                shared_suffix_len += after_ch.len_utf8();
            }
            _ => break,
        }
    }

    let after_end = after.len().saturating_sub(shared_suffix_len);
    after[prefix_len..after_end].to_string()
}

fn resolve_forward_word_start(
    text: &str,
    cursor_byte_offset: usize,
    big_word: bool,
) -> Option<usize> {
    if text.is_empty() {
        return None;
    }

    let ranges = token_ranges_for_target(
        text,
        if cursor_byte_offset < text.len() {
            cursor_byte_offset
        } else {
            text.len().saturating_sub(1)
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

fn resolve_forward_word_end(text: &str, cursor_byte_offset: usize) -> Option<usize> {
    if text.is_empty() {
        return None;
    }

    let ranges = token_ranges_for_target(
        text,
        if cursor_byte_offset < text.len() {
            cursor_byte_offset
        } else {
            text.len().saturating_sub(1)
        },
        TextObjectTarget::Word,
    )?;

    let current = current_token_index(&ranges, cursor_byte_offset);
    if let Some(index) = current {
        let range = &ranges[index];
        if cursor_byte_offset < range.end.saturating_sub(1) {
            return Some(previous_char_start(text, range.end));
        }
        return ranges
            .get(index + 1)
            .map(|range| previous_char_start(text, range.end));
    }

    ranges
        .iter()
        .find(|range| range.start >= cursor_byte_offset)
        .map(|range| previous_char_start(text, range.end))
}

fn token_ranges_for_target(
    text: &str,
    cursor_byte_offset: usize,
    target: TextObjectTarget,
) -> Option<Vec<Range<usize>>> {
    match target {
        TextObjectTarget::Word => japanese_token_ranges(text)
            .or_else(|| word_ranges(text, is_word_char))
            .filter(|ranges| !ranges.is_empty())
            .or_else(|| {
                let _ = cursor_byte_offset;
                None
            }),
        TextObjectTarget::BigWord => word_ranges(text, |ch| !ch.is_whitespace()),
        _ => None,
    }
}

fn japanese_token_ranges(text: &str) -> Option<Vec<Range<usize>>> {
    let tokenizer = japanese_tokenizer()?;
    let tokens = tokenizer.tokenize(text).ok()?;
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

fn word_ranges<F>(text: &str, predicate: F) -> Option<Vec<Range<usize>>>
where
    F: Fn(char) -> bool,
{
    let mut ranges = Vec::new();
    let mut current_start = None;

    for (index, ch) in text.char_indices() {
        if predicate(ch) {
            current_start.get_or_insert(index);
        } else if let Some(start) = current_start.take() {
            ranges.push(start..index);
        }
    }

    if let Some(start) = current_start {
        ranges.push(start..text.len());
    }

    (!ranges.is_empty()).then_some(ranges)
}

fn current_token_index(ranges: &[Range<usize>], cursor_byte_offset: usize) -> Option<usize> {
    ranges
        .iter()
        .position(|range| range.start <= cursor_byte_offset && cursor_byte_offset < range.end)
}

fn find_japanese_word_range(text: &str, cursor_byte_offset: usize) -> Option<(usize, usize)> {
    let ranges = japanese_token_ranges(text)?;
    find_token_range_at_or_near_cursor(text, &ranges, cursor_byte_offset)
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
    if let Some(path) = env::var_os(LINDERA_DICTIONARY_PATH_ENV)
        && !path.is_empty()
    {
        return load_dictionary(path.to_string_lossy().as_ref()).map_err(|error| {
            format!("failed to load lindera dictionary from {LINDERA_DICTIONARY_PATH_ENV}: {error}")
        });
    }

    #[cfg(feature = "embedded-ipadic")]
    {
        load_dictionary("embedded://ipadic")
            .map_err(|error| format!("failed to load embedded lindera dictionary: {error}"))
    }

    #[cfg(not(feature = "embedded-ipadic"))]
    {
        Err(format!(
            "japanese tokenizer is disabled; enable the `embedded-ipadic` feature or set {LINDERA_DICTIONARY_PATH_ENV}"
        ))
    }
}

fn find_token_range_at_or_near_cursor(
    text: &str,
    ranges: &[Range<usize>],
    cursor_byte_offset: usize,
) -> Option<(usize, usize)> {
    let mut cursor = cursor_byte_offset.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }

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

fn find_word_range<F>(text: &str, cursor_byte_offset: usize, predicate: F) -> Option<(usize, usize)>
where
    F: Fn(char) -> bool,
{
    if text.is_empty() {
        return None;
    }

    let char_positions: Vec<(usize, char)> = text.char_indices().collect();
    if char_positions.is_empty() {
        return None;
    }

    let mut current_index =
        char_positions.partition_point(|(byte_index, _)| *byte_index < cursor_byte_offset);
    if current_index == char_positions.len() {
        current_index = current_index.saturating_sub(1);
    }

    if !predicate(char_positions[current_index].1) {
        if let Some(next_index) = char_positions
            .iter()
            .enumerate()
            .skip(current_index + 1)
            .find_map(|(index, (_, ch))| predicate(*ch).then_some(index))
        {
            current_index = next_index;
        } else if let Some(previous_index) = char_positions[..=current_index]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, (_, ch))| predicate(*ch).then_some(index))
        {
            current_index = previous_index;
        } else {
            return None;
        }
    }

    let mut start_index = current_index;
    while start_index > 0 && predicate(char_positions[start_index - 1].1) {
        start_index -= 1;
    }

    let mut end_index = current_index + 1;
    while end_index < char_positions.len() && predicate(char_positions[end_index].1) {
        end_index += 1;
    }

    let start = char_positions[start_index].0;
    let end = char_positions
        .get(end_index)
        .map_or(text.len(), |(byte_index, _)| *byte_index);
    Some((start, end))
}

fn expand_around_word(text: &str, word_start: usize, word_end: usize) -> Range<usize> {
    let trailing_end = consume_whitespace_forward(text, word_end);
    if trailing_end > word_end {
        word_start..trailing_end
    } else {
        consume_whitespace_backward(text, word_start)..word_end
    }
}

fn resolve_quoted_object_range(
    text: &str,
    cursor_byte_offset: usize,
    quote: char,
    modifier: TextObjectModifier,
) -> Option<Range<usize>> {
    let (open, close) = find_enclosing_quotes(text, cursor_byte_offset, quote)?;
    Some(match modifier {
        TextObjectModifier::Inner => (open + quote.len_utf8())..close,
        TextObjectModifier::Around => open..(close + quote.len_utf8()),
    })
}

fn find_enclosing_quotes(
    text: &str,
    cursor_byte_offset: usize,
    quote: char,
) -> Option<(usize, usize)> {
    let positions: Vec<usize> = text
        .char_indices()
        .filter_map(|(index, ch)| (ch == quote && !is_escaped(text, index)).then_some(index))
        .collect();
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
    text: &str,
    cursor_byte_offset: usize,
    open: char,
    close: char,
    modifier: TextObjectModifier,
) -> Option<Range<usize>> {
    let (open_index, close_index) = find_enclosing_pair(text, cursor_byte_offset, open, close)?;
    Some(match modifier {
        TextObjectModifier::Inner => (open_index + open.len_utf8())..close_index,
        TextObjectModifier::Around => open_index..(close_index + close.len_utf8()),
    })
}

fn find_enclosing_pair(
    text: &str,
    cursor_byte_offset: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
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
                            && *close_offset == text.len() - close.len_utf8()
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

fn is_escaped(text: &str, byte_index: usize) -> bool {
    if byte_index == 0 {
        return false;
    }

    let mut index = previous_char_start(text, byte_index);
    let mut backslashes = 0usize;
    loop {
        let ch = text[index..].chars().next().unwrap();
        if ch != '\\' {
            break;
        }
        backslashes += 1;
        if index == 0 {
            break;
        }
        index = previous_char_start(text, index);
    }

    backslashes % 2 == 1
}

fn consume_whitespace_forward(text: &str, mut offset: usize) -> usize {
    while let Some(ch) = text[offset..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        offset += ch.len_utf8();
    }
    offset
}

fn consume_whitespace_backward(text: &str, offset: usize) -> usize {
    let mut start = offset;
    while start > 0 {
        let previous = previous_char_start(text, start);
        let ch = text[previous..start].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        start = previous;
    }
    start
}

fn previous_char_start(text: &str, offset: usize) -> usize {
    let mut index = offset.saturating_sub(1);
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
