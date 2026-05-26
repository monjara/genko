use std::path::{Path, PathBuf};

use gpui::{AppContext, Context, ExternalPaths, PathPromptOptions, Window};
use ::richtext::RichDocument;

use crate::{
    app::{
        CURRENT_DIRECTORY_FALLBACK, FILE_OPEN_ERROR_TITLE, FILE_PICKER_ERROR_TITLE,
        FILE_SAVE_ERROR_TITLE, OPEN_PROMPT_LABEL, SAVE_PATH_PICKER_ERROR_TITLE, SoukouApp,
    },
    document::DocumentKind,
};

impl SoukouApp {
    fn is_supported_document_path(path: &Path) -> bool {
        DocumentKind::from_path(path).is_some()
    }

    fn load_plain_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_text(text, cx)
        });
        self.rich_document = None;
        self.active_document.set_path(path);
        self.sync_editor_richtext_projection(cx);
    }

    fn load_rich_document(
        &mut self,
        path: PathBuf,
        document: RichDocument,
        cx: &mut Context<Self>,
    ) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_text(document.plain_text(), cx)
        });
        self.rich_document = Some(document);
        self.active_document.set_path(path);
        self.sync_editor_richtext_projection(cx);
    }

    fn open_standalone_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        self.load_plain_document(path, text, cx);
    }

    fn open_standalone_rich_document(
        &mut self,
        path: PathBuf,
        document: RichDocument,
        cx: &mut Context<Self>,
    ) {
        self.load_rich_document(path, document, cx);
    }

    fn save_document_to_path(
        &mut self,
        path: PathBuf,
        contents: String,
        _window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    std::fs::write(&path, contents.as_bytes()).map(|_| path)
                })
                .await;

            match result {
                Ok(path) => {
                    let _ = this.update(cx, |this, cx| {
                        this.active_document.set_path(path);
                        cx.notify();
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.show_error_modal(FILE_SAVE_ERROR_TITLE, error.to_string(), cx);
                    });
                }
            }
        })
        .detach();
    }

    fn open_document_path(
        &mut self,
        path: PathBuf,
        preserve_workspace: bool,
        _window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let Some(document_kind) = DocumentKind::from_path(path.as_path()) else {
            self.show_error_modal(
                FILE_OPEN_ERROR_TITLE,
                DocumentKind::supported_open_error_detail().into(),
                cx,
            );
            return;
        };

        cx.spawn(async move |this, cx| {
            let path_for_read = path.clone();
            let result = cx
                .background_spawn(async move {
                    std::fs::read_to_string(&path_for_read).map(|text| (path_for_read, text))
                })
                .await;

            match result {
                Ok((path, text)) => {
                    let _ = this.update(cx, |this, cx| match document_kind {
                        DocumentKind::PlainText => {
                            if preserve_workspace {
                                this.load_plain_document(path, &text, cx);
                            } else {
                                this.open_standalone_plain_document(path, &text, cx);
                            }
                        }
                        DocumentKind::RichText => match RichDocument::from_json(text.as_str()) {
                            Ok(document) => {
                                if preserve_workspace {
                                    this.load_rich_document(path, document, cx);
                                } else {
                                    this.open_standalone_rich_document(path, document, cx);
                                }
                            }
                            Err(error) => {
                                let detail =
                                    format!("リッチテキスト文書を解析できませんでした: {error}");
                                this.show_error_modal(FILE_OPEN_ERROR_TITLE, detail, cx);
                            }
                        },
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.show_error_modal(FILE_OPEN_ERROR_TITLE, error.to_string(), cx);
                    });
                }
            }
        })
        .detach();
    }

    pub(super) fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(OPEN_PROMPT_LABEL.into()),
        });
        let window_handle = window.window_handle();

        cx.spawn(async move |this, cx| {
            let Ok(result) = picker.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(Some(mut paths)) => paths.pop(),
                Ok(None) => None,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.show_error_modal(FILE_PICKER_ERROR_TITLE, error.to_string(), cx);
                    });
                    None
                }
            }) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.open_document_path(path, false, window_handle, cx);
            });
        })
        .detach();
    }

    pub(super) fn open_dropped_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = paths
            .paths()
            .iter()
            .find(|path| Self::is_supported_document_path(path.as_path()))
            .cloned()
        else {
            let _ = window;
            self.show_error_modal(
                FILE_OPEN_ERROR_TITLE,
                DocumentKind::supported_open_error_detail().into(),
                cx,
            );
            return;
        };

        self.open_document_path(path, true, window.window_handle(), cx);
    }

    pub(super) fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_richtext_from_editor(cx);
        let window_handle = window.window_handle();
        let contents = match self.active_document.kind() {
            DocumentKind::PlainText => self.editor_controller.read(cx).snapshot_text(cx),
            DocumentKind::RichText => match self
                .rich_document
                .as_ref()
                .and_then(|document| document.to_json().ok())
            {
                Some(json) => json,
                None => {
                    let _ = window;
                    self.show_error_modal(
                        FILE_SAVE_ERROR_TITLE,
                        "リッチテキスト文書を保存形式へ変換できませんでした".to_string(),
                        cx,
                    );
                    return;
                }
            },
        };

        if let Some(path) = self.active_document.path().map(Path::to_path_buf) {
            self.save_document_to_path(path, contents, window_handle, cx);
            return;
        }

        let initial_directory = self
            .active_document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK));
        let suggested_name = self
            .active_document
            .path()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or(self.active_document.kind().default_file_name())
            .to_string();
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));

        cx.spawn(async move |this, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.show_error_modal(SAVE_PATH_PICKER_ERROR_TITLE, error.to_string(), cx);
                    });
                    None
                }
            }) else {
                return;
            };

            let _ = this.update(cx, |this, cx| {
                this.save_document_to_path(path, contents, window_handle, cx);
            });
        })
        .detach();
    }
}
