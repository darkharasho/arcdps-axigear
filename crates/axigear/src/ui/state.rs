//! UI-thread-only state: what is open, edit buffers, and change detection.

use axigear_core::driver::{Command, SettingsPatch, UiSnapshot};
use axigear_core::settings::BadgeSettings;

#[derive(Default)]
pub struct UiState {
    pub checklist_open: bool,
    /// Badge window position when the mouse went down on it (click vs drag).
    pub press_pos: Option<[f32; 2]>,
    /// Last badge position we know the worker has.
    pub badge_pos: Option<[f32; 2]>,
    pub comp_input: String,
    pub api_key: String,
    /// Edit buffers above were filled from settings once.
    pub loaded: bool,
    /// Badge settings sent but not yet echoed back in a snapshot.
    pub badge_edit: Option<BadgeSettings>,
    pub scale_edit: Option<f32>,
    pub hotkey_seen: Option<String>,
    pub update_kicked: bool,
}

impl UiState {
    pub fn sync(&mut self, snap: &UiSnapshot) {
        if !self.loaded {
            self.comp_input = snap.settings.comp_input.clone();
            self.api_key = snap.settings.api_key.clone();
            self.badge_pos = snap.settings.badge.pos;
            self.loaded = true;
        }
        if self.badge_edit.as_ref() == Some(&snap.settings.badge) {
            self.badge_edit = None;
        }
        if self.hotkey_seen.as_deref() != Some(snap.settings.hotkey.as_str()) {
            crate::keys::set_hotkey(&snap.settings.hotkey);
            self.hotkey_seen = Some(snap.settings.hotkey.clone());
        }
        if crate::keys::take_toggle() {
            self.checklist_open = !self.checklist_open;
        }
        if let Some(hotkey) = crate::keys::take_bound() {
            crate::plugin::send(Command::Settings(SettingsPatch::Hotkey(hotkey)));
        }
        if !self.update_kicked {
            self.update_kicked = true;
            crate::updater::kick_check_on_load(snap.settings.auto_update_check);
        }
    }

    /// The badge settings to show: a pending edit wins over the snapshot.
    pub fn badge_settings(&self, snap: &UiSnapshot) -> BadgeSettings {
        self.badge_edit.clone().unwrap_or_else(|| snap.settings.badge.clone())
    }

    pub fn edit_badge(&mut self, b: BadgeSettings) {
        self.badge_edit = Some(b.clone());
        crate::plugin::send(Command::Settings(SettingsPatch::Badge(b)));
    }
}
