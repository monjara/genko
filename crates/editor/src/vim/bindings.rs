use gpui::{App, KeyBinding};
use settings::AppSettings;

use super::{
    VimAppend, VimBlockAppendAfter, VimBlockInsertBefore, VimChangeOperator, VimDeleteChar,
    VimDeleteOperator, VimEnterInsertMode, VimMoveBigWordForward, VimMoveDown, VimMoveLeft,
    VimMoveRight, VimMoveUp, VimMoveWordEndForward, VimMoveWordForward, VimNormalMode,
    VimOpenNextColumn, VimPasteAfter, VimPasteBefore, VimRedo, VimRepeatLastChange,
    VimTextObjectAround, VimTextObjectBigWord, VimTextObjectBracket, VimTextObjectDoubleQuote,
    VimTextObjectInner, VimTextObjectParen, VimTextObjectSingleQuote, VimTextObjectWord, VimUndo,
    VimVisualBlockMode, VimVisualMode, VimYankOperator,
};

pub(crate) fn init(cx: &mut App) {
    if !AppSettings::global(cx).vim_mode {
        return;
    }

    cx.bind_keys([
        KeyBinding::new("i", VimEnterInsertMode, Some("vim_mode == normal")),
        KeyBinding::new("a", VimAppend, Some("vim_mode == normal")),
        KeyBinding::new(
            "escape",
            VimNormalMode,
            Some(
                "vim_mode == insert || vim_mode == visual || vim_mode == visual_block || vim_mode == operator_delete || vim_mode == operator_change || vim_mode == operator_yank || vim_mode == operator_delete_inner || vim_mode == operator_delete_around || vim_mode == operator_change_inner || vim_mode == operator_change_around || vim_mode == operator_yank_inner || vim_mode == operator_yank_around",
            ),
        ),
        KeyBinding::new("v", VimVisualMode, Some("vim_mode == normal")),
        KeyBinding::new("v", VimNormalMode, Some("vim_mode == visual")),
        KeyBinding::new("ctrl-v", VimVisualBlockMode, Some("vim_mode == normal")),
        KeyBinding::new("ctrl-v", VimNormalMode, Some("vim_mode == visual_block")),
        KeyBinding::new("I", VimBlockInsertBefore, Some("vim_mode == visual_block")),
        KeyBinding::new("A", VimBlockAppendAfter, Some("vim_mode == visual_block")),
        KeyBinding::new(
            "d",
            VimDeleteOperator,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "c",
            VimChangeOperator,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "y",
            VimYankOperator,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new("p", VimPasteAfter, Some("vim_mode == normal")),
        KeyBinding::new("P", VimPasteBefore, Some("vim_mode == normal")),
        KeyBinding::new("u", VimUndo, Some("vim_mode == normal")),
        KeyBinding::new("ctrl-r", VimRedo, Some("vim_mode == normal")),
        KeyBinding::new(".", VimRepeatLastChange, Some("vim_mode == normal")),
        KeyBinding::new(
            "w",
            VimMoveWordForward,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "W",
            VimMoveBigWordForward,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "e",
            VimMoveWordEndForward,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new("o", VimOpenNextColumn, Some("vim_mode == normal")),
        KeyBinding::new("d", VimDeleteOperator, Some("vim_mode == operator_delete")),
        KeyBinding::new("c", VimChangeOperator, Some("vim_mode == operator_change")),
        KeyBinding::new("y", VimYankOperator, Some("vim_mode == operator_yank")),
        KeyBinding::new(
            "w",
            VimMoveWordForward,
            Some("vim_mode == operator_delete || vim_mode == operator_change || vim_mode == operator_yank"),
        ),
        KeyBinding::new(
            "W",
            VimMoveBigWordForward,
            Some("vim_mode == operator_delete || vim_mode == operator_change || vim_mode == operator_yank"),
        ),
        KeyBinding::new(
            "e",
            VimMoveWordEndForward,
            Some("vim_mode == operator_delete || vim_mode == operator_change || vim_mode == operator_yank"),
        ),
        KeyBinding::new(
            "i",
            VimTextObjectInner,
            Some("vim_mode == operator_delete || vim_mode == operator_change || vim_mode == operator_yank"),
        ),
        KeyBinding::new(
            "a",
            VimTextObjectAround,
            Some("vim_mode == operator_delete || vim_mode == operator_change || vim_mode == operator_yank"),
        ),
        KeyBinding::new(
            "w",
            VimTextObjectWord,
            Some("vim_mode == operator_delete_inner || vim_mode == operator_delete_around || vim_mode == operator_change_inner || vim_mode == operator_change_around || vim_mode == operator_yank_inner || vim_mode == operator_yank_around"),
        ),
        KeyBinding::new(
            "W",
            VimTextObjectBigWord,
            Some("vim_mode == operator_delete_inner || vim_mode == operator_delete_around || vim_mode == operator_change_inner || vim_mode == operator_change_around || vim_mode == operator_yank_inner || vim_mode == operator_yank_around"),
        ),
        KeyBinding::new(
            "\"",
            VimTextObjectDoubleQuote,
            Some("vim_mode == operator_delete_inner || vim_mode == operator_delete_around || vim_mode == operator_change_inner || vim_mode == operator_change_around || vim_mode == operator_yank_inner || vim_mode == operator_yank_around"),
        ),
        KeyBinding::new(
            "'",
            VimTextObjectSingleQuote,
            Some("vim_mode == operator_delete_inner || vim_mode == operator_delete_around || vim_mode == operator_change_inner || vim_mode == operator_change_around || vim_mode == operator_yank_inner || vim_mode == operator_yank_around"),
        ),
        KeyBinding::new(
            "(",
            VimTextObjectParen,
            Some("vim_mode == operator_delete_inner || vim_mode == operator_delete_around || vim_mode == operator_change_inner || vim_mode == operator_change_around || vim_mode == operator_yank_inner || vim_mode == operator_yank_around"),
        ),
        KeyBinding::new(
            "[",
            VimTextObjectBracket,
            Some("vim_mode == operator_delete_inner || vim_mode == operator_delete_around || vim_mode == operator_change_inner || vim_mode == operator_change_around || vim_mode == operator_yank_inner || vim_mode == operator_yank_around"),
        ),
        KeyBinding::new(
            "h",
            VimMoveLeft,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "j",
            VimMoveDown,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "k",
            VimMoveUp,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "l",
            VimMoveRight,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
        KeyBinding::new(
            "x",
            VimDeleteChar,
            Some("vim_mode == normal || vim_mode == visual || vim_mode == visual_block"),
        ),
    ]);
}
