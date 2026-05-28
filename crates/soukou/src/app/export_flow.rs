use std::path::{Path, PathBuf};

use ::richtext::EpubMetadata;
use gpui::{AppContext, Context, Entity, Focusable, Window};
use settings::{AppSettings, ExportTargetFormat, ExportWritingMode};
use ui::TextInput;

use crate::{
    app::{
        APP_NAME, CURRENT_DIRECTORY_FALLBACK, EXPORT_ERROR_TITLE, EpubMetadataForm,
        SAVE_PATH_PICKER_ERROR_TITLE, SoukouApp, non_empty_option,
    },
    document::ExportFormat,
};
use document_export as export;

impl SoukouApp {
    fn current_epub_metadata_defaults(&self, cx: &mut Context<Self>) -> EpubMetadata {
        let metadata = self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.current_epub_metadata(cx)
        });
        let fallback_title = self
            .editor_controller
            .update(cx, |editor_controller, cx| {
                editor_controller.first_heading_title(cx)
            })
            .or_else(|| {
                self.active_document
                    .path()
                    .and_then(Path::file_stem)
                    .and_then(|stem| stem.to_str())
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| APP_NAME.to_string());

        let mut metadata = metadata.unwrap_or_default();
        if metadata.title.trim().is_empty() {
            metadata.title = fallback_title;
        }
        if metadata.language.trim().is_empty() {
            metadata.language = "ja".to_string();
        }
        if metadata.identifier.trim().is_empty() {
            metadata.identifier = Self::generate_epub_identifier();
        }
        metadata
    }

    fn generate_epub_identifier() -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!(
            "urn:soukou:{}-{}",
            timestamp.as_secs(),
            timestamp.subsec_nanos()
        )
    }

    fn make_text_input(
        window: &mut Window,
        cx: &mut Context<Self>,
        placeholder: &str,
        value: &str,
    ) -> Entity<TextInput> {
        let input = cx.new(TextInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
            input.set_text(value, cx);
        });
        window.focus(&input.focus_handle(cx), cx);
        input
    }

    fn open_epub_metadata_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let metadata = self.current_epub_metadata_defaults(cx);
        let creators = metadata.creators.join(", ");
        let description = metadata.description.clone().unwrap_or_default();
        let publisher = metadata.publisher.clone().unwrap_or_default();
        let rights = metadata.rights.clone().unwrap_or_default();
        let published_at = metadata.published_at.clone().unwrap_or_default();

        let title = Self::make_text_input(window, cx, "書籍タイトル", metadata.title.as_str());
        let creators_input = Self::make_text_input(
            window,
            cx,
            "著者名（複数の場合はカンマ区切り）",
            creators.as_str(),
        );
        let language = Self::make_text_input(window, cx, "言語コード", metadata.language.as_str());
        let identifier = Self::make_text_input(window, cx, "識別子", metadata.identifier.as_str());
        let description_input = Self::make_text_input(window, cx, "説明文", description.as_str());
        let publisher_input = Self::make_text_input(window, cx, "出版者", publisher.as_str());
        let rights_input = Self::make_text_input(window, cx, "権利表記", rights.as_str());
        let published_at_input =
            Self::make_text_input(window, cx, "公開日 (YYYY-MM-DD)", published_at.as_str());

        self.epub_metadata_form = Some(EpubMetadataForm {
            title,
            creators: creators_input,
            language,
            identifier,
            description: description_input,
            publisher: publisher_input,
            rights: rights_input,
            published_at: published_at_input,
            error_message: None,
        });
        if let Some(form) = self.epub_metadata_form.as_ref() {
            window.focus(&form.title.focus_handle(cx), cx);
        }
        cx.notify();
    }

    pub(super) fn dismiss_epub_metadata_modal(
        &mut self,
        _: &gpui::MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.epub_metadata_form = None;
        cx.notify();
    }

    fn collect_epub_metadata(&self, cx: &mut Context<Self>) -> EpubMetadata {
        let form = self
            .epub_metadata_form
            .as_ref()
            .expect("EPUB form should exist");
        let title = form.title.read(cx).text().trim().to_string();
        let creators = form
            .creators
            .read(cx)
            .text()
            .split([',', '、'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let language = form.language.read(cx).text().trim().to_string();
        let identifier = form.identifier.read(cx).text().trim().to_string();
        let description = non_empty_option(form.description.read(cx).text());
        let publisher = non_empty_option(form.publisher.read(cx).text());
        let rights = non_empty_option(form.rights.read(cx).text());
        let published_at = non_empty_option(form.published_at.read(cx).text());

        EpubMetadata {
            title,
            creators,
            language,
            identifier,
            description,
            publisher,
            rights,
            published_at,
        }
    }

    pub(super) fn confirm_epub_metadata_modal(
        &mut self,
        _: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let metadata = self.collect_epub_metadata(cx);
        let error_message = if metadata.title.is_empty() {
            Some("タイトルを入力してください。".to_string())
        } else if metadata.creators.is_empty() {
            Some("著者名を 1 つ以上入力してください。".to_string())
        } else if metadata.language.is_empty() {
            Some("言語コードを入力してください。".to_string())
        } else if metadata.identifier.is_empty() {
            Some("識別子を入力してください。".to_string())
        } else {
            None
        };

        if let Some(error_message) = error_message {
            if let Some(form) = self.epub_metadata_form.as_mut() {
                form.error_message = Some(error_message);
            }
            cx.notify();
            return;
        }

        if self.current_document_is_richtext(cx) {
            let _ = self.editor_controller.update(cx, |editor_controller, cx| {
                editor_controller.set_epub_metadata(metadata.clone(), cx);
            });
        }

        self.epub_metadata_form = None;
        cx.notify();
        self.start_export_document(ExportFormat::Epub, Some(metadata), window, cx);
    }

    fn start_export_document(
        &mut self,
        format: ExportFormat,
        epub_metadata: Option<EpubMetadata>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base_name = self
            .active_document
            .path()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .unwrap_or("untitled");
        let suggested_name = format!(
            "{}.{}",
            base_name,
            match format {
                ExportFormat::Word => export::ExportFormat::Word.file_extension(),
                ExportFormat::Epub => export::ExportFormat::Epub.file_extension(),
            }
        );
        let initial_directory = self
            .active_document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK));
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));
        let _window_handle = window.window_handle();
        let Some(rich_document) = self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.richtext_document(cx)
        }) else {
            self.show_error_modal(
                EXPORT_ERROR_TITLE,
                "リッチテキスト文書をエクスポート形式へ変換できませんでした".to_string(),
                cx,
            );
            return;
        };
        let export_format = match format {
            ExportFormat::Word => export::ExportFormat::Word,
            ExportFormat::Epub => export::ExportFormat::Epub,
        };
        let export_target = match format {
            ExportFormat::Word => ExportTargetFormat::Word,
            ExportFormat::Epub => ExportTargetFormat::Epub,
        };
        let export_options = export::ExportOptions {
            writing_mode: match AppSettings::global(cx)
                .export_settings
                .format(export_target)
                .writing_mode
            {
                ExportWritingMode::Vertical => export::ExportWritingMode::Vertical,
                ExportWritingMode::Horizontal => export::ExportWritingMode::Horizontal,
            },
            epub_metadata,
        };
        let this = cx.entity().downgrade();

        cx.spawn(async move |_, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    let Some(this_entity) = this.upgrade() else {
                        return;
                    };
                    let _ = this_entity.update(cx, |this, cx| {
                        this.show_error_modal(SAVE_PATH_PICKER_ERROR_TITLE, error.to_string(), cx);
                    });
                    None
                }
            }) else {
                return;
            };

            let write_result = cx
                .background_spawn(async move {
                    export::write_export(
                        path.as_path(),
                        export_format,
                        &rich_document,
                        export_options,
                    )
                })
                .await;

            if let Err(error) = write_result {
                let Some(this_entity) = this.upgrade() else {
                    return;
                };
                let _ = this_entity.update(cx, |this, cx| {
                    this.show_error_modal(EXPORT_ERROR_TITLE, error.to_string(), cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn export_document(
        &mut self,
        format: ExportFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match format {
            ExportFormat::Word => self.start_export_document(format, None, window, cx),
            ExportFormat::Epub => self.open_epub_metadata_modal(window, cx),
        }
    }

    pub(super) fn export_txt_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let base_name = self
            .active_document
            .path()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .unwrap_or("untitled");
        let suggested_name = format!("{base_name}.txt");
        let initial_directory = self
            .active_document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(CURRENT_DIRECTORY_FALLBACK));
        let receiver = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));
        let _window_handle = window.window_handle();
        let contents = self.editor_controller.read(cx).snapshot_text(cx);
        let this = cx.entity().downgrade();

        cx.spawn(async move |_, cx| {
            let Ok(result) = receiver.await else {
                return;
            };

            let Some(path) = (match result {
                Ok(path) => path,
                Err(error) => {
                    let Some(this_entity) = this.upgrade() else {
                        return;
                    };
                    let _ = this_entity.update(cx, |this, cx| {
                        this.show_error_modal(SAVE_PATH_PICKER_ERROR_TITLE, error.to_string(), cx);
                    });
                    None
                }
            }) else {
                return;
            };

            let write_result = cx
                .background_spawn(async move { std::fs::write(path.as_path(), contents) })
                .await;

            if let Err(error) = write_result {
                let Some(this_entity) = this.upgrade() else {
                    return;
                };
                let _ = this_entity.update(cx, |this, cx| {
                    this.show_error_modal(EXPORT_ERROR_TITLE, error.to_string(), cx);
                });
            }
        })
        .detach();
    }
}
