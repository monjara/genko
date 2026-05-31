use std::path::{Path, PathBuf};

use gpui::{AppContext, Context, Window};

use crate::app::{
    CURRENT_DIRECTORY_FALLBACK, EXPORT_ERROR_TITLE, SAVE_PATH_PICKER_ERROR_TITLE, SoukouApp,
};

impl SoukouApp {
    pub(super) fn export_txt_document(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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
