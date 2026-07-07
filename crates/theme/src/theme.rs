use std::time::{Duration, Instant};

use gpui::{App, AsyncApp, Global, Rgba, WindowAppearance};
use serde::{Deserialize, Serialize};

pub const APP_FONT_FAMILY: &str = "Zen Old Mincho";
const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const CURSOR_ACTIVITY_GRACE_PERIOD: Duration = Duration::from_millis(700);

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

struct CursorBlinkState {
    enabled: bool,
    visible: bool,
    activity_until: Option<Instant>,
}

impl Global for CursorBlinkState {}

pub fn init(cx: &mut App) {
    let theme = Theme::load(ThemeAppearance::Light);
    cx.set_global::<Theme>(theme);
    cx.set_global(CursorBlinkState {
        enabled: true,
        visible: true,
        activity_until: None,
    });
    spawn_cursor_blink_timer(cx);
}

fn spawn_cursor_blink_timer(cx: &mut App) {
    cx.spawn(|cx: &mut AsyncApp| {
        let app = cx.clone();
        async move {
            loop {
                app.background_executor().timer(CURSOR_BLINK_INTERVAL).await;
                app.update(|cx| {
                    let cursor_blink = cx.global_mut::<CursorBlinkState>();
                    cursor_blink.visible = !cursor_blink.visible;
                    if cursor_blink.enabled {
                        cx.refresh_windows();
                    }
                });
            }
        }
    })
    .detach();
}

pub fn set_cursor_blink_enabled(enabled: bool, cx: &mut App) {
    let cursor_blink = cx.global_mut::<CursorBlinkState>();
    cursor_blink.enabled = enabled;
    cursor_blink.visible = true;
    cursor_blink.activity_until = None;
    cx.refresh_windows();
}

pub fn note_cursor_activity(cx: &mut App) {
    let cursor_blink = cx.global_mut::<CursorBlinkState>();
    cursor_blink.visible = true;
    cursor_blink.activity_until = Some(Instant::now() + CURSOR_ACTIVITY_GRACE_PERIOD);
    cx.refresh_windows();
}

pub fn cursor_should_be_visible(cx: &App) -> bool {
    let cursor_blink = cx.global::<CursorBlinkState>();
    if !cursor_blink.enabled {
        return true;
    }
    if cursor_blink
        .activity_until
        .is_some_and(|activity_until| Instant::now() < activity_until)
    {
        return true;
    }
    cursor_blink.visible
}

pub fn apply_mode(mode: ThemeMode, cx: &mut App) {
    let system_appearance = cx.window_appearance();
    apply_mode_for_window_appearance(mode, system_appearance, cx);
}

pub fn apply_mode_for_window_appearance(
    mode: ThemeMode,
    system_appearance: WindowAppearance,
    cx: &mut App,
) {
    *Theme::global_mut(cx) = Theme::load(mode.theme_appearance(system_appearance));
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::System
    }
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "システムに合わせる",
            Self::Light => "ライト",
            Self::Dark => "ダーク",
        }
    }

    fn theme_appearance(self, system_appearance: WindowAppearance) -> ThemeAppearance {
        match self {
            Self::System => ThemeAppearance::from_window_appearance(system_appearance),
            Self::Light => ThemeAppearance::Light,
            Self::Dark => ThemeAppearance::Dark,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThemeAppearance {
    Light,
    Dark,
}

impl ThemeAppearance {
    fn from_window_appearance(window_appearance: WindowAppearance) -> Self {
        match window_appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
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

    fn load(appearance: ThemeAppearance) -> Self {
        let rgba = |red: f32, green: f32, blue: f32, alpha: f32| Rgba {
            r: red,
            g: green,
            b: blue,
            a: alpha,
        };

        match appearance {
            ThemeAppearance::Light => Self {
                primary: rgba(0.859, 0.718, 0.525, 1.),
                senodary: rgba(0.890, 0.796, 0.667, 1.),
                text_primary: rgba(0., 0., 0., 1.),
                text_senodary: rgba(0.439, 0.353, 0.290, 1.),
                bg_primary: rgba(1., 1., 1., 1.),
                bg_senodary: rgba(0.961, 0.961, 0.961, 1.),
                selection_range: rgba(0.541, 0.361, 0.965, 0.200),
                black: rgba(0., 0., 0., 1.),
                white: rgba(1., 1., 1., 1.),
            },
            ThemeAppearance::Dark => Self {
                primary: rgba(0.784, 0.800, 0.824, 1.),
                senodary: rgba(0.561, 0.588, 0.624, 1.),
                text_primary: rgba(0.957, 0.957, 0.961, 1.),
                text_senodary: rgba(0.682, 0.706, 0.737, 1.),
                bg_primary: rgba(0.063, 0.067, 0.071, 1.),
                bg_senodary: rgba(0.125, 0.133, 0.149, 1.),
                selection_range: rgba(0.953, 0.831, 0.353, 0.350),
                black: rgba(0.957, 0.957, 0.961, 1.),
                white: rgba(0.063, 0.067, 0.071, 1.),
            },
        }
    }
}
