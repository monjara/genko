use std::{
    error::Error as StdError,
    fmt, io,
    path::{Path, PathBuf},
};

pub mod document_io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    PlainText,
}

#[derive(Debug)]
pub enum DocumentError {
    OpenFailed { path: PathBuf, source: io::Error },
    SaveFailed { path: PathBuf, source: io::Error },
    MetadataOpenFailed { path: PathBuf, source: io::Error },
    MetadataSaveFailed { path: PathBuf, source: io::Error },
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenFailed { path, source } => {
                write!(formatter, "{} を開けませんでした: {source}", path.display())
            }
            Self::SaveFailed { path, source } => {
                write!(
                    formatter,
                    "{} を保存できませんでした: {source}",
                    path.display()
                )
            }
            Self::MetadataOpenFailed { path, source } => {
                write!(
                    formatter,
                    "{} のメタデータを読めませんでした: {source}",
                    path.display()
                )
            }
            Self::MetadataSaveFailed { path, source } => {
                write!(
                    formatter,
                    "{} のメタデータを保存できませんでした: {source}",
                    path.display()
                )
            }
        }
    }
}

impl StdError for DocumentError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::OpenFailed { source, .. }
            | Self::SaveFailed { source, .. }
            | Self::MetadataOpenFailed { source, .. }
            | Self::MetadataSaveFailed { source, .. } => Some(source),
        }
    }
}

impl DocumentKind {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("txt") {
            return Some(Self::PlainText);
        }

        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveDocument {
    path: Option<PathBuf>,
    kind: DocumentKind,
}

impl Default for ActiveDocument {
    fn default() -> Self {
        Self {
            path: None,
            kind: DocumentKind::PlainText,
        }
    }
}

impl ActiveDocument {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.kind = DocumentKind::from_path(path.as_path()).unwrap_or(DocumentKind::PlainText);
        self.path = Some(path);
    }
}
