use std::{
    fs, io,
    path::{Path, PathBuf},
};

use gpui::{
    AnyElement, Context, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Window, actions, div, px,
};
use theme::Theme;

actions!(
    workspace,
    [ToggleWorkspacePane, OpenWorkspaceFile]
);

pub const WORKSPACE_PANE_WIDTH: f32 = 280.0;

#[derive(Clone, Debug)]
pub enum Event {
    OpenPath(PathBuf),
}

#[derive(Clone, Debug, Default)]
pub struct WorkspaceState {
    root_dir: Option<PathBuf>,
    active_file: Option<PathBuf>,
    entries: Vec<WorkspaceEntry>,
    pane_visible: bool,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root_dir(&self) -> Option<&Path> {
        self.root_dir.as_deref()
    }

    pub fn active_file(&self) -> Option<&Path> {
        self.active_file.as_deref()
    }

    pub fn entries(&self) -> &[WorkspaceEntry] {
        &self.entries
    }

    pub fn is_pane_visible(&self) -> bool {
        self.pane_visible
    }

    pub fn open_file(&mut self, path: PathBuf) {
        self.active_file = Some(path);
    }

    pub fn open_file_without_root(&mut self, path: PathBuf) {
        self.root_dir = None;
        self.entries.clear();
        self.active_file = Some(path);
    }

    pub fn open_root(&mut self, root_dir: PathBuf, entries: Vec<WorkspaceEntry>) {
        self.root_dir = Some(root_dir);
        self.entries = entries;
    }

    pub fn suggested_save_directory(&self) -> Option<&Path> {
        self.active_file()
            .and_then(Path::parent)
            .or_else(|| self.root_dir())
    }

    pub fn suggested_file_name(&self) -> Option<&str> {
        self.active_file()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
    }

    pub fn toggle_pane(&mut self) {
        self.pane_visible = !self.pane_visible;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceEntry {
    path: PathBuf,
    name: String,
    depth: usize,
    kind: WorkspaceEntryKind,
}

impl WorkspaceEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.kind, WorkspaceEntryKind::Directory)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceEntryKind {
    Directory,
    File,
}

pub fn scan_workspace_entries(root_dir: &Path) -> io::Result<Vec<WorkspaceEntry>> {
    let mut entries = Vec::new();
    collect_workspace_entries(root_dir, root_dir, 0, &mut entries)?;
    Ok(entries)
}

fn collect_workspace_entries(
    root_dir: &Path,
    dir: &Path,
    depth: usize,
    entries: &mut Vec<WorkspaceEntry>,
) -> io::Result<()> {
    let mut children = fs::read_dir(dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| !is_hidden(entry.path().as_path()))
        .collect::<Vec<_>>();

    children.sort_by(|left, right| {
        let left_is_dir = left.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        let right_is_dir = right.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    for child in children {
        let path = child.path();
        let metadata = child.file_type()?;
        if metadata.is_file() && !is_supported_text_file(path.as_path()) {
            continue;
        }
        let kind = if metadata.is_dir() {
            WorkspaceEntryKind::Directory
        } else {
            WorkspaceEntryKind::File
        };
        let relative = path.strip_prefix(root_dir).unwrap_or(path.as_path());
        entries.push(WorkspaceEntry {
            path: path.clone(),
            name: relative.display().to_string(),
            depth,
            kind,
        });

        if metadata.is_dir() {
            collect_workspace_entries(root_dir, &path, depth + 1, entries)?;
        }
    }

    Ok(())
}

pub struct Workspace {
    state: WorkspaceState,
}

impl Workspace {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            state: WorkspaceState::new(),
        }
    }

    pub fn active_file(&self) -> Option<&Path> {
        self.state.active_file()
    }

    pub fn is_pane_visible(&self) -> bool {
        self.state.is_pane_visible()
    }

    pub fn suggested_save_directory(&self) -> Option<&Path> {
        self.state.suggested_save_directory()
    }

    pub fn suggested_file_name(&self) -> Option<&str> {
        self.state.suggested_file_name()
    }

    pub fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.state.open_file(path);
        cx.notify();
    }

    pub fn open_file_without_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.state.open_file_without_root(path);
        cx.notify();
    }

    pub fn open_root(
        &mut self,
        root_dir: PathBuf,
        entries: Vec<WorkspaceEntry>,
        cx: &mut Context<Self>,
    ) {
        self.state.open_root(root_dir, entries);
        cx.notify();
    }

    pub fn toggle_pane(&mut self, cx: &mut Context<Self>) {
        self.state.toggle_pane();
        cx.notify();
    }

    fn toggle_workspace_pane(
        &mut self,
        _: &ToggleWorkspacePane,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_pane(cx);
    }

    fn render_entry(
        &self,
        entry: &WorkspaceEntry,
        workspace: Entity<Self>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = entry.path().to_path_buf();
        let is_active = self.state.active_file() == Some(path.as_path());
        let is_dir = entry.is_dir();
        let label = if is_dir {
            format!("{} /", entry.name())
        } else {
            entry.name().to_string()
        };
        let indent = px(12.0 * entry.depth() as f32 + 12.0);
        let entry_id = format!("workspace-entry-{}", path.display());

        div()
            .id(entry_id)
            .w_full()
            .h(px(28.0))
            .pl(indent)
            .pr_3()
            .flex()
            .items_center()
            .rounded_sm()
            .bg(if is_active {
                Theme::global(cx).primary()
            } else {
                Theme::global(cx).bg_senodary()
            })
            .text_color(if is_active {
                Theme::global(cx).white()
            } else if is_dir {
                Theme::global(cx).text_senodary()
            } else {
                Theme::global(cx).text_primary()
            })
            .cursor_pointer()
            .child(label)
            .on_click(move |_, _, cx| {
                if is_dir {
                    return;
                }

                let _ = workspace.update(cx, |_, cx| {
                    cx.emit(Event::OpenPath(path.clone()));
                });
            })
            .into_any_element()
    }
}

impl EventEmitter<Event> for Workspace {}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = cx.entity();
        let root_label = self
            .state
            .root_dir()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("未選択")
            .to_string();
        let entry_elements = self
            .state
            .entries()
            .iter()
            .map(|entry| self.render_entry(entry, workspace.clone(), cx))
            .collect::<Vec<_>>();

        div()
            .id("workspace-pane")
            .w(px(WORKSPACE_PANE_WIDTH))
            .h_full()
            .flex_none()
            .bg(Theme::global(cx).bg_senodary())
            .border_r_1()
            .border_color(Theme::global(cx).senodary())
            .on_action(cx.listener(Self::toggle_workspace_pane))
            .child(
                div()
                    .w_full()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().font_weight(gpui::FontWeight::BOLD).child("Workspace"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(Theme::global(cx).text_senodary())
                                    .child(root_label),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("workspace-open-file-button")
                                    .px_3()
                                    .h(px(32.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(Theme::global(cx).primary())
                                    .bg(Theme::global(cx).white())
                                    .cursor_pointer()
                                    .child("ファイル")
                                    .on_click(move |_, window, cx| {
                                        window.dispatch_action(Box::new(OpenWorkspaceFile), cx);
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .children(entry_elements),
                    ),
            )
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn is_supported_text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_save_directory_prefers_active_file_parent() {
        let mut workspace = WorkspaceState::new();
        workspace.open_root(PathBuf::from("/tmp/project"), Vec::new());
        workspace.open_file(PathBuf::from("/tmp/project/src/main.rs"));
        assert_eq!(
            workspace.suggested_save_directory(),
            Some(Path::new("/tmp/project/src"))
        );
    }

    #[test]
    fn open_file_without_root_clears_workspace_entries() {
        let mut workspace = WorkspaceState::new();
        workspace.open_root(
            PathBuf::from("/tmp/project"),
            vec![WorkspaceEntry {
                path: PathBuf::from("/tmp/project/src"),
                name: "src".into(),
                depth: 0,
                kind: WorkspaceEntryKind::Directory,
            }],
        );

        workspace.open_file_without_root(PathBuf::from("/tmp/standalone.txt"));

        assert!(workspace.root_dir().is_none());
        assert!(workspace.entries().is_empty());
        assert_eq!(
            workspace.active_file(),
            Some(Path::new("/tmp/standalone.txt"))
        );
    }
}
