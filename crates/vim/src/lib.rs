use gpui::Render;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VimState {
    mode: VimMode,
    visual_anchor_cell: Option<usize>,
}

impl VimState {
    pub fn new(enabled: bool) -> Self {
        Self {
            mode: if enabled {
                VimMode::Normal
            } else {
                VimMode::Insert
            },
            visual_anchor_cell: None,
        }
    }

    pub fn reset_for_enabled(&mut self, enabled: bool) {
        *self = Self::new(enabled);
    }

    pub fn mode(&self) -> VimMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: VimMode) {
        self.mode = mode;
    }

    pub fn visual_anchor_cell(&self) -> Option<usize> {
        self.visual_anchor_cell
    }

    pub fn set_visual_anchor_cell(&mut self, anchor: Option<usize>) {
        self.visual_anchor_cell = anchor;
    }

    pub fn key_context(&self, enabled: bool) -> &'static str {
        if !enabled {
            "Genko vim_mode=off"
        } else {
            match self.mode {
                VimMode::Normal => "Genko vim_mode=normal",
                VimMode::Insert => "Genko vim_mode=insert",
                VimMode::Visual => "Genko vim_mode=visual",
            }
        }
    }

    pub fn is_command_mode(&self, enabled: bool) -> bool {
        enabled && self.mode != VimMode::Insert
    }

    pub fn status_label(&self, enabled: bool) -> &'static str {
        if !enabled {
            ""
        } else {
            match self.mode {
                VimMode::Normal => " / NORMAL",
                VimMode::Insert => " / INSERT",
                VimMode::Visual => " / VISUAL",
            }
        }
    }
}

pub struct Vim {}

impl Render for Vim {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::Empty
    }
}
