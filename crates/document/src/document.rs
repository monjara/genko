use std::path::{Path, PathBuf};

pub mod document_io;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    PlainText,
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
