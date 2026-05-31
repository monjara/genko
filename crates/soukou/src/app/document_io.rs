use std::path::{Path, PathBuf};

use gpui::{AppContext, Context, ExternalPaths, PathPromptOptions, Window};
use workspace::{WorkspaceState, scan_workspace_entries};

use crate::{
    app::{
        CURRENT_DIRECTORY_FALLBACK, FILE_OPEN_ERROR_TITLE, FILE_PICKER_ERROR_TITLE,
        FILE_SAVE_ERROR_TITLE, OPEN_PROMPT_LABEL, SAVE_PATH_PICKER_ERROR_TITLE, SoukouApp,
    },
    document::DocumentKind,
};

impl SoukouApp {
    fn load_plain_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.load_plain_text(text, cx)
        });
        self.active_document.set_path(path);
    }

    fn notify_workspace(&self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |_, cx| cx.notify());
    }

    fn open_workspace_plain_document(&mut self, path: PathBuf, text: &str, cx: &mut Context<Self>) {
        WorkspaceState::global_mut(cx).open_file(path.clone());
        self.load_plain_document(path, text, cx);
        self.notify_workspace(cx);
    }

    fn open_standalone_plain_document(
        &mut self,
        path: PathBuf,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        WorkspaceState::global_mut(cx).open_file_without_root(path.clone());
        self.load_plain_document(path, text, cx);
        self.notify_workspace(cx);
    }

    fn open_unsupported_document(
        &mut self,
        path: PathBuf,
        preserve_workspace: bool,
        cx: &mut Context<Self>,
    ) {
        if preserve_workspace {
            WorkspaceState::global_mut(cx).open_unsupported_file(path);
        } else {
            WorkspaceState::global_mut(cx).open_unsupported_file_without_root(path);
        }
        self.notify_workspace(cx);
        cx.notify();
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
                        let in_workspace = WorkspaceState::global(cx)
                            .root_dir()
                            .is_some_and(|root_dir| path.starts_with(root_dir));
                        if in_workspace {
                            WorkspaceState::global_mut(cx).open_file(path.clone());
                        } else {
                            WorkspaceState::global_mut(cx).open_file_without_root(path.clone());
                        }
                        this.active_document.set_path(path);
                        this.notify_workspace(cx);
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
        cx: &mut Context<Self>,
    ) {
        let Some(document_kind) = DocumentKind::from_path(path.as_path()) else {
            self.open_unsupported_document(path, preserve_workspace, cx);
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
                                this.open_workspace_plain_document(path, &text, cx);
                            } else {
                                this.open_standalone_plain_document(path, &text, cx);
                            }
                        }
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

    fn open_directory_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let path_for_scan = path.clone();
            let result = cx
                .background_spawn(async move {
                    scan_workspace_entries(path_for_scan.as_path())
                        .map(|entries| (path_for_scan, entries))
                })
                .await;

            match result {
                Ok((path, entries)) => {
                    let _ = this.update(cx, |this, cx| {
                        WorkspaceState::global_mut(cx).open_root(path, entries);
                        this.notify_workspace(cx);
                        cx.notify();
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

    pub(super) fn open_workspace_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if path.is_dir() {
            self.open_directory_path(path, cx);
        } else {
            self.open_document_path(path, true, cx);
        }
    }

    pub(super) fn open_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some(OPEN_PROMPT_LABEL.into()),
        });

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
                if path.is_dir() {
                    this.open_directory_path(path, cx);
                } else {
                    this.open_document_path(path, false, cx);
                }
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
        let Some(path) = paths.paths().first().cloned() else {
            let _ = window;
            self.show_error_modal(
                FILE_OPEN_ERROR_TITLE,
                DocumentKind::supported_open_error_detail().into(),
                cx,
            );
            return;
        };

        if path.is_dir() {
            self.open_directory_path(path, cx);
        } else {
            self.open_document_path(path, true, cx);
        }
    }

    pub(super) fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if WorkspaceState::global(cx).unsupported_file().is_some() {
            self.show_error_modal(
                FILE_SAVE_ERROR_TITLE,
                "サポートしていないファイルは保存できません".to_string(),
                cx,
            );
            return;
        }

        let window_handle = window.window_handle();
        let contents = match self.current_document_kind(cx) {
            DocumentKind::PlainText => self.editor_controller.read(cx).snapshot_text(cx),
        };

        if let Some(path) = self.active_document.path().map(Path::to_path_buf) {
            self.save_document_to_path(path, contents, window_handle, cx);
            return;
        }

        let initial_directory = WorkspaceState::global(cx)
            .suggested_save_directory()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK));
        let suggested_name = WorkspaceState::global(cx)
            .suggested_file_name()
            .unwrap_or(self.current_document_kind(cx).default_file_name())
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
