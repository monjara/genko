use std::{
    fs, io,
    path::{Path, PathBuf},
};

use gpui::actions;

actions!(workspace, [ToggleWorkspacePane]);

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

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
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
