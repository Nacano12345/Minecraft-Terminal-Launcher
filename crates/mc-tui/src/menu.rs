//! Top-level (Esc) menu state.

use mc_plugin::{PluginEntry, PluginRegistry};
use ratatui::layout::Rect;

/// Display version shown under the logo.
pub const VERSION: &str = "0.0_Alpha";

/// Five-line "QCL" block logo.
pub const LOGO: [&str; 5] = [
    " ██████   ██████  ██      ",
    "██    ██ ██       ██      ",
    "██    ██ ██       ██      ",
    "██  ▄ ██ ██       ██      ",
    " ██████   ██████  ███████ ",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuScreen {
    Main,
    Options,
    Plugins,
    Actions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Options,
    Plugins,
    Actions,
    Exit,
}

/// Editable rows inside the Options screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionField {
    Username,
    Memory,
    Java,
}

impl OptionField {
    pub const ALL: [OptionField; 3] = [
        OptionField::Username,
        OptionField::Memory,
        OptionField::Java,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OptionField::Username => "Username",
            OptionField::Memory => "Memory",
            OptionField::Java => "Java",
        }
    }
}

impl MenuItem {
    pub const ALL: [MenuItem; 4] = [
        MenuItem::Options,
        MenuItem::Plugins,
        MenuItem::Actions,
        MenuItem::Exit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Options => "Options",
            MenuItem::Plugins => "Plugins",
            MenuItem::Actions => "Actions",
            MenuItem::Exit => "Exit",
        }
    }
}

pub struct MenuState {
    pub open: bool,
    pub screen: MenuScreen,
    pub selected: usize,
    pub plugins: Vec<PluginEntry>,
    pub plugin_error: Option<String>,
    /// Clickable rows, rebuilt every frame.
    pub hits: Vec<(Rect, MenuItem)>,
    pub option_selected: usize,
    pub option_editing: Option<OptionField>,
    pub option_buffer: String,
    pub option_error: Option<String>,
    /// Clickable option rows, rebuilt every frame.
    pub option_hits: Vec<(Rect, usize)>,
    pub plugin_selected: usize,
    /// Clickable plugin rows, rebuilt every frame.
    pub plugin_hits: Vec<(Rect, usize)>,
    pub action_selected: usize,
    /// Clickable action rows, rebuilt every frame.
    pub action_hits: Vec<(Rect, usize)>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            open: false,
            screen: MenuScreen::Main,
            selected: 0,
            plugins: Vec::new(),
            plugin_error: None,
            hits: Vec::new(),
            option_selected: 0,
            option_editing: None,
            option_buffer: String::new(),
            option_error: None,
            option_hits: Vec::new(),
            plugin_selected: 0,
            plugin_hits: Vec::new(),
            action_selected: 0,
            action_hits: Vec::new(),
        }
    }
}

impl MenuState {
    pub fn reset(&mut self) {
        self.open = true;
        self.screen = MenuScreen::Main;
        self.selected = 0;
        self.option_selected = 0;
        self.option_editing = None;
        self.option_buffer.clear();
        self.option_error = None;
        self.option_hits.clear();
        self.plugin_selected = 0;
        self.plugin_hits.clear();
        self.action_selected = 0;
        self.action_hits.clear();
    }

    pub fn current_option(&self) -> Option<OptionField> {
        OptionField::ALL.get(self.option_selected).copied()
    }

    pub fn move_option(&mut self, delta: isize) {
        if self.option_editing.is_some() {
            return;
        }
        let count = OptionField::ALL.len() as isize;
        self.option_selected = (self.option_selected as isize + delta).rem_euclid(count) as usize;
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.screen != MenuScreen::Main {
            return;
        }
        let count = MenuItem::ALL.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
    }

    pub fn current(&self) -> Option<MenuItem> {
        MenuItem::ALL.get(self.selected).copied()
    }

    pub fn load_plugins(&mut self, registry: &PluginRegistry) {
        self.plugins = registry.entries.clone();
        self.plugin_selected = self
            .plugin_selected
            .min(self.plugins.len().saturating_sub(1));
        self.plugin_error = None;
        self.plugin_hits.clear();
    }

    pub fn move_plugin(&mut self, delta: isize) {
        if self.screen != MenuScreen::Plugins || self.plugins.is_empty() {
            return;
        }
        let count = self.plugins.len() as isize;
        self.plugin_selected = (self.plugin_selected as isize + delta).rem_euclid(count) as usize;
    }

    pub fn current_plugin(&self) -> Option<&PluginEntry> {
        self.plugins.get(self.plugin_selected)
    }
}
