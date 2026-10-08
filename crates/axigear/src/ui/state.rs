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
