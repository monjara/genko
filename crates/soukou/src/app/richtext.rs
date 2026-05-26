use ::richtext::{BlockKind, InlineStyle, RichDocument, single_change};
use gpui::{Context, Window};

use crate::{
    app::{FeatureGate, SoukouApp},
    document::DocumentKind,
};

impl SoukouApp {
    pub(super) fn sync_editor_richtext_projection(&mut self, cx: &mut Context<Self>) {
        let rich_document = self.rich_document.clone();
        self.editor_controller.update(cx, |editor_controller, cx| {
            editor_controller.set_richtext_document(rich_document.as_ref(), cx);
        });
        self.last_richtext_revision = self.editor_controller.read(cx).draft_revision(cx);
    }

    pub(super) fn sync_richtext_from_editor(&mut self, cx: &mut Context<Self>) {
        if self.active_document.kind() != DocumentKind::RichText {
            return;
        }

        let revision = self.editor_controller.read(cx).draft_revision(cx);
        if revision == self.last_richtext_revision {
            return;
        }

        if let Some(document) = self.rich_document.as_mut() {
            let applied_edit_batch = self.editor_controller.read(cx).last_applied_edit_batch(cx);
            if let Some(batch) = applied_edit_batch
                && batch.revision() == revision
            {
                for edit in batch.edits() {
                    let range = edit.start()..edit.start() + edit.removed_text().len();
                    document.replace_text(range, edit.inserted_text());
                }
            } else {
                let text = self.editor_controller.read(cx).snapshot_text(cx);
                if let Some((range, replacement)) = single_change(document.plain_text(), &text) {
                    document.replace_text(range, replacement.as_str());
                } else if document.plain_text() != text {
                    let epub_metadata = document.epub_metadata.clone();
                    *document = RichDocument::new(text);
                    document.epub_metadata = epub_metadata;
                }
            }
        }

        self.sync_editor_richtext_projection(cx);
    }

    fn ensure_richtext_document(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_feature_available(FeatureGate::RichText) {
            return false;
        }

        self.sync_richtext_from_editor(cx);
        if self.active_document.kind() == DocumentKind::RichText && self.rich_document.is_some() {
            return true;
        }

        let text = self.editor_controller.read(cx).snapshot_text(cx);
        self.active_document.set_kind(DocumentKind::RichText);
        self.rich_document = Some(RichDocument::new(text));
        self.sync_editor_richtext_projection(cx);
        true
    }

    pub(super) fn apply_inline_style(
        &mut self,
        style: InlineStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_feature_available(FeatureGate::RichText) {
            self.prompt_pro_required(FeatureGate::RichText, window, cx);
            return;
        }
        if !self.ensure_richtext_document(cx) {
            return;
        }

        let selected_range = self.editor_controller.read(cx).selected_byte_range(cx);
        if selected_range.is_empty() {
            return;
        }

        if let Some(document) = self.rich_document.as_mut() {
            document.toggle_inline_style(selected_range, style);
        }
        self.sync_editor_richtext_projection(cx);
    }

    pub(super) fn apply_block_kind(
        &mut self,
        kind: BlockKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_feature_available(FeatureGate::RichText) {
            self.prompt_pro_required(FeatureGate::RichText, window, cx);
            return;
        }
        if !self.ensure_richtext_document(cx) {
            return;
        }

        let selected_range = self.editor_controller.read(cx).selected_byte_range(cx);
        if let Some(document) = self.rich_document.as_mut() {
            document.set_block_kind_for_range(selected_range, kind);
        }
        self.sync_editor_richtext_projection(cx);
    }
}
