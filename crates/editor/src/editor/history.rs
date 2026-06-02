use gpui::Context;

use super::{AppliedEditBatch, EditOperation, EditTransaction, Editor, PendingTransaction};

pub(super) fn begin_transaction(editor: &mut Editor) {
    if editor.history.active_transaction.is_none() {
        editor.history.active_transaction = Some(PendingTransaction {
            before: editor.view_state(),
            edits: Vec::new(),
        });
    }
}

pub(super) fn commit_transaction(editor: &mut Editor, cx: &mut Context<Editor>) -> bool {
    let Some(pending) = editor.history.active_transaction.take() else {
        return false;
    };
    let after = editor.view_state();
    if pending.edits.is_empty() && pending.before == after {
        return false;
    }

    editor.history.undo_stack.push(EditTransaction {
        before: pending.before,
        after,
        edits: pending.edits.clone(),
    });
    editor.last_applied_edit_batch = Some(AppliedEditBatch {
        revision: editor.draft_revision,
        edits: pending.edits,
    });
    editor.sync_rich_text_meta_after_text_change();
    editor.history.redo_stack.clear();
    cx.notify();
    true
}

pub(super) fn active_transaction_inserted_text(editor: &Editor) -> Option<String> {
    editor
        .history
        .active_transaction
        .as_ref()
        .map(|transaction| transaction_inserted_text(&transaction.edits))
}

pub(super) fn last_transaction_inserted_text(editor: &Editor) -> Option<String> {
    editor
        .history
        .undo_stack
        .last()
        .map(|transaction| transaction_inserted_text(&transaction.edits))
}

pub(super) fn undo(editor: &mut Editor, cx: &mut Context<Editor>) -> bool {
    let Some(transaction) = editor.history.undo_stack.pop() else {
        return false;
    };
    for edit in transaction.edits.iter().rev() {
        let inserted_end = edit.start + edit.inserted_text.len();
        editor.draft.replace_range(
            inserted_end.saturating_sub(edit.inserted_text.len())..inserted_end,
            &edit.removed_text,
        );
    }
    editor.bump_draft_revision();
    editor.restore_view_state(transaction.before.clone());
    editor.last_applied_edit_batch = Some(AppliedEditBatch {
        revision: editor.draft_revision,
        edits: inverse_edit_operations(&transaction.edits),
    });
    editor.sync_rich_text_meta_after_text_change();
    editor.history.redo_stack.push(transaction);
    cx.notify();
    true
}

pub(super) fn redo(editor: &mut Editor, cx: &mut Context<Editor>) -> bool {
    let Some(transaction) = editor.history.redo_stack.pop() else {
        return false;
    };
    for edit in &transaction.edits {
        let removed_end = edit.start + edit.removed_text.len();
        editor.draft.replace_range(
            removed_end.saturating_sub(edit.removed_text.len())..removed_end,
            &edit.inserted_text,
        );
    }
    editor.bump_draft_revision();
    editor.restore_view_state(transaction.after.clone());
    editor.last_applied_edit_batch = Some(AppliedEditBatch {
        revision: editor.draft_revision,
        edits: transaction.edits.clone(),
    });
    editor.sync_rich_text_meta_after_text_change();
    editor.history.undo_stack.push(transaction);
    cx.notify();
    true
}

pub(super) fn transaction_inserted_text(edits: &[EditOperation]) -> String {
    let mut inserted_edits = edits.iter().filter(|edit| !edit.inserted_text.is_empty());
    let Some(first_edit) = inserted_edits.next() else {
        return String::new();
    };

    let mut region_start = first_edit.start;
    let mut region = first_edit.removed_text.clone();
    replace_region_text(
        &mut region,
        0..first_edit.removed_text.len(),
        &first_edit.inserted_text,
    );

    for edit in inserted_edits {
        let edit_start = edit.start;
        let edit_end = edit.start + edit.removed_text.len();
        if edit_start < region_start {
            let prefix_len = region_start - edit_start;
            let prefix = &edit.removed_text[..prefix_len.min(edit.removed_text.len())];
            region.insert_str(0, prefix);
            region_start = edit_start;
        }

        let current_region_end = region_start + region.len();
        if edit_start > current_region_end {
            break;
        }

        if edit_end > current_region_end {
            let suffix_start = edit
                .removed_text
                .len()
                .saturating_sub(edit_end - current_region_end);
            region.push_str(&edit.removed_text[suffix_start..]);
        }

        let local_start = edit_start.saturating_sub(region_start);
        let local_end = local_start + edit.removed_text.len();
        replace_region_text(&mut region, local_start..local_end, &edit.inserted_text);
    }

    region
}

fn replace_region_text(region: &mut String, range: std::ops::Range<usize>, replacement: &str) {
    region.replace_range(range, replacement);
}

fn inverse_edit_operations(edits: &[EditOperation]) -> Vec<EditOperation> {
    edits
        .iter()
        .rev()
        .map(|edit| EditOperation {
            start: edit.start,
            removed_text: edit.inserted_text.clone(),
            inserted_text: edit.removed_text.clone(),
            affects_rich_text: edit.affects_rich_text,
        })
        .collect()
}
