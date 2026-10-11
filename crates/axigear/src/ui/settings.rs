//! The arcdps options tab: comp, API key, severities, badge, hotkey, updates.

use arcdps::imgui::Ui;
use axigear_core::driver::{Command, SettingsPatch, UiSnapshot};
use axigear_core::report::{Category, SeveritySetting};
use axigear_core::settings::BadgeSettings;

use super::state::UiState;
use super::theme;
use crate::plugin::send;

fn heading(ui: &Ui, text: &str) {
    ui.spacing();
    ui.text_colored(theme::TEXT_FAINT, text);
}

pub fn render(ui: &Ui, snap: &UiSnapshot, state: &mut UiState) {
    let s = &snap.settings;

    if let Some(e) = &snap.save_error {
        ui.text_colored(theme::WARN, e);
    }

    heading(ui, "COMP");
    ui.input_text_multiline("##axigear-comp", &mut state.comp_input, [420.0, 60.0]).build();
    if ui.button("Load") {
        send(Command::LoadInput(state.comp_input.clone()));
        state.comp_input.clear();
    }
    if let Some(e) = &snap.load_error {
        ui.text_colored(theme::DANGER, e);
    }
    ui.text_colored(theme::TEXT_FAINT, "Paste an AxiForge comp or build code, or a published comp link.");
    if snap.comps.is_empty() {
        ui.text_colored(theme::TEXT_FAINT, "No saved comps.");
    }
    for row in &snap.comps {
        let _id = ui.push_id(row.input.as_str());
        ui.text_colored(if row.active { theme::GOLD } else { theme::TEXT }, &row.name);
        ui.same_line();
        ui.text_colored(theme::TEXT_FAINT, &row.source);
        if row.active {
            ui.same_line();
            ui.text_colored(theme::GOLD, "· in use");
        } else {
            ui.same_line();
            if ui.small_button("Use") {
                send(Command::UseComp(row.input.clone()));
            }
        }
        ui.same_line();
        if ui.small_button("Refresh") {
            send(Command::RefreshComp(row.input.clone()));
        }
        ui.same_line();
        if ui.small_button("Unsubscribe") {
            send(Command::Unsubscribe(row.input.clone()));
        }
        if let Some(e) = &row.error {
            ui.text_colored(theme::WARN, e);
        }
    }

    heading(ui, "GW2 API KEY");
    ui.set_next_item_width(320.0);
    ui.input_text("##axigear-key", &mut state.api_key).password(true).build();
    if ui.is_item_deactivated_after_edit() {
        for c in super::key_edit::commit_commands(&state.api_key, &s.api_key, false) {
            send(c);
        }
    }
    ui.same_line();
    if ui.button("Test##key") {
        for c in super::key_edit::commit_commands(&state.api_key, &s.api_key, true) {
            send(c);
        }
    }
    if let Some((line, ok)) = super::key_edit::status(&state.api_key, &s.api_key, snap.key_test.as_deref()) {
        ui.text_colored(if ok { theme::OK } else { theme::WARN }, line);
    }
    ui.text_colored(theme::TEXT_FAINT, "Saved when you leave the field. Needs the characters and builds permissions. Stored in plain text in addons/axigear/config.json.");

    heading(ui, "SEVERITY");
    let labels: Vec<&str> = SeveritySetting::ALL.iter().map(|x| x.label()).collect();
    for cat in Category::ALL {
        let mut idx = SeveritySetting::ALL.iter().position(|x| *x == s.severities.get(cat)).unwrap_or(0);
        ui.set_next_item_width(120.0);
        if ui.combo_simple_string(format!("{}##sev", cat.label()), &mut idx, &labels) {
            send(Command::Settings(SettingsPatch::Severity(cat, SeveritySetting::ALL[idx])));
        }
    }

    heading(ui, "BADGE");
    let mut b: BadgeSettings = state.badge_settings(snap);
    let mut shown = !b.hidden;
    let mut changed = ui.checkbox("Show badge", &mut shown);
    b.hidden = !shown;
    if b.hidden {
        ui.same_line();
        ui.text_colored(theme::TEXT_FAINT, "open the checklist with the hotkey or arcdps's window list");
    }
    changed |= ui.checkbox("Hide in combat", &mut b.hide_in_combat);
    changed |= ui.checkbox("Lock position", &mut b.lock_position);
    changed |= ui.checkbox("Only show in the comp's game mode", &mut b.matching_mode_only);
    if changed {
        state.edit_badge(b.clone());
    }
    let mut scale = super::clamp_scale(state.scale_edit.unwrap_or(b.scale));
    ui.set_next_item_width(160.0);
    if ui.slider("Scale", super::SCALE_MIN, super::SCALE_MAX, &mut scale) {
        state.scale_edit = Some(scale);
    }
    if ui.is_item_deactivated_after_edit() {
        state.scale_edit = None;
        state.edit_badge(BadgeSettings { scale: super::clamp_scale(scale), ..b });
    }

    heading(ui, "CHECKLIST HOTKEY");
    let label = if crate::keys::binding() {
        "Press a key...".to_string()
    } else if s.hotkey.is_empty() {
        "(unbound)".to_string()
    } else {
        s.hotkey.clone()
    };
    ui.text(&label);
    ui.same_line();
    if crate::keys::binding() {
        if ui.small_button("Cancel##hotkey") {
            crate::keys::cancel_binding();
        }
    } else if ui.small_button("Set##hotkey") {
        crate::keys::start_binding();
    }
    ui.same_line();
    if ui.small_button("Clear##hotkey") {
        crate::keys::cancel_binding();
        send(Command::Settings(SettingsPatch::Hotkey(String::new())));
    }

    heading(ui, "UPDATES");
    let mut auto = s.auto_update_check;
    if ui.checkbox("Check for updates on startup", &mut auto) {
        send(Command::Settings(SettingsPatch::AutoUpdate(auto)));
    }
    update_pill(ui);
    let mut debug = s.debug_logging;
    if ui.checkbox("Log self buff and skill events to arcdps.log (debug)", &mut debug) {
        send(Command::Settings(SettingsPatch::DebugLogging(debug)));
    }
}

/// Same states and wording as arcdps-axipulse's update pill.
fn update_pill(ui: &Ui) {
    use crate::updater::{dismiss_error, set_failed, snapshot, start_install, UpdateState};
    let st = snapshot();
    let (label, color) = match &st {
        UpdateState::Available { tag, .. } => (format!("Update available \u{00b7} {tag}"), theme::OK),
        UpdateState::Downloading { pct, .. } if pct.is_finite() => (format!("Downloading... {pct:.0}%"), theme::META),
        UpdateState::Downloading { .. } => ("Downloading...".to_string(), theme::META),
        UpdateState::Installed { tag } => (format!("Restart GW2 to load {tag}"), theme::WARN),
        UpdateState::Failed { msg } => (format!("Update failed: {msg}"), theme::DANGER),
        _ => return,
    };
    ui.text_colored(color, &label);
    if let UpdateState::Available { .. } = &st {
        ui.same_line();
        if ui.small_button("Install") {
            match crate::paths::dll_dir() {
                Some(dir) => start_install(dir),
                None => set_failed("could not locate DLL directory"),
            }
        }
    }
    if let UpdateState::Failed { .. } = &st {
        ui.same_line();
        if ui.small_button("\u{00d7}##dismiss-update") {
            dismiss_error();
        }
    }
}
