use rope::TextRope;

use super::*;
use crate::vim::state::BlockRegister;

fn rope(text: &str) -> TextRope {
    TextRope::from_str(text)
}

fn resolve_text_range(
    text: &str,
    cursor_byte_offset: usize,
    modifier: TextObjectModifier,
    target: TextObjectTarget,
) -> Option<std::ops::Range<usize>> {
    resolve_text_object_range(&rope(text), cursor_byte_offset, modifier, target)
}

fn resolve_motion(text: &str, cursor_byte_offset: usize, motion: MotionKind) -> Option<usize> {
    resolve_motion_target(&rope(text), cursor_byte_offset, motion)
}

fn resolve_motion_edit_range(
    text: &str,
    cursor_byte_offset: usize,
    motion: MotionKind,
    operator: VimOperator,
) -> Option<std::ops::Range<usize>> {
    resolve_motion_range(&rope(text), cursor_byte_offset, motion, operator)
}

fn resolve_repeat_range(
    text: &str,
    cursor_byte_offset: usize,
    target: RepeatTarget,
    is_change: bool,
) -> Option<std::ops::Range<usize>> {
    resolve_repeat_target_range(&rope(text), cursor_byte_offset, target, is_change)
}

#[test]
fn inner_word_targets_current_word() {
    assert_eq!(
        resolve_text_range(
            "alpha beta",
            2,
            TextObjectModifier::Inner,
            TextObjectTarget::Word
        ),
        Some(0..5)
    );
}

#[test]
fn inner_word_skips_forward_from_whitespace() {
    assert_eq!(
        resolve_text_range(
            "alpha beta",
            5,
            TextObjectModifier::Inner,
            TextObjectTarget::Word
        ),
        Some(6..10)
    );
}

#[test]
fn around_word_prefers_trailing_spaces() {
    assert_eq!(
        resolve_text_range(
            "alpha   beta",
            1,
            TextObjectModifier::Around,
            TextObjectTarget::Word,
        ),
        Some(0..8)
    );
}

#[test]
fn around_word_uses_leading_spaces_when_no_trailing_spaces() {
    assert_eq!(
        resolve_text_range(
            "alpha",
            2,
            TextObjectModifier::Around,
            TextObjectTarget::Word
        ),
        Some(0..5)
    );
    assert_eq!(
        resolve_text_range(
            "   alpha",
            4,
            TextObjectModifier::Around,
            TextObjectTarget::Word
        ),
        Some(0..8)
    );
}

#[test]
fn big_word_includes_punctuation_until_whitespace() {
    assert_eq!(
        resolve_text_range(
            "foo.bar baz",
            2,
            TextObjectModifier::Inner,
            TextObjectTarget::BigWord
        ),
        Some(0..7)
    );
}

#[cfg(feature = "embedded-ipadic")]
#[test]
fn japanese_word_uses_lindera_boundaries() {
    assert_eq!(
        resolve_text_range(
            "関西国際空港限定トートバッグ",
            "関西国際空港".len() + 1,
            TextObjectModifier::Inner,
            TextObjectTarget::Word,
        ),
        Some("関西国際空港".len().."関西国際空港限定".len())
    );
}

#[test]
fn around_japanese_word_expands_whitespace() {
    let text = "関西国際空港 限定 トートバッグ";
    let cursor = text.find("限定").unwrap();
    assert_eq!(
        resolve_text_range(
            text,
            cursor,
            TextObjectModifier::Around,
            TextObjectTarget::Word
        ),
        Some("関西国際空港 ".len().."関西国際空港 限定 ".len())
    );
}

#[test]
fn double_quote_objects_work() {
    assert_eq!(
        resolve_text_range(
            r#"say "hello world" now"#,
            7,
            TextObjectModifier::Inner,
            TextObjectTarget::DoubleQuote,
        ),
        Some(5..16)
    );
    assert_eq!(
        resolve_text_range(
            r#"say "hello world" now"#,
            7,
            TextObjectModifier::Around,
            TextObjectTarget::DoubleQuote,
        ),
        Some(4..17)
    );
}

#[test]
fn single_quote_objects_work() {
    assert_eq!(
        resolve_text_range(
            "say 'hello' now",
            7,
            TextObjectModifier::Inner,
            TextObjectTarget::SingleQuote,
        ),
        Some(5..10)
    );
}

#[test]
fn paren_objects_work() {
    assert_eq!(
        resolve_text_range(
            "call(foo(bar))",
            10,
            TextObjectModifier::Inner,
            TextObjectTarget::Paren,
        ),
        Some(9..12)
    );
    assert_eq!(
        resolve_text_range(
            "call(foo(bar))",
            10,
            TextObjectModifier::Around,
            TextObjectTarget::Paren,
        ),
        Some(8..13)
    );
}

#[test]
fn bracket_objects_work() {
    assert_eq!(
        resolve_text_range(
            "arr[one[two]]",
            9,
            TextObjectModifier::Inner,
            TextObjectTarget::Bracket,
        ),
        Some(8..11)
    );
    assert_eq!(
        resolve_text_range(
            "arr[one[two]]",
            9,
            TextObjectModifier::Around,
            TextObjectTarget::Bracket,
        ),
        Some(7..12)
    );
}

#[test]
fn word_forward_moves_to_next_word_start() {
    assert_eq!(
        resolve_motion("alpha beta gamma", 0, MotionKind::WordForward),
        Some(6)
    );
    assert_eq!(
        resolve_motion("alpha beta gamma", 5, MotionKind::WordForward),
        Some(6)
    );
}

#[test]
fn big_word_forward_skips_until_next_whitespace_boundary() {
    assert_eq!(
        resolve_motion("foo.bar baz", 0, MotionKind::BigWordForward),
        Some(8)
    );
}

#[test]
fn word_end_moves_to_current_or_next_word_end() {
    assert_eq!(
        resolve_motion("alpha beta", 1, MotionKind::WordEndForward),
        Some(4)
    );
    assert_eq!(
        resolve_motion("alpha beta", 5, MotionKind::WordEndForward),
        Some(9)
    );
}

#[cfg(feature = "embedded-ipadic")]
#[test]
fn japanese_word_forward_uses_lindera_boundaries() {
    let text = "関西国際空港限定トートバッグ";
    assert_eq!(
        resolve_motion(text, 0, MotionKind::WordForward),
        Some("関西国際空港".len())
    );
}

#[test]
fn delete_word_motion_targets_next_word_start() {
    assert_eq!(
        resolve_motion_edit_range(
            "alpha beta",
            0,
            MotionKind::WordForward,
            VimOperator::Delete
        ),
        Some(0..6)
    );
}

#[test]
fn change_word_motion_stops_at_current_word_end() {
    assert_eq!(
        resolve_motion_edit_range(
            "alpha beta",
            0,
            MotionKind::WordForward,
            VimOperator::Change
        ),
        Some(0..5)
    );
}

#[test]
fn end_motion_range_includes_target_character() {
    assert_eq!(
        resolve_motion_edit_range(
            "alpha beta",
            0,
            MotionKind::WordEndForward,
            VimOperator::Delete,
        ),
        Some(0..5)
    );
}

#[test]
fn current_column_cell_range_returns_current_column_bounds() {
    assert_eq!(current_column_cell_range(0, 4, 10), Some(0..4));
    assert_eq!(current_column_cell_range(4, 4, 10), Some(4..8));
    assert_eq!(current_column_cell_range(8, 4, 10), Some(8..10));
}

#[test]
fn current_column_cell_range_uses_last_non_empty_column_for_end_cursor() {
    assert_eq!(current_column_cell_range(10, 4, 10), Some(8..10));
}

#[test]
fn current_column_cell_range_returns_none_for_empty_document() {
    assert_eq!(current_column_cell_range(0, 4, 0), None);
    assert_eq!(current_column_cell_range(0, 0, 10), None);
}

#[test]
fn block_paste_operations_insert_one_string_per_column() {
    let register = BlockRegister {
        row_count: 5,
        column_count: 1,
        cells: vec![
            "サ".into(),
            "ン".into(),
            "プ".into(),
            "ル".into(),
            "".into(),
        ],
    };

    assert_eq!(
        block_paste_operations(8, 24, &register),
        vec![(8, "サンプル".into())]
    );
}

#[test]
fn block_paste_operations_keep_columns_separate() {
    let register = BlockRegister {
        row_count: 2,
        column_count: 2,
        cells: vec!["A".into(), "B".into(), "C".into(), "D".into()],
    };

    assert_eq!(
        block_paste_operations(3, 10, &register),
        vec![(3, "AB".into()), (13, "CD".into())]
    );
}

#[test]
fn resolve_repeat_target_range_ignores_line_targets() {
    assert_eq!(
        resolve_repeat_range("abc", 0, RepeatTarget::Line, false),
        None
    );
}
