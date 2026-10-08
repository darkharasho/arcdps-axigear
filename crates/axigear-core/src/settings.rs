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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Code or link as pasted; empty = no comp.
    pub comp_input: String,
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            comp_input: String::new(),
            api_key: String::new(),
            severities: Severities::default(),
            badge: BadgeSettings::default(),
            hotkey: "Ctrl+Shift+G".into(),
            auto_update_check: true,
            debug_logging: false,
            picks: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
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
}
