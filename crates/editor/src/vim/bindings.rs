use gpui::{App, KeyBinding};
use settings::AppSettings;

use super::state::{TextObjectModifier, VimOperator, operator_context};
use super::{
    VimAppend, VimBlockAppendAfter, VimBlockInsertBefore, VimChangeOperator, VimDeleteChar,
    VimDeleteOperator, VimEnterInsertMode, VimMoveBigWordForward, VimMoveDown, VimMoveLeft,
    VimMoveRight, VimMoveUp, VimMoveWordEndForward, VimMoveWordForward, VimNormalMode,
    VimOpenNextColumn, VimPasteAfter, VimPasteBefore, VimRedo, VimRepeatLastChange,
    VimTextObjectAround, VimTextObjectBigWord, VimTextObjectBracket, VimTextObjectDoubleQuote,
    VimTextObjectInner, VimTextObjectParen, VimTextObjectSingleQuote, VimTextObjectWord, VimUndo,
    VimVisualBlockMode, VimVisualMode, VimYankOperator,
};

const MODE_NORMAL: &str = "vim_mode == normal";
const MODE_INSERT: &str = "vim_mode == insert";
const MODE_VISUAL: &str = "vim_mode == visual";
const MODE_VISUAL_BLOCK: &str = "vim_mode == visual_block";
const MODE_ANY_VISUAL: &str =
    "vim_mode == normal || vim_mode == visual || vim_mode == visual_block";

pub(crate) fn init(cx: &mut App) {
    if !AppSettings::global(cx).vim_mode {
        return;
    }

    macro_rules! kb {
        ($key:literal, $action:expr, $context:expr) => {
            KeyBinding::new($key, $action, Some($context))
        };
    }

    cx.bind_keys([
        kb!("i", VimEnterInsertMode, MODE_NORMAL),
        kb!("a", VimAppend, MODE_NORMAL),
        kb!("escape", VimNormalMode, MODE_INSERT),
        kb!("escape", VimNormalMode, MODE_VISUAL),
        kb!("escape", VimNormalMode, MODE_VISUAL_BLOCK),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Delete, None)
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Change, None)
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Yank, None)
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Around))
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Around))
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "escape",
            VimNormalMode,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Around))
        ),
        kb!("v", VimVisualMode, MODE_NORMAL),
        kb!("v", VimNormalMode, MODE_VISUAL),
        kb!("ctrl-v", VimVisualBlockMode, MODE_NORMAL),
        kb!("ctrl-v", VimNormalMode, MODE_VISUAL_BLOCK),
        kb!("I", VimBlockInsertBefore, MODE_VISUAL_BLOCK),
        kb!("A", VimBlockAppendAfter, MODE_VISUAL_BLOCK),
        kb!("d", VimDeleteOperator, MODE_NORMAL),
        kb!("d", VimDeleteOperator, MODE_VISUAL),
        kb!("d", VimDeleteOperator, MODE_VISUAL_BLOCK),
        kb!("c", VimChangeOperator, MODE_NORMAL),
        kb!("c", VimChangeOperator, MODE_VISUAL),
        kb!("c", VimChangeOperator, MODE_VISUAL_BLOCK),
        kb!("y", VimYankOperator, MODE_NORMAL),
        kb!("y", VimYankOperator, MODE_VISUAL),
        kb!("y", VimYankOperator, MODE_VISUAL_BLOCK),
        kb!("p", VimPasteAfter, MODE_NORMAL),
        kb!("P", VimPasteBefore, MODE_NORMAL),
        kb!("u", VimUndo, MODE_NORMAL),
        kb!("ctrl-r", VimRedo, MODE_NORMAL),
        kb!(".", VimRepeatLastChange, MODE_NORMAL),
        kb!("w", VimMoveWordForward, MODE_ANY_VISUAL),
        kb!("W", VimMoveBigWordForward, MODE_ANY_VISUAL),
        kb!("e", VimMoveWordEndForward, MODE_ANY_VISUAL),
        kb!("o", VimOpenNextColumn, MODE_NORMAL),
        kb!(
            "d",
            VimDeleteOperator,
            operator_context(VimOperator::Delete, None)
        ),
        kb!(
            "c",
            VimChangeOperator,
            operator_context(VimOperator::Change, None)
        ),
        kb!(
            "y",
            VimYankOperator,
            operator_context(VimOperator::Yank, None)
        ),
        kb!(
            "w",
            VimMoveWordForward,
            operator_context(VimOperator::Delete, None)
        ),
        kb!(
            "w",
            VimMoveWordForward,
            operator_context(VimOperator::Change, None)
        ),
        kb!(
            "w",
            VimMoveWordForward,
            operator_context(VimOperator::Yank, None)
        ),
        kb!(
            "W",
            VimMoveBigWordForward,
            operator_context(VimOperator::Delete, None)
        ),
        kb!(
            "W",
            VimMoveBigWordForward,
            operator_context(VimOperator::Change, None)
        ),
        kb!(
            "W",
            VimMoveBigWordForward,
            operator_context(VimOperator::Yank, None)
        ),
        kb!(
            "e",
            VimMoveWordEndForward,
            operator_context(VimOperator::Delete, None)
        ),
        kb!(
            "e",
            VimMoveWordEndForward,
            operator_context(VimOperator::Change, None)
        ),
        kb!(
            "e",
            VimMoveWordEndForward,
            operator_context(VimOperator::Yank, None)
        ),
        kb!(
            "i",
            VimTextObjectInner,
            operator_context(VimOperator::Delete, None)
        ),
        kb!(
            "a",
            VimTextObjectAround,
            operator_context(VimOperator::Delete, None)
        ),
        kb!(
            "i",
            VimTextObjectInner,
            operator_context(VimOperator::Change, None)
        ),
        kb!(
            "a",
            VimTextObjectAround,
            operator_context(VimOperator::Change, None)
        ),
        kb!(
            "i",
            VimTextObjectInner,
            operator_context(VimOperator::Yank, None)
        ),
        kb!(
            "a",
            VimTextObjectAround,
            operator_context(VimOperator::Yank, None)
        ),
        kb!(
            "w",
            VimTextObjectWord,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "w",
            VimTextObjectWord,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Around))
        ),
        kb!(
            "w",
            VimTextObjectWord,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "w",
            VimTextObjectWord,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Around))
        ),
        kb!(
            "w",
            VimTextObjectWord,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "w",
            VimTextObjectWord,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Around))
        ),
        kb!(
            "W",
            VimTextObjectBigWord,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "W",
            VimTextObjectBigWord,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Around))
        ),
        kb!(
            "W",
            VimTextObjectBigWord,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "W",
            VimTextObjectBigWord,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Around))
        ),
        kb!(
            "W",
            VimTextObjectBigWord,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "W",
            VimTextObjectBigWord,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Around))
        ),
        kb!(
            "\"",
            VimTextObjectDoubleQuote,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "\"",
            VimTextObjectDoubleQuote,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Around))
        ),
        kb!(
            "\"",
            VimTextObjectDoubleQuote,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "\"",
            VimTextObjectDoubleQuote,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Around))
        ),
        kb!(
            "\"",
            VimTextObjectDoubleQuote,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "\"",
            VimTextObjectDoubleQuote,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Around))
        ),
        kb!(
            "'",
            VimTextObjectSingleQuote,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "'",
            VimTextObjectSingleQuote,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Around))
        ),
        kb!(
            "'",
            VimTextObjectSingleQuote,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "'",
            VimTextObjectSingleQuote,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Around))
        ),
        kb!(
            "'",
            VimTextObjectSingleQuote,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "'",
            VimTextObjectSingleQuote,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Around))
        ),
        kb!(
            "(",
            VimTextObjectParen,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "(",
            VimTextObjectParen,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Around))
        ),
        kb!(
            "(",
            VimTextObjectParen,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "(",
            VimTextObjectParen,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Around))
        ),
        kb!(
            "(",
            VimTextObjectParen,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "(",
            VimTextObjectParen,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Around))
        ),
        kb!(
            "[",
            VimTextObjectBracket,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "[",
            VimTextObjectBracket,
            operator_context(VimOperator::Delete, Some(TextObjectModifier::Around))
        ),
        kb!(
            "[",
            VimTextObjectBracket,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "[",
            VimTextObjectBracket,
            operator_context(VimOperator::Change, Some(TextObjectModifier::Around))
        ),
        kb!(
            "[",
            VimTextObjectBracket,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Inner))
        ),
        kb!(
            "[",
            VimTextObjectBracket,
            operator_context(VimOperator::Yank, Some(TextObjectModifier::Around))
        ),
        kb!("h", VimMoveLeft, MODE_ANY_VISUAL),
        kb!("j", VimMoveDown, MODE_ANY_VISUAL),
        kb!("k", VimMoveUp, MODE_ANY_VISUAL),
        kb!("l", VimMoveRight, MODE_ANY_VISUAL),
        kb!("x", VimDeleteChar, MODE_ANY_VISUAL),
    ]);
}
