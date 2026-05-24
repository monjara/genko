use std::path::{Path, PathBuf};

use richtext::FILE_EXTENSION as RICHTEXT_FILE_EXTENSION;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    PlainText,
    RichText,
}

impl DocumentKind {
    pub fn default_file_name(self) -> &'static str {
        match self {
            Self::PlainText => "untitled.txt",
            Self::RichText => "untitled.soukou",
        }
    }

    pub fn supported_open_error_detail() -> &'static str {
        "現在は .txt と .soukou ファイルに対応しています"
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("txt") {
            Some(Self::PlainText)
        } else if extension.eq_ignore_ascii_case(RICHTEXT_FILE_EXTENSION) {
            Some(Self::RichText)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Word,
    Epub,
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

    pub fn kind(&self) -> DocumentKind {
        self.kind
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.kind = DocumentKind::from_path(path.as_path()).unwrap_or(DocumentKind::PlainText);
        self.path = Some(path);
    }

    pub fn set_kind(&mut self, kind: DocumentKind) {
        self.kind = kind;
        if let Some(path) = &self.path
            && DocumentKind::from_path(path.as_path()) != Some(kind)
        {
            self.path = None;
        }
    }
}
