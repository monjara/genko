use std::{
    fs, io,
    path::{Path, PathBuf},
};

use gpui::{
    App, Context, EventEmitter, Global, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, actions, div, prelude::FluentBuilder, px, svg,
};
use theme::Theme;

mod workspace_entry_row;

use workspace_entry_row::WorkspaceEntryRow;

actions!(workspace, [ToggleWorkspacePane, OpenWorkspace]);

const DEFAULT_WORKSPACE_PANE_WIDTH: f32 = 280.0;

#[derive(Clone, Debug)]
pub enum Event {
    OpenPath(PathBuf),
}

#[derive(Clone, Debug)]
pub struct WorkspaceState {
    root_dir: Option<PathBuf>,
    active_file: Option<PathBuf>,
    unsupported_file: Option<PathBuf>,
    entries: Vec<WorkspaceEntry>,
    pane_visible: bool,
    pane_width: f32,
}

impl WorkspaceState {
    pub fn init(cx: &mut App) {
        cx.set_global(Self {
            pane_width: DEFAULT_WORKSPACE_PANE_WIDTH,
            ..Self::default()
        });
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    #[cfg(test)]
    fn new() -> Self {
        Self::default()
    }

    pub fn root_dir(&self) -> Option<&Path> {
        self.root_dir.as_deref()
    }

    fn active_file(&self) -> Option<&Path> {
        self.active_file.as_deref()
    }

    pub fn unsupported_file(&self) -> Option<&Path> {
        self.unsupported_file.as_deref()
    }

    pub fn active_path(&self) -> Option<&Path> {
        self.active_file().or_else(|| self.unsupported_file())
    }

    fn entries(&self) -> &[WorkspaceEntry] {
        &self.entries
    }

    pub fn is_pane_visible(&self) -> bool {
        self.pane_visible
    }

    pub fn pane_width(&self) -> f32 {
        self.pane_width
    }

    pub fn open_file(&mut self, path: PathBuf) {
        self.active_file = Some(path);
        self.unsupported_file = None;
    }

    pub fn open_saved_file(&mut self, path: PathBuf, is_new_file: bool) {
        if self.entries.is_empty() && self.root_dir.is_none() {
            self.add_saved_entry(path.as_path());
        }
        self.open_file(path);
    }

    pub fn open_file_without_root(&mut self, path: PathBuf) {
        self.root_dir = None;
        self.entries.clear();
        self.active_file = Some(path);
        self.unsupported_file = None;
    }

    pub fn open_unsupported_file(&mut self, path: PathBuf) {
        self.active_file = None;
        self.unsupported_file = Some(path);
    }

    pub fn open_unsupported_file_without_root(&mut self, path: PathBuf) {
        self.root_dir = None;
        self.entries.clear();
        self.open_unsupported_file(path);
    }

    pub fn open_root(&mut self, root_dir: PathBuf, entries: Vec<WorkspaceEntry>) {
        self.root_dir = Some(root_dir);
        self.entries = entries;
        self.active_file = None;
        self.unsupported_file = None;
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

    fn add_saved_entry(&mut self, path: &Path) {
        let Some(root_dir) = self.root_dir.as_deref() else {
            return;
        };
        if path.parent() != Some(root_dir) || is_hidden(path) || !is_supported_text_file(path) {
            return;
        }
        if self.entries.iter().any(|entry| entry.path == path) {
            return;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        self.entries.push(WorkspaceEntry {
            path: path.to_path_buf(),
            name: name.to_string(),
        });
        self.entries
            .sort_by(|left, right| left.name.cmp(&right.name));
    }
}

impl Global for WorkspaceState {}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            root_dir: None,
            active_file: None,
            unsupported_file: None,
            entries: Vec::new(),
            pane_visible: false,
            pane_width: DEFAULT_WORKSPACE_PANE_WIDTH,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceEntry {
    path: PathBuf,
    name: String,
}

impl WorkspaceEntry {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

pub fn scan_workspace_entries(root_dir: &Path) -> io::Result<Vec<WorkspaceEntry>> {
    let mut entries = fs::read_dir(root_dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| !is_hidden(entry.path().as_path()))
        .filter_map(|entry| {
            let path = entry.path();
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() || !is_supported_text_file(path.as_path()) {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_string();
            Some(WorkspaceEntry { path, name })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

pub struct Workspace {}

impl Workspace {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {}
    }

    fn toggle_pane(&mut self, cx: &mut Context<Self>) {
        WorkspaceState::global_mut(cx).toggle_pane();
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
}

impl EventEmitter<Event> for Workspace {}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = cx.entity();
        let root_label = WorkspaceState::global(cx)
            .root_dir()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("未選択")
            .to_string();
        let pane_width = WorkspaceState::global(cx).pane_width();
        let entries = WorkspaceState::global(cx).entries().to_vec();
        let entry_count = entries.len();
        let entry_elements = entries
            .iter()
            .map(|entry| WorkspaceEntryRow::new(entry, workspace.clone(), cx).into_any_element())
            .collect::<Vec<_>>();

        div()
            .id("workspace-pane")
            .w(px(pane_width))
            .h_full()
            .flex_none()
            .bg(Theme::global(cx).bg_senodary())
            .border_r_1()
            .border_color(Theme::global(cx).senodary())
            .on_action(cx.listener(Self::toggle_workspace_pane))
            .flex()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .w_full()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div().flex().items_center().gap_2().child(
                            div()
                                .id("workspace-open-file-button")
                                .w(px(34.0))
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .border_1()
                                .border_color(Theme::global(cx).primary())
                                .bg(Theme::global(cx).white())
                                .cursor_pointer()
                                .text_color(Theme::global(cx).primary())
                                .hover(|style| style.bg(Theme::global(cx).white()))
                                .child(
                                    svg()
                                        .external_path(icons::FOLDER_OPEN)
                                        .size_5()
                                        .text_color(Theme::global(cx).primary()),
                                )
                                .on_click(move |_, window, cx| {
                                    window.dispatch_action(Box::new(OpenWorkspace), cx);
                                }),
                        ),
                    )
                    .child(
                        div().flex_col().gap_1().child(
                            div()
                                .text_sm()
                                .text_color(Theme::global(cx).text_senodary())
                                .child(root_label),
                        ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .when(entry_count == 0, |this| {
                                this.child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .text_sm()
                                        .text_color(Theme::global(cx).text_senodary())
                                        .child("txtファイルはありません"),
                                )
                            })
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
    use gpui::TestAppContext;

    fn test_workspace_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "soukou_workspace_test_{}_{}",
            name,
            std::process::id()
        ));
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                panic!(
                    "failed to remove test directory {}: {error}",
                    path.display()
                );
            }
        }
        fs::create_dir_all(&path).unwrap();
        path
    }

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
        workspace.open_root(PathBuf::from("/tmp/project"), Vec::new());

        workspace.open_file_without_root(PathBuf::from("/tmp/standalone.txt"));

        assert!(workspace.root_dir().is_none());
        assert!(workspace.entries().is_empty());
        assert_eq!(
            workspace.active_file(),
            Some(Path::new("/tmp/standalone.txt"))
        );
    }

    #[test]
    fn open_saved_new_file_adds_workspace_entry() {
        let root_dir = PathBuf::from("/tmp/project");
        let mut workspace = WorkspaceState::new();
        workspace.open_root(
            root_dir.clone(),
            vec![WorkspaceEntry {
                path: root_dir.join("b.txt"),
                name: "b.txt".to_string(),
            }],
        );

        workspace.open_saved_file(root_dir.join("a.txt"), true);

        assert_eq!(
            workspace.active_file(),
            Some(root_dir.join("a.txt").as_path())
        );
        assert_eq!(
            workspace
                .entries()
                .iter()
                .map(WorkspaceEntry::name)
                .collect::<Vec<_>>(),
            vec!["a.txt", "b.txt"]
        );
    }

    #[test]
    fn open_saved_existing_file_does_not_add_workspace_entry() {
        let root_dir = PathBuf::from("/tmp/project");
        let mut workspace = WorkspaceState::new();
        workspace.open_root(root_dir.clone(), Vec::new());

        workspace.open_saved_file(root_dir.join("a.txt"), false);

        assert!(workspace.entries().is_empty());
        assert_eq!(
            workspace.active_file(),
            Some(root_dir.join("a.txt").as_path())
        );
    }

    #[test]
    fn scan_workspace_entries_lists_only_direct_txt_files() {
        let dir = test_workspace_dir("direct_txt");
        fs::write(dir.join("b.md"), "ignored").unwrap();
        fs::write(dir.join("a.txt"), "shown").unwrap();
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("nested").join("nested.txt"), "ignored").unwrap();

        let entries = scan_workspace_entries(dir.as_path()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), "a.txt");
        assert_eq!(entries[0].path(), dir.join("a.txt").as_path());

        fs::remove_dir_all(dir).unwrap();
    }

    #[gpui::test]
    fn toggle_workspace_pane_action_updates_global_visibility(cx: &mut TestAppContext) {
        cx.update(|cx| {
            theme::init(cx);
            WorkspaceState::init(cx);
        });
        let (workspace, cx) = cx.add_window_view(|_, cx| Workspace::new(cx));

        assert!(!cx.read_global::<WorkspaceState, _>(|workspace_state, _| {
            workspace_state.is_pane_visible()
        }));

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.toggle_workspace_pane(&ToggleWorkspacePane, window, cx);
        });
        assert!(cx.read_global::<WorkspaceState, _>(|workspace_state, _| {
            workspace_state.is_pane_visible()
        }));

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.toggle_workspace_pane(&ToggleWorkspacePane, window, cx);
        });
        assert!(!cx.read_global::<WorkspaceState, _>(|workspace_state, _| {
            workspace_state.is_pane_visible()
        }));
    }
}
