use gpui::{App, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
    VisualBlock,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VimOperator {
    Delete,
    Change,
    Yank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextObjectModifier {
    Inner,
    Around,
}

pub(crate) fn operator_key_context(
    operator: VimOperator,
    modifier: Option<TextObjectModifier>,
) -> &'static str {
    match (operator, modifier) {
        (VimOperator::Delete, None) => "Genko vim_mode=operator_delete",
        (VimOperator::Change, None) => "Genko vim_mode=operator_change",
        (VimOperator::Yank, None) => "Genko vim_mode=operator_yank",
        (VimOperator::Delete, Some(TextObjectModifier::Inner)) => {
            "Genko vim_mode=operator_delete_inner"
        }
        (VimOperator::Delete, Some(TextObjectModifier::Around)) => {
            "Genko vim_mode=operator_delete_around"
        }
        (VimOperator::Change, Some(TextObjectModifier::Inner)) => {
            "Genko vim_mode=operator_change_inner"
        }
        (VimOperator::Change, Some(TextObjectModifier::Around)) => {
            "Genko vim_mode=operator_change_around"
        }
        (VimOperator::Yank, Some(TextObjectModifier::Inner)) => {
            "Genko vim_mode=operator_yank_inner"
        }
        (VimOperator::Yank, Some(TextObjectModifier::Around)) => {
            "Genko vim_mode=operator_yank_around"
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextObjectTarget {
    Word,
    BigWord,
    DoubleQuote,
    SingleQuote,
    Paren,
    Bracket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MotionKind {
    WordForward,
    BigWordForward,
    WordEndForward,
    WordBackward,
    BigWordBackward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockInsertKind {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertKind {
    Insert,
    Append,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RepeatTarget {
    Motion(MotionKind),
    TextObject(TextObjectModifier, TextObjectTarget),
    Line,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RepeatableCommand {
    DeleteChar,
    Delete(RepeatTarget),
    BlockDelete {
        row_count: usize,
        column_count: usize,
    },
    Change {
        target: RepeatTarget,
        inserted_text: String,
    },
    BlockChange {
        row_count: usize,
        column_count: usize,
        inserted_text: String,
    },
    PasteAfter,
    PasteBefore,
    Insert {
        kind: InsertKind,
        inserted_text: String,
    },
    BlockInsert {
        kind: BlockInsertKind,
        row_count: usize,
        inserted_text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingInsert {
    pub(crate) kind: InsertKind,
    pub(crate) change_target: Option<RepeatTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingBlockInsert {
    pub(crate) kind: BlockInsertKind,
    pub(crate) row_count: usize,
    pub(crate) column_count: usize,
    pub(crate) delete_selection: bool,
    pub(crate) target_cells: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum YankRegister {
    Empty,
    CharWise(String),
    LineWise {
        content: String,
        leading_rows: usize,
    },
    BlockWise(BlockRegister),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockRegister {
    pub(crate) row_count: usize,
    pub(crate) column_count: usize,
    pub(crate) cells: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VimState {
    pub mode: VimMode,
    pub command_line: Option<String>,
}

pub(crate) fn init(cx: &mut App) {
    cx.set_global::<VimState>(VimState::new());
}

impl VimState {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            command_line: None,
        }
    }
}

impl Global for VimState {}
