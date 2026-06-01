use gpui::{App, Global, Rgba};
use serde::Deserialize;

pub const APP_FONT_FAMILY: &str = "Zen Old Mincho";

pub struct Theme {
    primary: Rgba,
    senodary: Rgba,
    text_primary: Rgba,
    text_senodary: Rgba,
    bg_primary: Rgba,
    bg_senodary: Rgba,
    selection_range: Rgba,
    black: Rgba,
    white: Rgba,
}

impl Global for Theme {}

pub fn init(cx: &mut App) {
    let theme = Theme::load_default();
    cx.set_global::<Theme>(theme);
}

impl Theme {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    #[allow(dead_code)]
    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    pub fn primary(&self) -> Rgba {
        self.primary
    }
    pub fn senodary(&self) -> Rgba {
        self.senodary
    }
    pub fn text_primary(&self) -> Rgba {
        self.text_primary
    }
    pub fn text_senodary(&self) -> Rgba {
        self.text_senodary
    }
    pub fn bg_primary(&self) -> Rgba {
        self.bg_primary
    }
    pub fn bg_senodary(&self) -> Rgba {
        self.bg_senodary
    }
    pub fn selection_range(&self) -> Rgba {
        self.selection_range
    }
    pub fn black(&self) -> Rgba {
        self.black
    }
    pub fn white(&self) -> Rgba {
        self.white
    }

    fn load_default() -> Self {
        // TODO: 複数のテーマを作成し、1テーマごとに1jsonファイルを作成する
        //       設定画面からテーマを選択できるようにする
        let json_str = include_str!("../themes/default.json");

        let theme_json: ThemeJson =
            serde_json::from_str(json_str).expect("Failed to parse default theme JSON");

        Self {
            primary: theme_json.primary,
            senodary: theme_json.senodary,
            text_primary: theme_json.text_primary,
            text_senodary: theme_json.text_senodary,
            bg_primary: theme_json.bg_primary,
            bg_senodary: theme_json.bg_senodary,
            selection_range: theme_json.selection_range,
            black: Rgba {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 255.,
            },
            white: Rgba {
                r: 255.,
                g: 255.,
                b: 255.,
                a: 255.,
            },
        }
    }
}

#[derive(Deserialize)]
struct ThemeJson {
    primary: Rgba,
    senodary: Rgba,
    text_primary: Rgba,
    text_senodary: Rgba,
    bg_primary: Rgba,
    bg_senodary: Rgba,
    selection_range: Rgba,
}
