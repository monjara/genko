#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
    VisualBlock,
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

pub(crate) fn operator_context(
    operator: VimOperator,
    modifier: Option<TextObjectModifier>,
) -> &'static str {
    match (operator, modifier) {
        (VimOperator::Delete, None) => "vim_mode == operator_delete",
        (VimOperator::Change, None) => "vim_mode == operator_change",
        (VimOperator::Yank, None) => "vim_mode == operator_yank",
        (VimOperator::Delete, Some(TextObjectModifier::Inner)) => {
            "vim_mode == operator_delete_inner"
        }
        (VimOperator::Delete, Some(TextObjectModifier::Around)) => {
            "vim_mode == operator_delete_around"
        }
        (VimOperator::Change, Some(TextObjectModifier::Inner)) => {
            "vim_mode == operator_change_inner"
        }
        (VimOperator::Change, Some(TextObjectModifier::Around)) => {
            "vim_mode == operator_change_around"
        }
        (VimOperator::Yank, Some(TextObjectModifier::Inner)) => "vim_mode == operator_yank_inner",
        (VimOperator::Yank, Some(TextObjectModifier::Around)) => "vim_mode == operator_yank_around",
    }
}

fn operator_key_context(
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
    LineWise(String),
    BlockWise(BlockRegister),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockRegister {
    pub(crate) row_count: usize,
    pub(crate) column_count: usize,
    pub(crate) cells: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VimState {
    mode: VimMode,
    visual_anchor_cell: Option<usize>,
    pending_operator: Option<VimOperator>,
    pending_text_object_modifier: Option<TextObjectModifier>,
}

impl VimState {
    pub(crate) fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            visual_anchor_cell: None,
            pending_operator: None,
            pending_text_object_modifier: None,
        }
    }

    pub(crate) fn mode(&self) -> VimMode {
        self.mode
    }

    pub(crate) fn set_mode(&mut self, mode: VimMode) {
        self.mode = mode;
    }

    pub(crate) fn visual_anchor_cell(&self) -> Option<usize> {
        self.visual_anchor_cell
    }

    pub(crate) fn set_visual_anchor_cell(&mut self, anchor: Option<usize>) {
        self.visual_anchor_cell = anchor;
    }

    pub(crate) fn set_pending_operator(&mut self, operator: Option<VimOperator>) {
        self.pending_operator = operator;
    }

    pub(crate) fn pending_operator(&self) -> Option<VimOperator> {
        self.pending_operator
    }

    pub(crate) fn set_pending_text_object_modifier(
        &mut self,
        modifier: Option<TextObjectModifier>,
    ) {
        self.pending_text_object_modifier = modifier;
    }

    pub(crate) fn pending_text_object_modifier(&self) -> Option<TextObjectModifier> {
        self.pending_text_object_modifier
    }

    pub(crate) fn clear_pending(&mut self) {
        self.pending_operator = None;
        self.pending_text_object_modifier = None;
    }

    pub(crate) fn key_context(&self) -> &'static str {
        match (
            self.mode,
            self.pending_operator,
            self.pending_text_object_modifier,
        ) {
            (VimMode::Insert, _, _) => "Genko vim_mode=insert",
            (VimMode::Visual, _, _) => "Genko vim_mode=visual",
            (VimMode::VisualBlock, _, _) => "Genko vim_mode=visual_block",
            (VimMode::Normal, None, _) => "Genko vim_mode=normal",
            (VimMode::Normal, Some(operator), modifier) => operator_key_context(operator, modifier),
        }
    }
}
