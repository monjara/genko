use std::{collections::BTreeMap, fs, path::Path, path::PathBuf, rc::Rc};

use gpui::{App, KeyBinding, KeyBindingContextPredicate};
use serde::Deserialize;

const KEYMAP_FILE: &str = "keymap.json";

#[cfg(target_os = "macos")]
const DEFAULT_KEYMAP_JSON: &str = include_str!("../resources/default-macos.json");
#[cfg(target_os = "linux")]
const DEFAULT_KEYMAP_JSON: &str = include_str!("../resources/default-linux.json");
#[cfg(target_os = "windows")]
const DEFAULT_KEYMAP_JSON: &str = include_str!("../resources/default-windows.json");
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
const DEFAULT_KEYMAP_JSON: &str = include_str!("../resources/default-linux.json");

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct KeymapFile(Vec<KeymapSection>);

#[derive(Debug, Deserialize)]
struct KeymapSection {
    #[serde(default)]
    context: String,
    #[serde(default)]
    bindings: BTreeMap<String, String>,
}

pub struct LoadedKeyBindings {
    pub key_bindings: Vec<KeyBinding>,
    pub error: Option<String>,
}

pub fn load_key_bindings(cx: &App) -> LoadedKeyBindings {
    let mut error = None;
    let mut key_bindings = match load_default_key_bindings(cx) {
        Ok(key_bindings) => key_bindings,
        Err(load_error) => {
            return LoadedKeyBindings {
                key_bindings: Vec::new(),
                error: Some(load_error),
            };
        }
    };

    if let Some(keymap_path) = existing_keymap_file_path() {
        match load_key_bindings_from_file(&keymap_path, cx) {
            Ok(mut user_key_bindings) => key_bindings.append(&mut user_key_bindings),
            Err(load_error) => {
                error = Some(format!(
                    "{} を読み込めませんでした。\n\n{}",
                    keymap_path.display(),
                    load_error
                ));
            }
        }
    }

    LoadedKeyBindings {
        key_bindings,
        error,
    }
}

fn load_default_key_bindings(cx: &App) -> Result<Vec<KeyBinding>, String> {
    load_key_bindings_from_json(DEFAULT_KEYMAP_JSON, cx)
}

fn load_key_bindings_from_file(keymap_path: &Path, cx: &App) -> Result<Vec<KeyBinding>, String> {
    let keymap_json = fs::read_to_string(keymap_path)
        .map_err(|error| format!("キーマップファイルを読めません: {error}"))?;
    load_key_bindings_from_json(&keymap_json, cx)
}

fn load_key_bindings_from_json(keymap_json: &str, cx: &App) -> Result<Vec<KeyBinding>, String> {
    let keymap_file = serde_json::from_str::<KeymapFile>(keymap_json)
        .map_err(|error| format!("キーマップJSONを解析できません: {error}"))?;
    keymap_file.load(cx)
}

impl KeymapFile {
    fn load(&self, cx: &App) -> Result<Vec<KeyBinding>, String> {
        let mut key_bindings = Vec::new();

        for section in &self.0 {
            let context_predicate = section.context_predicate()?;
            for (keystroke, action_name) in &section.bindings {
                key_bindings.push(load_key_binding(
                    keystroke,
                    action_name,
                    context_predicate.clone(),
                    cx,
                )?);
            }
        }

        Ok(key_bindings)
    }
}

impl KeymapSection {
    fn context_predicate(&self) -> Result<Option<Rc<KeyBindingContextPredicate>>, String> {
        let context = self.context.trim();
        if context.is_empty() {
            return Ok(None);
        }

        KeyBindingContextPredicate::parse(context)
            .map(Rc::new)
            .map(Some)
            .map_err(|error| format!("context `{context}` を解析できません: {error}"))
    }
}

fn load_key_binding(
    keystroke: &str,
    action_name: &str,
    context_predicate: Option<Rc<KeyBindingContextPredicate>>,
    cx: &App,
) -> Result<KeyBinding, String> {
    let keystroke = keystroke.trim();
    if keystroke.is_empty() {
        return Err("空のキーストロークは指定できません".to_string());
    }

    let action_name = action_name.trim();
    if action_name.is_empty() {
        return Err(format!("`{keystroke}` の action が空です"));
    }

    let action = cx
        .build_action(action_name, None)
        .map_err(|error| format!("`{action_name}` を action として解決できません: {error}"))?;

    KeyBinding::load(
        keystroke,
        action,
        context_predicate,
        false,
        None,
        cx.keyboard_mapper().as_ref(),
    )
    .map_err(|error| format!("`{keystroke}` は不正なキーストロークです: {error}"))
}

fn existing_keymap_file_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        keymap_file_path().filter(|path| path.exists())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("soukou");
        xdg_dirs.find_config_file(KEYMAP_FILE)
    }
}

#[cfg(target_os = "windows")]
fn keymap_file_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|appdata| PathBuf::from(appdata).join("soukou").join(KEYMAP_FILE))
}

#[cfg(test)]
mod tests {
    use super::KeymapFile;

    const DEFAULT_MACOS_KEYMAP_JSON: &str = include_str!("../resources/default-macos.json");
    const DEFAULT_LINUX_KEYMAP_JSON: &str = include_str!("../resources/default-linux.json");
    const DEFAULT_WINDOWS_KEYMAP_JSON: &str = include_str!("../resources/default-windows.json");

    #[test]
    fn parses_default_action_keymap_files() {
        assert_platform_keymap(DEFAULT_MACOS_KEYMAP_JSON, "cmd-b");
        assert_platform_keymap(DEFAULT_LINUX_KEYMAP_JSON, "ctrl-b");
        assert_platform_keymap(DEFAULT_WINDOWS_KEYMAP_JSON, "ctrl-b");
    }

    fn assert_platform_keymap(keymap_json: &str, bold_keystroke: &str) {
        let keymap_file = serde_json::from_str::<KeymapFile>(keymap_json).unwrap();

        assert!(keymap_file.0.iter().any(|section| {
            section.bindings.get(bold_keystroke).map(String::as_str) == Some("menu::RichTextBold")
        }));
        assert!(
            keymap_file
                .0
                .iter()
                .any(|section| section.context == "SoukouTextInput"
                    && section.bindings.get("escape").map(String::as_str)
                        == Some("soukou::CancelRubyEditor"))
        );
    }
}
