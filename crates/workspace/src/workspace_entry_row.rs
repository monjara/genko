use std::path::PathBuf;

use gpui::{
    App, Entity, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use theme::Theme;

use crate::{Event, Workspace, WorkspaceEntry, WorkspaceState};

#[derive(IntoElement)]
pub(super) struct WorkspaceEntryRow {
    path: PathBuf,
    label: String,
    is_active: bool,
    workspace: Entity<Workspace>,
}

impl WorkspaceEntryRow {
    pub(super) fn new(entry: &WorkspaceEntry, workspace: Entity<Workspace>, cx: &App) -> Self {
        let path = entry.path().to_path_buf();
        let is_active = WorkspaceState::global(cx).active_path() == Some(path.as_path());

        Self {
            path,
            label: entry.name().to_string(),
            is_active,
            workspace,
        }
    }
}

impl RenderOnce for WorkspaceEntryRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let entry_id = format!("workspace-entry-{}", self.path.display());
        let path = self.path;
        let workspace = self.workspace;

        div()
            .id(entry_id)
            .w_full()
            .h(px(28.0))
            .pl_3()
            .pr_3()
            .flex()
            .items_center()
            .rounded_sm()
            .bg(if self.is_active {
                Theme::global(cx).primary()
            } else {
                Theme::global(cx).bg_senodary()
            })
            .text_color(if self.is_active {
                Theme::global(cx).white()
            } else {
                Theme::global(cx).text_primary()
            })
            .cursor_pointer()
            .child(self.label)
            .on_click(move |_, _, cx| {
                workspace.update(cx, |_, cx| {
                    cx.emit(Event::OpenPath(path.clone()));
                });
            })
    }
}
