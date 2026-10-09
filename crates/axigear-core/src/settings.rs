//! `config.json` in `<addons>/axigear/`. Missing fields take defaults, so old
//! files keep working as settings are added.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::SlotRef;
use crate::report::Severities;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BadgeSettings {
    pub hide_in_combat: bool,
    pub lock_position: bool,
    /// Only show when the map's game mode matches the comp's.
    pub matching_mode_only: bool,
    pub scale: f32,
    pub pos: Option<[f32; 2]>,
}

impl Default for BadgeSettings {
    fn default() -> Self {
        BadgeSettings { hide_in_combat: true, lock_position: false, matching_mode_only: false, scale: 1.0, pos: None }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedComp {
    /// Exactly what was loaded (trimmed): a code or a link.
    pub input: String,
    /// Comp name from the last successful load; "" until then.
    pub name: String,
}

impl SavedComp {
    /// The name, or the first 24 characters of the input when no name is known yet.
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        match self.input.char_indices().nth(24) {
            Some((i, _)) => format!("{}…", &self.input[..i]),
            None => self.input.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// v0.1.x single comp; read once and moved into `comps`, never written.
    #[serde(skip_serializing)]
    pub comp_input: String,
    /// Saved comps, newest first.
    pub comps: Vec<SavedComp>,
    /// `input` of the comp in use.
    pub active_comp: Option<String>,
    /// GW2 API key, plain text (documented in the README).
    pub api_key: String,
    pub severities: Severities,
    pub badge: BadgeSettings,
    pub hotkey: String,
    pub auto_update_check: bool,
    /// Log self buff/cast events to arcdps.log (for verifying buff IDs).
    pub debug_logging: bool,
    /// Manual slot picks keyed by `session::pick_key(comp, character)`.
    pub picks: BTreeMap<String, SlotRef>,
    /// Which tab the loadout window shows.
    pub loadout_tab: crate::report::Tab,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            comp_input: String::new(),
            comps: Vec::new(),
            active_comp: None,
            api_key: String::new(),
            severities: Severities::default(),
            badge: BadgeSettings::default(),
            hotkey: "Ctrl+Shift+G".into(),
            auto_update_check: true,
            debug_logging: false,
            picks: BTreeMap::new(),
            loadout_tab: crate::report::Tab::Build,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        let mut s: Settings = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let legacy = std::mem::take(&mut s.comp_input);
        let legacy = legacy.trim();
        if !legacy.is_empty() && s.comps.is_empty() {
            s.comps.push(SavedComp { input: legacy.to_string(), name: String::new() });
            s.active_comp = Some(legacy.to_string());
        }
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::fsutil::write_atomic(path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Category, SeveritySetting};

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert!(s.badge.hide_in_combat);
        assert_eq!(s.hotkey, "Ctrl+Shift+G");
        assert!(s.auto_update_check);
        assert_eq!(s.severities.get(Category::Infusions), SeveritySetting::Advisory);
    }

    #[test]
    fn loadout_tab_defaults_to_build() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.loadout_tab, crate::report::Tab::Build);
    }

    #[test]
    fn missing_or_broken_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, "{nope").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn partial_files_keep_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"api_key":"K","badge":{"lock_position":true}}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.api_key, "K");
        assert!(s.badge.lock_position && s.badge.hide_in_combat);
        assert_eq!(s.hotkey, "Ctrl+Shift+G");
    }

    #[test]
    fn save_round_trips_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("axigear/config.json");
        let mut s = Settings::default();
        s.picks.insert("code:1|Tester".into(), SlotRef { line: 1, slot: 0, build: 2 });
        s.severities.set(Category::Food, SeveritySetting::Off);
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        assert!(!dir.path().join("axigear/config.tmp").exists());
    }

    #[test]
    fn old_comp_input_migrates_to_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"comp_input":"CODE1"}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.comps, vec![SavedComp { input: "CODE1".into(), name: String::new() }]);
        assert_eq!(s.active_comp.as_deref(), Some("CODE1"));
        assert!(s.comp_input.is_empty());
        s.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("comp_input"), "{text}");
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn migration_does_not_clobber_existing_comps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"comp_input":"OLD","comps":[{"input":"NEW","name":"N"}],"active_comp":"NEW"}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.comps.len(), 1);
        assert_eq!(s.comps[0].input, "NEW");
        assert_eq!(s.active_comp.as_deref(), Some("NEW"));
    }

    #[test]
    fn display_name_falls_back_to_a_short_input() {
        let named = SavedComp { input: "x".into(), name: "Tuesday".into() };
        assert_eq!(named.display_name(), "Tuesday");
        let long = SavedComp { input: "https://someone.github.io/axibuilds/?c=abc".into(), name: String::new() };
        assert_eq!(long.display_name(), "https://someone.github.i…");
        let short = SavedComp { input: "abc".into(), name: String::new() };
        assert_eq!(short.display_name(), "abc");
    }
}
