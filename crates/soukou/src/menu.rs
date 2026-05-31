use gpui::{App, Menu, MenuItem};

use crate::{CheckForUpdates, ExportTxt, OpenFile, OpenSettings, Quit, SaveFile};

const APP_NAME: &str = "草稿";
const OPEN_PROMPT_LABEL: &str = "開く";
const SETTINGS_MENU_LABEL: &str = "設定";
const CHECK_FOR_UPDATES_MENU_LABEL: &str = "更新を確認";
const QUIT_MENU_LABEL: &str = "終了";
const FILE_MENU_LABEL: &str = "ファイル";
const SAVE_MENU_LABEL: &str = "保存";
const EXPORT_TXT_MENU_LABEL: &str = "txtエクスポート";

pub(crate) fn init(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            disabled: false,
            name: APP_NAME.into(),
            items: vec![
                MenuItem::action(SETTINGS_MENU_LABEL, OpenSettings),
                MenuItem::action(CHECK_FOR_UPDATES_MENU_LABEL, CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action(QUIT_MENU_LABEL, Quit),
            ],
        },
        Menu {
            disabled: false,
            name: FILE_MENU_LABEL.into(),
            items: vec![
                MenuItem::action(OPEN_PROMPT_LABEL, OpenFile),
                MenuItem::action(SAVE_MENU_LABEL, SaveFile),
                MenuItem::separator(),
                MenuItem::action(EXPORT_TXT_MENU_LABEL, ExportTxt),
            ],
        },
    ])
}
