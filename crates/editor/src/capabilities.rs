use gpui::{App, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProFeature {
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppCapabilities;

impl AppCapabilities {
    pub fn init(cx: &mut App) {
        cx.set_global::<Self>(Self::default());
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn supports(&self, _feature: ProFeature) -> bool {
        true
    }
}

impl Global for AppCapabilities {}
