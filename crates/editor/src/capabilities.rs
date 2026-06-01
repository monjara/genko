use gpui::{App, Global};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct AppCapabilities;

impl AppCapabilities {
    pub(super) fn init(cx: &mut App) {
        cx.set_global::<Self>(Self::default());
    }

    #[allow(dead_code)]
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    #[allow(dead_code)]
    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }
}

impl Global for AppCapabilities {}
