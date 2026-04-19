use std::{fs, path::PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct AppSettings {
    pub(crate) show_grid_lines: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_grid_lines: true,
        }
    }
}

impl AppSettings {
    const SETTINGS_FILE: &'static str = "settings.json";

    pub(crate) fn load() -> Self {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("genko");
        Self::load_from_base_dirs(&xdg_dirs)
    }

    fn load_from_base_dirs(xdg_dirs: &xdg::BaseDirectories) -> Self {
        Self::load_from_config_file(xdg_dirs.find_config_file(Self::SETTINGS_FILE))
    }

    fn load_from_config_file(settings_path: Option<PathBuf>) -> Self {
        let Some(settings_path) = settings_path else {
            return Self::default();
        };

        let Ok(settings_json) = fs::read_to_string(settings_path) else {
            return Self::default();
        };

        serde_json::from_str(&settings_json).unwrap_or_default()
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
        fs::write(dir.join("settings.json"), r#"{"show_grid_lines": false}"#).unwrap();

        let settings = AppSettings::load_from_config_file(Some(dir.join("settings.json")));

        assert!(!settings.show_grid_lines);

        let _ = fs::remove_dir_all(dir);
    }
}
