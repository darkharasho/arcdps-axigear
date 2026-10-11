//! Failing and warning checks, one line each; click jumps to the slot,
//! the leading button dismisses it for this comp.

use arcdps::imgui::Ui;
use axigear_core::driver::{Command, SettingsPatch};
use axigear_core::report::{CheckReport, Status, Tone};

use super::focus::{self, Focus, PULSE_SECS};
use super::state::UiState;
use super::{icons, theme};
use crate::plugin::send;

const DISMISSED: &str = "axigear-dismissed";

fn dismiss(report: &CheckReport, id: &str, dismissed: bool) {
    send(Command::Settings(SettingsPatch::Dismiss { comp_key: report.comp_key.clone(), id: id.to_string(), dismissed }));
}

pub fn render(ui: &Ui, report: &CheckReport, state: &mut UiState) {
    let line = ui.text_line_height();
    let mut problems: Vec<_> = report
        .results
        .iter()
        .filter(|r| matches!(r.tone(), Tone::Danger | Tone::Warn))
        .collect();
    problems.sort_by_key(|r| r.tone().rank());
    let unknown: Vec<_> = report
        .results
        .iter()
        .filter(|r| r.status == Status::Unknown)
        .collect();
    // Nothing decided yet (no key, no live data): "All 0 checks pass" would
    // sit oddly above "N waiting for data", so say nothing.
    if problems.is_empty() && report.summary().decided() > 0 {
        icons::draw(ui, Tone::Ok, line);
        ui.same_line();
        ui.text_colored(
            theme::TEXT_DIM,
            format!("All {} checks pass", report.summary().decided()),
        );
    }
    for (i, r) in problems.iter().enumerate() {
        icons::draw(ui, r.tone(), line);
        ui.same_line();
        if ui.small_button(format!("\u{00d7}##dismiss{i}")) {
            dismiss(report, &r.id, true);
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Dismiss for this comp");
        }
        ui.same_line();
        let clicked = ui.selectable(format!("{}  ##problem{i}", r.label));
        ui.same_line();
        ui.text_colored(theme::TEXT_DIM, r.detail());
        if clicked {
            if let Some(key) = focus::target(r) {
                let tab = key.tab();
                state.tab = Some(tab);
                send(Command::Settings(SettingsPatch::LoadoutTab(tab)));
                state.focus = Some(Focus {
                    key,
                    until: ui.time() + PULSE_SECS,
                });
            }
        }
    }
    if !unknown.is_empty() {
        icons::draw(ui, Tone::Neutral, line);
        ui.same_line();
        ui.text_colored(
            theme::TEXT_FAINT,
            format!("{} waiting for data", unknown.len()),
        );
        if ui.is_item_hovered() {
            ui.tooltip(|| {
                for r in &unknown {
                    ui.text_colored(theme::TEXT_DIM, format!("{} · {}", r.label, r.detail()));
                }
            });
        }
    }
    if !report.dismissed.is_empty() {
        icons::draw(ui, Tone::Neutral, line);
        ui.same_line();
        if ui.selectable(format!("{} dismissed##dismissed", report.dismissed.len())) {
            ui.open_popup(DISMISSED);
        }
        ui.popup(DISMISSED, || {
            for (i, r) in report.dismissed.iter().enumerate() {
                if ui.small_button(format!("Restore##restore{i}")) {
                    dismiss(report, &r.id, false);
                }
                ui.same_line();
                ui.text(&r.label);
                ui.same_line();
                ui.text_colored(theme::TEXT_DIM, r.detail());
            }
        });
    }
}
