use openidea_plugin::{ActionSource, host};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub show_tips_click_count: u32,
    pub propose_to_create_shortcut_count: u32,
    pub hard_mode: bool,
    pub menus_enabled: bool,
    pub toolbar_buttons_enabled: bool,
    pub context_menu_enabled: bool,
    pub search_enabled: bool,
    pub other_buttons_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_tips_click_count: 1,
            propose_to_create_shortcut_count: 3,
            hard_mode: false,
            menus_enabled: true,
            toolbar_buttons_enabled: true,
            context_menu_enabled: true,
            search_enabled: true,
            other_buttons_enabled: true,
        }
    }
}

impl Settings {
    /// Reads all plugin settings from the host, falling back to schema defaults.
    pub fn load() -> Self {
        let defaults = Self::default();
        let show_tips_click_count = host::setting_number("showTipsClickCount")
            .filter(|n| n.is_finite() && *n >= 1.0)
            .map(|n| n as u32)
            .unwrap_or(defaults.show_tips_click_count)
            .max(1);

        let propose_to_create_shortcut_count = host::setting_number("proposeToCreateShortcutCount")
            .filter(|n| n.is_finite() && *n >= 0.0)
            .map(|n| n as u32)
            .unwrap_or(defaults.propose_to_create_shortcut_count);

        Self {
            show_tips_click_count,
            propose_to_create_shortcut_count,
            hard_mode: host::setting_bool("hardMode").unwrap_or(defaults.hard_mode),
            menus_enabled: host::setting_bool("menusEnabled").unwrap_or(defaults.menus_enabled),
            toolbar_buttons_enabled: host::setting_bool("toolbarButtonsEnabled")
                .unwrap_or(defaults.toolbar_buttons_enabled),
            context_menu_enabled: host::setting_bool("contextMenuEnabled")
                .unwrap_or(defaults.context_menu_enabled),
            search_enabled: host::setting_bool("searchEnabled").unwrap_or(defaults.search_enabled),
            other_buttons_enabled: host::setting_bool("otherButtonsEnabled")
                .unwrap_or(defaults.other_buttons_enabled),
        }
    }

    /// Returns whether mouse/pointer tips are enabled for the given action source.
    pub fn is_source_enabled(&self, source: ActionSource) -> bool {
        match source {
            ActionSource::Keyboard => false,
            ActionSource::Menu => self.menus_enabled,
            ActionSource::Toolbar => self.toolbar_buttons_enabled,
            ActionSource::ContextMenu => self.context_menu_enabled,
            ActionSource::Search => self.search_enabled,
            _ => self.other_buttons_enabled,
        }
    }
}
