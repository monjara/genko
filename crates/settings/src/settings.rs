use std::{
    fs,
    path::{Path, PathBuf},
};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

const DEFAULT_ROWS_PER_COLUMN: usize = 20;
const MIN_ROWS_PER_COLUMN: usize = 1;
const MAX_ROWS_PER_COLUMN: usize = 60;
const DEFAULT_CELL_SIZE: usize = 28;
const MIN_CELL_SIZE: usize = 20;
const MAX_CELL_SIZE: usize = 60;
const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub show_grid_lines: bool,
    pub hanging_punctuation: bool,
    pub cell_size: usize,
    pub rows_per_column: Option<usize>,
    #[serde(rename = "vimMode")]
    pub vim_mode: bool,
}

impl Global for AppSettings {}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_grid_lines: true,
            hanging_punctuation: true,
            cell_size: DEFAULT_CELL_SIZE,
            rows_per_column: None,
            vim_mode: false,
        }
    }
}

pub fn init(cx: &mut App) {
    let state = AppSettings::load();
    cx.set_global::<AppSettings>(state);
}

impl AppSettings {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    pub fn default_rows_per_column() -> usize {
        DEFAULT_ROWS_PER_COLUMN
    }

    pub fn min_rows_per_column() -> usize {
        MIN_ROWS_PER_COLUMN
    }

    pub fn max_rows_per_column() -> usize {
        MAX_ROWS_PER_COLUMN
    }

    pub fn default_cell_size() -> usize {
        DEFAULT_CELL_SIZE
    }

    pub fn min_cell_size() -> usize {
        MIN_CELL_SIZE
    }

    pub fn max_cell_size() -> usize {
        MAX_CELL_SIZE
    }

    pub fn load() -> Self {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("genko");
        Self::load_from_base_dirs(&xdg_dirs)
    }

    pub fn save(&self) -> Result<(), String> {
        let settings = self.normalized();
        let xdg_dirs = xdg::BaseDirectories::with_prefix("genko");
        let settings_path = xdg_dirs
            .place_config_file(SETTINGS_FILE)
            .map_err(|error| format!("設定ファイルの保存先を作成できません: {error}"))?;
        settings.save_to_file(&settings_path)
    }

    pub fn normalized(&self) -> Self {
        Self {
            show_grid_lines: self.show_grid_lines,
            hanging_punctuation: self.hanging_punctuation,
            cell_size: self.cell_size.clamp(MIN_CELL_SIZE, MAX_CELL_SIZE),
            rows_per_column: self
                .rows_per_column
                .map(|rows| rows.clamp(MIN_ROWS_PER_COLUMN, MAX_ROWS_PER_COLUMN)),
            vim_mode: self.vim_mode,
        }
    }

    fn load_from_base_dirs(xdg_dirs: &xdg::BaseDirectories) -> Self {
        Self::load_from_config_file(xdg_dirs.find_config_file(SETTINGS_FILE))
    }

    fn load_from_config_file(settings_path: Option<PathBuf>) -> Self {
        let Some(settings_path) = settings_path else {
            return Self::default();
        };

        let Ok(settings_json) = fs::read_to_string(settings_path) else {
            return Self::default();
        };

        serde_json::from_str::<Self>(&settings_json)
            .map(|settings| settings.normalized())
            .unwrap_or_default()
    }

    fn save_to_file(&self, settings_path: &Path) -> Result<(), String> {
        let settings_json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("設定をJSONへ変換できません: {error}"))?;
        fs::write(settings_path, settings_json)
            .map_err(|error| format!("設定ファイルを書き込めません: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn test_settings_dir(name: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!(
            "genko_settings_test_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn uses_default_when_settings_file_is_missing() {
        let dir = test_settings_dir("missing");
        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings, AppSettings::default());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_show_grid_lines_from_settings_file() {
        let dir = test_settings_dir("loads");
        fs::write(
            dir.join("settings.json"),
            r#"{"show_grid_lines": false, "rows_per_column": 24}"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert!(!settings.show_grid_lines);
        assert!(settings.hanging_punctuation);
        assert_eq!(settings.cell_size, DEFAULT_CELL_SIZE);
        assert_eq!(settings.rows_per_column, Some(24));
        assert!(!settings.vim_mode);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uses_auto_rows_per_column_when_missing() {
        let dir = test_settings_dir("rows_missing");
        fs::write(dir.join("settings.json"), r#"{"show_grid_lines": false}"#).unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.rows_per_column, None);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uses_auto_rows_per_column_when_null() {
        let dir = test_settings_dir("rows_null");
        fs::write(dir.join("settings.json"), r#"{"rows_per_column": null}"#).unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.rows_per_column, None);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clamps_rows_per_column_from_settings_file() {
        let dir = test_settings_dir("rows_clamp");
        fs::write(
            dir.join("settings.json"),
            format!(r#"{{"rows_per_column": {}}}"#, MAX_ROWS_PER_COLUMN + 1),
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.rows_per_column, Some(MAX_ROWS_PER_COLUMN));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saves_show_grid_lines_to_settings_file() {
        let dir = test_settings_dir("saves");
        let settings_path = dir.join("settings.json");
        let settings = AppSettings {
            show_grid_lines: false,
            hanging_punctuation: false,
            cell_size: 32,
            rows_per_column: Some(24),
            vim_mode: true,
        };

        settings.save_to_file(&settings_path).unwrap();

        let reloaded = AppSettings::load_from_config_file(Some(settings_path));
        assert!(!reloaded.show_grid_lines);
        assert!(!reloaded.hanging_punctuation);
        assert_eq!(reloaded.cell_size, 32);
        assert_eq!(reloaded.rows_per_column, Some(24));
        assert!(reloaded.vim_mode);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_vim_mode_from_settings_file() {
        let dir = test_settings_dir("vim_mode");
        fs::write(
            dir.join("settings.json"),
            r#"{"show_grid_lines": false, "rows_per_column": 24, "vimMode": true}"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert!(settings.vim_mode);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_hanging_punctuation_from_settings_file() {
        let dir = test_settings_dir("hanging_punctuation");
        fs::write(
            dir.join("settings.json"),
            r#"{"hanging_punctuation": false}"#,
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert!(!settings.hanging_punctuation);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clamps_cell_size_from_settings_file() {
        let dir = test_settings_dir("cell_size_clamp");
        fs::write(
            dir.join("settings.json"),
            format!(r#"{{"cell_size": {}}}"#, MAX_CELL_SIZE + 1),
        )
        .unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert_eq!(settings.cell_size, MAX_CELL_SIZE);

        let _ = fs::remove_dir_all(dir);
    }
}
