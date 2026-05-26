use gpui::{App, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProFeature {
    RichText,
    ExportWord,
    ExportEpub,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppCapabilities {
    pro_enabled: bool,
}

impl AppCapabilities {
    pub fn init(cx: &mut App) {
        cx.set_global::<Self>(Self::default());
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    pub fn set_pro_enabled(&mut self, pro_enabled: bool) {
        self.pro_enabled = pro_enabled;
    }

    pub fn supports(&self, feature: ProFeature) -> bool {
        match feature {
            ProFeature::RichText | ProFeature::ExportWord | ProFeature::ExportEpub => {
                self.pro_enabled
            }
        }
    }
}

impl Global for AppCapabilities {}
