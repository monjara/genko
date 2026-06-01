use std::{
    io,
    path::{Path, PathBuf},
};

use crate::DocumentKind;

pub const OPEN_DOCUMENT_PROMPT_LABEL: &str = "開く";
pub const SAVE_DOCUMENT_MENU_LABEL: &str = "保存";
pub const FILE_OPEN_ERROR_TITLE: &str = "ファイルを開けませんでした";
pub const FILE_SAVE_ERROR_TITLE: &str = "ファイルを保存できませんでした";
pub const FILE_PICKER_ERROR_TITLE: &str = "ファイル選択を開けませんでした";
pub const SAVE_PATH_PICKER_ERROR_TITLE: &str = "保存先を選択できませんでした";
pub const UNSUPPORTED_DOCUMENT_SAVE_ERROR_DETAIL: &str =
    "サポートしていないファイルは保存できません";
const CURRENT_DIRECTORY_FALLBACK: &str = ".";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentOpenTarget {
    Directory(PathBuf),
    SupportedDocument { path: PathBuf, kind: DocumentKind },
    UnsupportedDocument(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedDocumentTarget {
    Workspace,
    Standalone,
}

pub fn classify_open_path(path: PathBuf) -> DocumentOpenTarget {
    if path.is_dir() {
        return DocumentOpenTarget::Directory(path);
    }

    match DocumentKind::from_path(path.as_path()) {
        Some(kind) => DocumentOpenTarget::SupportedDocument { path, kind },
        None => DocumentOpenTarget::UnsupportedDocument(path),
    }
}

pub fn classify_saved_document_path(
    path: &Path,
    workspace_root: Option<&Path>,
) -> SavedDocumentTarget {
    if workspace_root.is_some_and(|workspace_root| path.starts_with(workspace_root)) {
        SavedDocumentTarget::Workspace
    } else {
        SavedDocumentTarget::Standalone
    }
}

pub fn current_directory_or_fallback() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from(CURRENT_DIRECTORY_FALLBACK))
}

pub fn suggested_save_directory(workspace_suggested_directory: Option<&Path>) -> PathBuf {
    workspace_suggested_directory
        .map(Path::to_path_buf)
        .unwrap_or_else(current_directory_or_fallback)
}

pub fn suggested_file_name(
    workspace_suggested_file_name: Option<&str>,
    document_kind: DocumentKind,
) -> String {
    workspace_suggested_file_name
        .unwrap_or(document_kind.default_file_name())
        .to_string()
}

pub fn read_document_to_string(path: PathBuf) -> io::Result<(PathBuf, String)> {
    std::fs::read_to_string(&path).map(|text| (path, text))
}

pub fn write_document_string(path: PathBuf, contents: String) -> io::Result<PathBuf> {
    std::fs::write(&path, contents.as_bytes()).map(|_| path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        DocumentOpenTarget, SavedDocumentTarget, classify_open_path, classify_saved_document_path,
        suggested_file_name, suggested_save_directory,
    };
    use crate::DocumentKind;

    #[test]
    fn classifies_directory_open_target() {
        let target = classify_open_path(PathBuf::from("."));

        assert_eq!(target, DocumentOpenTarget::Directory(PathBuf::from(".")));
    }

    #[test]
    fn classifies_supported_document_open_target() {
        let path = PathBuf::from("memo.txt");
        let target = classify_open_path(path.clone());

        assert_eq!(
            target,
            DocumentOpenTarget::SupportedDocument {
                path,
                kind: DocumentKind::PlainText
            }
        );
    }

    #[test]
    fn classifies_unsupported_document_open_target() {
        let path = PathBuf::from("memo.md");
        let target = classify_open_path(path.clone());

        assert_eq!(target, DocumentOpenTarget::UnsupportedDocument(path));
    }

    #[test]
    fn classifies_saved_document_target() {
        let workspace_root = PathBuf::from("/workspace");

        assert_eq!(
            classify_saved_document_path(
                PathBuf::from("/workspace/memo.txt").as_path(),
                Some(workspace_root.as_path()),
            ),
            SavedDocumentTarget::Workspace
        );
        assert_eq!(
            classify_saved_document_path(
                PathBuf::from("/other/memo.txt").as_path(),
                Some(workspace_root.as_path()),
            ),
            SavedDocumentTarget::Standalone
        );
        assert_eq!(
            classify_saved_document_path(PathBuf::from("/workspace/memo.txt").as_path(), None),
            SavedDocumentTarget::Standalone
        );
    }

    #[test]
    fn builds_suggested_save_directory() {
        assert_eq!(
            suggested_save_directory(Some(PathBuf::from("/workspace").as_path())),
            PathBuf::from("/workspace")
        );
    }

    #[test]
    fn builds_suggested_file_name() {
        assert_eq!(
            suggested_file_name(Some("memo.txt"), DocumentKind::PlainText),
            "memo.txt"
        );
        assert_eq!(
            suggested_file_name(None, DocumentKind::PlainText),
            "untitled.txt"
        );
    }
}
