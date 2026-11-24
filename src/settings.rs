//! Persistent app settings — the subset of the Zustand store marked
//! `partialize` in `src/lib/store.ts`, saved to the user's config dir.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent_projects: Vec<RecentProject>,
    pub shell_log_visible: bool,
    pub selected_port: String,
    pub repl_visible: bool,
    /// Combined bottom panel: 'shell' | 'repl' | 'hidden'
    pub bottom_mode: BottomMode,
    pub auto_connect: bool,
    pub last_port_path: String,
    pub last_profile_id: String,
    /// Manual board choices keyed by a port's serial number (or path).
    pub board_overrides: std::collections::HashMap<String, String>,
    /// Replacement `arduino-cli` FQBNs, keyed by catalog board id. Cores
    /// rename boards between releases, so the catalog's value is a default
    /// the user can correct without waiting for an Anode update.
    pub fqbn_overrides: std::collections::HashMap<String, String>,
    pub recent_boards: Vec<crate::hardware::RecentBoard>,
    pub panel_layout: Option<gpui_kit::component::dock::DockAreaState>,
    /// Local agent transcripts. ACP credentials, sessions, grants, and tool
    /// output handles stay with each provider and are never written here.
    pub agent_chats: Vec<crate::agent::SavedChat>,
    /// The provider the user last chose. Restored only while it is still
    /// configured and installed; Anode never picks one on its own.
    pub(crate) agent_provider: Option<String>,
    /// Start the chosen agent when a project opens.
    pub agent_auto_connect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub last_opened: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BottomMode {
    Shell,
    Repl,
    #[default]
    Hidden,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            recent_projects: Vec::new(),
            shell_log_visible: true,
            selected_port: String::new(),
            repl_visible: false,
            bottom_mode: BottomMode::Shell,
            auto_connect: true,
            last_port_path: String::new(),
            last_profile_id: String::new(),
            board_overrides: Default::default(),
            fqbn_overrides: Default::default(),
            recent_boards: Vec::new(),
            panel_layout: None,
            agent_chats: Vec::new(),
            agent_provider: None,
            agent_auto_connect: true,
        }
    }
}

impl Settings {
    pub fn add_recent_project(&mut self, path: impl Into<String>, name: impl Into<String>) {
        let path = path.into();
        let name = name.into();
        self.recent_projects.retain(|p| p.path != path);
        self.recent_projects.insert(
            0,
            RecentProject {
                path,
                name,
                last_opened: now_millis(),
            },
        );
        // Keep only the 10 most recent.
        self.recent_projects.truncate(10);
    }

    pub fn remove_recent_project(&mut self, path: &str) {
        self.recent_projects.retain(|p| p.path != path);
    }
}

pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("ANODE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|dir| dir.join("anode")))
        .map(|dir| dir.join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_era_settings_keep_defaults_for_panels_and_usb_history() {
        let settings: Settings = serde_json::from_str(r#"{"auto_connect":false,"last_port_path":"/dev/cu.old","board_overrides":{"usb123":"m5stamp-s3"}}"#).unwrap();
        assert!(!settings.auto_connect);
        assert_eq!(settings.last_port_path, "/dev/cu.old");
        assert_eq!(settings.board_overrides["usb123"], "m5stamp-s3");
        assert!(settings.recent_boards.is_empty());
        assert!(settings.panel_layout.is_none());
    }

    #[test]
    fn recent_projects_are_capped_and_deduped() {
        let mut s = Settings::default();
        for i in 0..12 {
            s.add_recent_project(format!("/p/{i}"), format!("p{i}"));
        }
        assert_eq!(s.recent_projects.len(), 10);
        assert_eq!(s.recent_projects[0].path, "/p/11");

        s.add_recent_project("/p/3", "p3");
        assert_eq!(s.recent_projects.len(), 10);
        assert_eq!(s.recent_projects[0].path, "/p/3");
        // The duplicate moved to the front instead of appearing twice.
        assert_eq!(
            s.recent_projects
                .iter()
                .filter(|p| p.path == "/p/3")
                .count(),
            1
        );

        s.remove_recent_project("/p/3");
        assert!(!s.recent_projects.iter().any(|p| p.path == "/p/3"));
    }
}
