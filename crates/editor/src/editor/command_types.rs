#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MotionKind {
    WordForward,
    BigWordForward,
    WordEndForward,
    WordBackward,
    BigWordBackward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextObjectModifier {
    Inner,
    Around,
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
pub(crate) enum PastePosition {
    Before,
    After,
}
